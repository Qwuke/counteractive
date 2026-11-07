//! Step 1, and everything later steps share: the running services, the
//! wallet driven through `wallet-cli`, and direct clients for the few
//! checks the wallet can't make.

use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context as _, bail, ensure};
use bytes::Bytes;
use credit::Credential;
use gate_conn::{AttestedConn, DialScope, Target, TcpDialer, UnverifiedConn};
use http_body_util::{BodyExt as _, Empty, Full};
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use hyper_util::rt::TokioIo;
use manifest::Manifest;
use proto::Receipt;
use proto::gate::GateState;
use proto::openai::{ChatCompletionRequest, ChatMessage, ChatRole};
use proto::storefront::StorefrontState;
use proto::walletd::{BalanceView, ReconcileReport, WalletCloseResponse};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpStream;
use zeroize::Zeroizing;

use super::{ok, tier2};
use crate::keys::Keys;
use crate::process::{Binaries, Service};
use crate::tree::Tree;
use crate::{operator, release, stack};

/// The key the storefront gets for fake-stripe.
const FAKE_STRIPE_KEY: &str = "sk_test_e2e";

/// What `wallet-cli --json chat` prints.
#[derive(Debug, Deserialize)]
pub struct ChatResult {
    /// The model's text.
    pub text: String,
    /// The gate's receipt for the turn.
    pub receipt: Option<Receipt>,
    /// Why walletd rolled the conversation over to a new session, if it did.
    pub rollover: Option<String>,
    /// How a lost session settled, if walletd recovered one.
    pub recovered: Option<String>,
}

impl ChatResult {
    /// The receipt, which every finished turn has.
    pub fn receipt(&self) -> anyhow::Result<&Receipt> {
        self.receipt.as_ref().context("the turn ended without a receipt")
    }
}

/// One finished `wallet-cli` run.
pub struct CliOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// The running stack and the wallet.
pub struct World {
    binaries: Binaries,
    pub tree: Tree,
    pub keys: Keys,
    engine: Service,
    fake_stripe: Service,
    pub fake_stripe_addr: SocketAddr,
    gate: Option<Service>,
    /// C tor, when the gate is also an onion service (tier 2).
    tor: Option<Service>,
    storefront: Service,
    pub storefront_target: Target,
    walletd: Option<Service>,
    /// The manifest the gate serves under, signed in step 1.
    pub manifest: Manifest,
    /// The idle timer of `release-idle.json`, for step 9.
    pub idle_seconds: u64,
    /// Credits the wallet has been charged so far, by receipts.
    pub spent: u64,
    /// The credential step 4 bought outside the wallet, for step 7.
    pub credential: Option<Credential>,
    /// Owns the temp dir; dropped last.
    dir: tempfile::TempDir,
}

/// How [`World::bring_up_with`] sets the stack up.
pub struct Options<'a> {
    /// C tor, to put the gate behind an onion service (tier 2).
    pub tor: Option<&'a Path>,
    /// Where Checkout sends a buyer who paid; walletd's `/paid` when `None`.
    pub success_url: Option<&'a str>,
    /// A pause before each token fake-engine streams, in milliseconds.
    pub token_delay_ms: u64,
    /// Takes each line the bring-up reports.
    pub report: &'a (dyn Fn(String) + Sync),
}

impl World {
    /// Step 1: keys, releases, every service, unlock and configure. With
    /// `tor`, C tor first puts the gate's socket behind an onion service,
    /// which the manifest and the wallet's pins then name.
    pub async fn bring_up(binaries: &Binaries, tor: Option<&Path>) -> anyhow::Result<Self> {
        let options = Options { tor, success_url: None, token_delay_ms: 0, report: &|line| ok(line) };
        Self::bring_up_with(binaries, &options).await
    }

    /// [`World::bring_up`], with the options the demo changes.
    pub async fn bring_up_with(binaries: &Binaries, options: &Options<'_>) -> anyhow::Result<Self> {
        let report = options.report;
        // Short, so socket paths stay under the platform's limit.
        let dir = tempfile::Builder::new().prefix("ca-e2e-").tempdir_in(short_temp_root())?;
        let tree = Tree::new(dir.path());
        let keys = Keys::generate(&tree)?;
        let idle_seconds = if options.tor.is_some() { tier2::IDLE_SECONDS } else { release::SHORT_IDLE_SECONDS };
        release::write(&tree, &keys, Some(idle_seconds))?;
        report(format!("keygen and release in {}: release.json and release-idle.json", dir.path().display()));
        let tor = match options.tor {
            Some(program) => {
                let service = stack::tor(program, &tree).await?;
                report(format!("C tor started; the gate's onion service is {}", tree.onion()?.unwrap_or_default()));
                Some(service)
            }
            None => None,
        };

        let engine = stack::engine(binaries, &tree, options.token_delay_ms).await?;
        let (fake_stripe, fake_stripe_addr) = stack::fake_stripe(binaries, &tree, None).await?;
        let gate = stack::gate(binaries, &tree, &tree.release()).await?;
        let stripe_base_url = format!("http://{fake_stripe_addr}");
        let (storefront, storefront_addr) =
            stack::storefront(binaries, &tree, None, &stripe_base_url, options.success_url).await?;
        report(format!(
            "fake-engine, fake-stripe ({fake_stripe_addr}), gate and storefront ({storefront_addr}) started"
        ));

        ensure!(operator::gate_health(&tree).await? == GateState::Locked, "a fresh gate must start Locked");
        let signed = operator::unlock_gate(&tree, &keys).await?;
        let manifest = manifest::verify_signed(&signed, &keys.operator_public_key()?)?;
        report(format!(
            "bootstrap: attested the gate, unlocked it with K, signed its new issuer key into manifest v{} \
             listing {} releases, posted it",
            manifest.version,
            manifest.releases.len()
        ));
        let storefront_target: Target = format!("tcp:{storefront_addr}").parse()?;
        operator::configure_storefront(&tree, &keys, &storefront_target, FAKE_STRIPE_KEY.into()).await?;

        ensure!(operator::gate_health(&tree).await? == GateState::Ready, "the gate is not ready");
        report("gate health: ready".into());
        ensure!(
            operator::storefront_health(&storefront_target).await? == StorefrontState::Ready,
            "the storefront is not ready"
        );
        report("storefront health: ready".into());
        stack::write_wallet_pins(&tree, &keys, storefront_addr)?;
        report("wallet.toml written with the pins".into());

        Ok(Self {
            binaries: binaries.clone(),
            tree,
            keys,
            engine,
            fake_stripe,
            fake_stripe_addr,
            gate: Some(gate),
            tor,
            storefront,
            storefront_target,
            walletd: None,
            manifest,
            idle_seconds,
            spent: 0,
            credential: None,
            dir,
        })
    }

    /// An attested connection to the gate, reached as the wallet reaches
    /// it: on its Unix socket, or through its onion service in tier 2.
    pub async fn gate_conn(&self) -> anyhow::Result<AttestedConn> {
        if self.over_tor() {
            tier2::attest_through_tor(&self.tree, &self.manifest).await
        } else {
            operator::attest_gate(&self.tree, &self.manifest).await
        }
    }

    /// Whether the wallet reaches the gate through Tor (tier 2), where
    /// every call to it takes round trips through relays.
    pub fn over_tor(&self) -> bool {
        self.tor.is_some()
    }

    /// An attested connection to the storefront.
    pub async fn storefront_conn(&self) -> anyhow::Result<AttestedConn> {
        let conn =
            UnverifiedConn::connect(&TcpDialer, &self.storefront_target, attest::Role::Storefront, &DialScope::new())
                .await?;
        Ok(conn.attest(&attest::PlatformVerifier::new(true), &self.manifest).await?)
    }

    /// Marks a Checkout Session paid, as a buyer pressing Pay would.
    pub async fn pay(&self, order_id: &str) -> anyhow::Result<()> {
        let stream = TcpStream::connect(self.fake_stripe_addr).await?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(connection);
        let request = hyper::Request::post(format!("/test/pay/{order_id}"))
            .header(HOST, self.fake_stripe_addr.to_string())
            .body(Empty::<Bytes>::new())?;
        let response = sender.send_request(request).await?;
        let status = response.status();
        response.into_body().collect().await?;
        ensure!(status.is_success(), "fake-stripe answered {status} to the test payment");
        Ok(())
    }

    /// Starts walletd for the wallet.
    pub async fn start_walletd(&mut self) -> anyhow::Result<SocketAddr> {
        ensure!(self.walletd.is_none(), "walletd is already running");
        let (service, addr) = stack::walletd(&self.binaries, &self.tree).await?;
        self.walletd = Some(service);
        Ok(addr)
    }

    /// Stops walletd with SIGTERM, letting it close its sessions.
    pub async fn stop_walletd(&mut self) -> anyhow::Result<()> {
        self.walletd.take().context("walletd is not running")?.stop().await
    }

    /// Stops the gate: SIGKILL when `crash`, SIGTERM otherwise.
    pub async fn stop_gate(&mut self, crash: bool) -> anyhow::Result<()> {
        let gate = self.gate.take().context("the gate is not running")?;
        if crash { gate.kill() } else { gate.stop().await }
    }

    /// Starts the gate on `release` and unlocks it.
    pub async fn start_gate(&mut self, release: &Path) -> anyhow::Result<()> {
        ensure!(self.gate.is_none(), "the gate is already running");
        self.gate = Some(stack::gate(&self.binaries, &self.tree, release).await?);
        operator::unlock_gate(&self.tree, &self.keys).await?;
        ensure!(operator::gate_health(&self.tree).await? == GateState::Ready, "the restarted gate is not ready");
        Ok(())
    }

    /// Runs `wallet-cli` with these arguments and `stdin`.
    pub async fn cli(&self, args: &[&str], stdin: Option<&str>) -> anyhow::Result<CliOutput> {
        self.wallet_cli().run(args, stdin).await
    }

    /// The wallet's `wallet-cli`, to run without borrowing the world.
    pub fn wallet_cli(&self) -> WalletCli {
        WalletCli::new(&self.binaries, self.tree.wallet())
    }

    /// Runs `wallet-cli` and requires it to succeed, returning stdout.
    pub async fn cli_ok(&self, args: &[&str]) -> anyhow::Result<String> {
        let output = self.cli(args, None).await?;
        if output.code != Some(0) {
            bail!("wallet-cli {} failed ({:?}): {}", args.join(" "), output.code, output.stderr.trim());
        }
        Ok(output.stdout)
    }

    /// Runs `wallet-cli --json` and decodes its answer.
    async fn cli_json<T: DeserializeOwned>(&self, args: &[&str], stdin: Option<&str>) -> anyhow::Result<T> {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let output = self.cli(&all, stdin).await?;
        if output.code != Some(0) {
            bail!("wallet-cli {} failed ({:?}): {}", args.join(" "), output.code, output.stderr.trim());
        }
        serde_json::from_str(&output.stdout).with_context(|| format!("decoding `wallet-cli {}`", args.join(" ")))
    }

    /// One chat turn in `conversation`, through walletd. The prompt goes
    /// on stdin, which takes prompts larger than one argument may be.
    pub async fn chat(&self, conversation: &str, model: &str, prompt: &str) -> anyhow::Result<ChatResult> {
        self.cli_json(&["chat", "--model", model, "--conversation", conversation, "-"], Some(prompt)).await
    }

    /// The same turn, expected to fail; returns the error text.
    pub async fn chat_error(&self, conversation: &str, model: &str, prompt: &str) -> anyhow::Result<String> {
        let args = ["--json", "chat", "--model", model, "--conversation", conversation, "-"];
        let output = self.cli(&args, Some(prompt)).await?;
        ensure!(output.code == Some(1), "expected the turn to fail, got exit {:?}: {}", output.code, output.stdout);
        Ok(output.stderr)
    }

    /// The wallet's balance view.
    pub async fn balance(&self) -> anyhow::Result<BalanceView> {
        self.cli_json(&["balance"], None).await
    }

    /// Closes one conversation's session, or all.
    pub async fn close(&self, conversation: Option<&str>) -> anyhow::Result<WalletCloseResponse> {
        match conversation {
            Some(id) => self.cli_json(&["close", "--conversation", id], None).await,
            None => self.cli_json(&["close"], None).await,
        }
    }

    /// Runs reconcile.
    pub async fn reconcile(&self) -> anyhow::Result<ReconcileReport> {
        self.cli_json(&["reconcile"], None).await
    }

    /// Sends a one-off, non-streaming chat straight to walletd's API and
    /// hangs up after `patience`, as an OpenAI client that times out does.
    /// Fails if walletd answered first, since then nothing was abandoned.
    pub async fn abandon_chat(&self, model: &str, prompt: &str, patience: Duration) -> anyhow::Result<()> {
        let wallet = self.tree.wallet();
        let addr: SocketAddr = std::fs::read_to_string(wallet.join("walletd.addr"))?.trim().parse()?;
        let token = Zeroizing::new(std::fs::read_to_string(wallet.join("walletd.token"))?);
        let authorization = Zeroizing::new(format!("Bearer {}", token.trim()));
        let body = ChatCompletionRequest {
            model: model.into(),
            messages: vec![ChatMessage::new(ChatRole::User, prompt)],
            max_completion_tokens: None,
            max_tokens: None,
            stream: Some(false),
        };
        let request = hyper::Request::post("/v1/chat/completions")
            .header(HOST, addr.to_string())
            .header(AUTHORIZATION, authorization.as_str())
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(serde_json::to_vec(&body)?)))?;
        let stream = TcpStream::connect(addr).await?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        let connection = tokio::spawn(connection);
        let answered = tokio::time::timeout(patience, sender.send_request(request)).await;
        // Ending the connection's task closes the socket: the client is gone.
        connection.abort();
        ensure!(answered.is_err(), "walletd answered within {patience:?}, so the client never gave up");
        Ok(())
    }

    /// The wallet's expected total: the purchase minus every charge.
    pub fn expected_balance(&self) -> u64 {
        release::USD_5_CREDITS - self.spent
    }

    /// Checks the balance is exactly the purchase minus every charge.
    pub async fn check_balance(&self) -> anyhow::Result<u64> {
        let view = self.balance().await?;
        ensure!(
            view.total_credits == self.expected_balance(),
            "balance {} != 5,000,000 - charges {} = {}",
            view.total_credits,
            self.spent,
            self.expected_balance()
        );
        ensure!(view.chains.iter().all(|chain| chain.resolved), "a chain is unresolved: {:?}", view.chains);
        Ok(view.total_credits)
    }

    /// Stops every service, then removes the temp dir unless the run
    /// failed. Returns the kept dir.
    pub async fn finish(mut self, passed: bool) -> Option<PathBuf> {
        let mut services: Vec<Service> = Vec::new();
        services.extend(self.walletd.take());
        services.push(self.storefront);
        services.extend(self.gate.take());
        services.extend(self.tor.take());
        services.push(self.fake_stripe);
        services.push(self.engine);
        for service in services {
            let name = service.name();
            if let Err(error) = service.stop().await {
                eprintln!("stopping {name}: {error:#}");
            }
        }
        if passed { None } else { Some(self.dir.keep()) }
    }
}

/// The built `wallet-cli`, run on one wallet directory.
#[derive(Debug, Clone)]
pub struct WalletCli {
    program: PathBuf,
    wallet: PathBuf,
}

impl WalletCli {
    /// The built `wallet-cli` on the wallet in `wallet`.
    pub fn new(binaries: &Binaries, wallet: PathBuf) -> Self {
        Self { program: binaries.path("wallet-cli"), wallet }
    }

    /// Runs it with these arguments and `stdin`.
    pub async fn run(&self, args: &[&str], stdin: Option<&str>) -> anyhow::Result<CliOutput> {
        let mut command = tokio::process::Command::new(&self.program);
        command.arg("--wallet-dir").arg(&self.wallet).args(args.iter().map(OsStr::new));
        command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        let mut child = command.spawn().context("starting wallet-cli")?;
        if let Some(text) = stdin {
            let mut pipe = child.stdin.take().context("no stdin pipe")?;
            pipe.write_all(text.as_bytes()).await?;
        }
        let output = child.wait_with_output().await?;
        Ok(CliOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    /// The wallet directory, where walletd writes its address and token.
    pub fn wallet(&self) -> &Path {
        &self.wallet
    }
}

/// Where e2e makes its temp dir: `/tmp` when it exists, because macOS's
/// per-user temp path is long and Unix socket paths are limited to about
/// 104 bytes.
pub fn short_temp_root() -> PathBuf {
    let tmp = Path::new("/tmp");
    if tmp.is_dir() { tmp.to_owned() } else { std::env::temp_dir() }
}
