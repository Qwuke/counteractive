//! `xtask demo --remote`: the console for the stack `xtask remote deploy`
//! started on the GPU instance.
//!
//! Only walletd and wallet-cli run here, on a fresh wallet in a temp dir.
//! The bring-up opens the SSH tunnel, unlocks the gate and configures the
//! storefront if they need it (as `remote unlock` does), and follows the
//! instance's logs into the activity log. The purchase plane goes through
//! the tunnel; chat goes over Tor to the instance's onion service, never
//! through the tunnel.

use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;

use anyhow::Context as _;

use super::console::{self, Component, Console, Kind, RemoteInfo};
use crate::e2e::world::{WalletCli, short_temp_root};
use crate::keys::Keys;
use crate::process::{Binaries, Service};
use crate::remote::transport::Tunnel;
use crate::remote::{self, Instance, RemoteConfig, Transport};
use crate::stack;
use crate::tree::Tree;

/// The instance's service logs the activity log follows, besides the
/// engine's.
const INSTANCE_LOGS: [&str; 4] = ["gate", "storefront", "fake-stripe", "tor"];

/// What runs here for the remote demo, and the tunnel to the instance.
pub struct RemoteStack {
    /// fake-stripe, through the tunnel.
    pub fake_stripe_addr: SocketAddr,
    walletd: Option<Service>,
    /// `tail -F` of the instance's logs.
    tails: Vec<Child>,
    /// Closed when the stack is dropped, after walletd has stopped.
    _tunnel: Tunnel,
    /// The wallet and the logs; removed unless kept.
    dir: tempfile::TempDir,
}

impl RemoteStack {
    /// Stops walletd and the log tails and closes the tunnel, then removes
    /// the temp dir unless the run failed. Returns the kept dir. The
    /// instance's services keep running for the next demo.
    pub async fn finish(mut self, passed: bool) -> Option<PathBuf> {
        if let Some(walletd) = self.walletd.take()
            && let Err(error) = walletd.stop().await
        {
            eprintln!("stopping walletd: {error:#}");
        }
        for mut tail in self.tails.drain(..) {
            let _ = tail.kill();
            let _ = tail.wait();
        }
        if passed { None } else { Some(self.dir.keep()) }
    }
}

/// Reaches the instance's stack, makes it ready, and starts a wallet here
/// that chats with it over Tor.
pub async fn bring_up(console: &Arc<Console>, binaries: &Binaries, tree: &Tree) -> anyhow::Result<RemoteStack> {
    let config = RemoteConfig::read(tree)?;
    let instance = config.instance()?.clone();
    let keys = Keys::read(tree)?;
    let onion = tree.onion()?.context("the tree has no onion address: run `xtask remote deploy`")?;
    console.event(
        "xtask",
        Kind::Info,
        format!("the services run on {}; this machine runs only walletd and wallet-cli", instance.describe()),
    );

    let transport = Transport::new(tree, instance.clone());
    let tunnel = transport.tunnel(tree, config.ports).await?;
    let ports = config.ports;
    console.event(
        "xtask",
        Kind::Ok,
        match instance {
            Instance::Ssh { .. } => format!(
                "SSH tunnel up: the instance's storefront ({}) and fake-stripe (http://{}) are forwarded here, and its \
                 gate socket for the operator",
                ports.storefront(),
                ports.fake_stripe()
            ),
            Instance::Local { .. } => "rehearsal: the storefront and fake-stripe listen here directly".to_owned(),
        },
    );
    let report = |line: String| console.event("xtask", Kind::Ok, line);
    remote::make_ready(tree, &keys, &config, &transport, &report).await?;
    let attestation = remote::fetch_attestation(tree, &transport)?;
    for line in attestation.lines().filter(|line| !line.trim().is_empty()) {
        console.event("instance", Kind::Info, line);
    }
    console.event(
        "xtask",
        Kind::Info,
        "the instance reported that about its TEE; the wallet doesn't verify it yet (the gate still attests \
         dev-insecure)",
    );

    let dir = tempfile::Builder::new().prefix("ca-demo-").tempdir_in(short_temp_root())?;
    let wallet = Tree::new(dir.path());
    let mut running =
        RemoteStack { fake_stripe_addr: ports.fake_stripe(), walletd: None, tails: Vec::new(), _tunnel: tunnel, dir };
    let started = async {
        let logs = wallet.logs();
        std::fs::create_dir_all(&logs)?;
        let engine_log = if matches!(instance, Instance::Local { .. }) { "fake-engine" } else { "vllm" };
        for name in INSTANCE_LOGS.into_iter().chain([engine_log]) {
            let out = OpenOptions::new().create(true).append(true).open(logs.join(format!("{name}.log")))?;
            running.tails.push(transport.tail(name, out)?);
        }
        console.start_tail(logs);

        stack::write_pins(&wallet.wallet(), &keys, stack::onion_target(&onion), ports.storefront())?;
        let remote = RemoteInfo { host: instance.describe(), engine: engine(&config, &instance) };
        let components = components(&config, &instance, &onion, &attestation);
        console::fill_view(console, &tree.release(), Some(&onion), components, Some(remote))?;
        let cli = WalletCli::new(binaries, wallet.wallet());
        console::init_wallet(console, &cli).await?;
        console.event("xtask", Kind::Info, "starting walletd: Arti bootstraps a Tor client first (up to 3 min)");
        let (walletd, addr) = stack::walletd_for(binaries, &wallet, true).await?;
        running.walletd = Some(walletd);
        console::link_wallet(console, cli, addr, "dials the instance's onion service through Arti")
    }
    .await;
    match started {
        Ok(()) => Ok(running),
        Err(error) => {
            if let Some(dir) = running.finish(false).await {
                console.event("xtask", Kind::Info, format!("the wallet and logs are kept in {}", dir.display()));
            }
            Err(error)
        }
    }
}

/// What answers chat on the instance, as a noun phrase whose first word
/// names the engine.
fn engine(config: &RemoteConfig, instance: &Instance) -> String {
    match instance {
        Instance::Ssh { .. } => format!("vLLM serving {} on the instance's GPU", config.model.weights),
        Instance::Local { .. } => "fake-engine standing in for vLLM in a rehearsal".to_owned(),
    }
}

/// The instance's services, for the page.
fn components(config: &RemoteConfig, instance: &Instance, onion: &str, attestation: &str) -> Vec<Component> {
    let ports = config.ports;
    let reported = attestation.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("; ");
    let engine_name = if matches!(instance, Instance::Local { .. }) { "fake-engine" } else { "vLLM" };
    vec![
        Component {
            name: "instance",
            plane: "both",
            detail: format!("{}: {reported} (as the instance reports it; not verified yet)", instance.describe()),
        },
        Component {
            name: "SSH tunnel",
            plane: "purchase",
            detail: "forwards the storefront, fake-stripe and the operator's gate socket; chat never uses it"
                .to_owned(),
        },
        Component {
            name: "C tor",
            plane: "use",
            detail: format!("on the instance: publishes {onion}:443, forwarding to the gate's Unix socket"),
        },
        Component {
            name: "gate",
            plane: "both",
            detail: "on the instance: issues credentials on issue.sock, meters sessions on gate.sock".to_owned(),
        },
        Component {
            name: engine_name,
            plane: "use",
            detail: format!("{}, as `{}` on engine.sock", engine(config, instance), config.model.id),
        },
        Component {
            name: "storefront",
            plane: "purchase",
            detail: format!(
                "on the instance, {} here through the tunnel: sells credit, never sees what it buys",
                ports.storefront()
            ),
        },
        Component {
            name: "fake-stripe",
            plane: "purchase",
            detail: format!(
                "on the instance, http://{} here through the tunnel: a test checkout; no card is charged",
                ports.fake_stripe()
            ),
        },
    ]
}
