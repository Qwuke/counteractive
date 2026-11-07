//! The demo's state: the stack's bring-up, what the page shows, the wallet
//! it drives, and the activity log it streams.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use manifest::ReleaseConfig;
use rand::TryRng as _;
use serde::Serialize;
use tokio::sync::{Notify, broadcast};
use zeroize::Zeroizing;

use crate::e2e::tier2;
use crate::e2e::world::{CliOutput, Options, WalletCli, World};
use crate::process::Binaries;

/// The pause before each token fake-engine streams, so the page shows a
/// reply arriving token by token, as from a real model.
pub const TOKEN_DELAY_MS: u64 = 40;
/// How often unpaid orders are asked about while any wait.
const COLLECT_POLL: Duration = Duration::from_secs(2);
/// How often the services' logs are read for new lines.
const TAIL_POLL: Duration = Duration::from_millis(500);
/// How often a slow wait for the onion service is reported.
const ONION_NOTE_EVERY: Duration = Duration::from_secs(30);
/// The longest log line shown, in characters.
const MAX_LOG_LINE: usize = 240;
/// Events kept for a page that connects late.
const EVENT_BACKLOG: usize = 4096;

/// What one activity-log entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A bring-up check that passed.
    Ok,
    Info,
    /// A command the console ran, as typed.
    Command,
    /// What a command printed.
    Output,
    Warn,
    Error,
}

/// One activity-log entry.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub seq: u64,
    /// Milliseconds since the demo started.
    pub at_ms: u64,
    pub source: String,
    pub kind: Kind,
    pub text: String,
}

/// Where the demo is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    Ready,
    Failed,
}

/// One running service, for the page's component list.
#[derive(Debug, Clone, Serialize)]
pub struct Component {
    pub name: &'static str,
    /// `purchase`, `use` or `both`: which plane it serves.
    pub plane: &'static str,
    pub detail: String,
}

/// A model and its per-token prices, from the release.
#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub id: String,
    /// The backend serving it: `fake` or `vllm`.
    pub engine: String,
    pub input_rate: u64,
    pub output_rate: u64,
}

/// Where the services run when they aren't on this machine.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteInfo {
    /// The instance, as SSH reaches it, or the rehearsal's directory.
    pub host: String,
    /// What answers chat there.
    pub engine: String,
}

/// A product the storefront sells, from the release.
#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub id: String,
    pub usd_cents: u64,
    pub credits: u64,
}

/// Where a purchase is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseStatus {
    AwaitingPayment,
    Collected,
}

/// One purchase made from the page.
#[derive(Debug, Clone, Serialize)]
pub struct Purchase {
    pub order_id: String,
    pub product: String,
    pub usd_cents: u64,
    pub credits: u64,
    pub checkout_url: String,
    pub status: PurchaseStatus,
    /// The wallet's chain holding the collected credential.
    pub chain: Option<i64>,
}

/// What walletd found when it fetched the gate's manifest over the use
/// plane, after the user approved the one the storefront served.
#[derive(Debug, Clone, Serialize)]
pub struct UsePlaneCheck {
    /// It reached the gate and got the approved manifest, byte for byte.
    pub ok: bool,
    pub after_ms: u64,
    pub text: String,
}

/// What `/api/state` returns.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub phase: Phase,
    pub error: Option<String>,
    pub over_tor: bool,
    /// The gate's onion address, once C tor has made it.
    pub onion: Option<String>,
    /// How long the bring-up took.
    pub ready_after_ms: Option<u64>,
    pub components: Vec<Component>,
    pub products: Vec<Product>,
    pub models: Vec<Model>,
    pub purchases: Vec<Purchase>,
    pub use_plane_check: Option<UsePlaneCheck>,
    /// Set when the services run on a GPU instance.
    pub remote: Option<RemoteInfo>,
}

/// How the console reaches the wallet once it is up.
pub struct Link {
    pub cli: WalletCli,
    pub walletd: SocketAddr,
    /// walletd's bearer token. Only the proxy uses it; the page never sees it.
    pub walletd_token: Zeroizing<String>,
}

/// Everything the console's handlers share.
pub struct Console {
    started: Instant,
    /// The page's token for `/api/*`: fresh each run, never written to disk.
    pub token: Zeroizing<String>,
    /// The `Host`s the console answers to.
    pub hosts: [String; 2],
    /// Where Checkout sends a buyer who paid.
    pub paid_url: String,
    events: Mutex<Vec<Event>>,
    sender: broadcast::Sender<Event>,
    view: Mutex<View>,
    link: OnceLock<Link>,
    /// One `wallet-cli` at a time, as from one terminal.
    cli_turn: tokio::sync::Mutex<()>,
    /// Asks the collector to run now: after a purchase, or from `/paid`.
    pub collect_now: Notify,
    /// The task reading the services' logs, from the moment they exist.
    tail: Mutex<Option<tokio::task::AbortHandle>>,
    /// Lines logged once: every connection to a dev server warns the same.
    noted: Mutex<HashSet<String>>,
    /// Whether the use-plane check has started.
    use_plane_started: AtomicBool,
}

impl Console {
    /// The console for a listener on `addr`.
    pub fn new(addr: SocketAddr, over_tor: bool) -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        // Without the OS's randomness no token could protect the console.
        rand::rngs::SysRng.try_fill_bytes(bytes.as_mut()).expect("the OS random number generator failed");
        let port = addr.port();
        Self {
            started: Instant::now(),
            token: Zeroizing::new(hex::encode(bytes.as_ref())),
            hosts: [format!("127.0.0.1:{port}"), format!("localhost:{port}")],
            paid_url: format!("http://127.0.0.1:{port}/paid"),
            events: Mutex::new(Vec::new()),
            sender: broadcast::channel(EVENT_BACKLOG).0,
            view: Mutex::new(View {
                phase: Phase::Starting,
                error: None,
                over_tor,
                onion: None,
                ready_after_ms: None,
                components: Vec::new(),
                products: Vec::new(),
                models: Vec::new(),
                purchases: Vec::new(),
                use_plane_check: None,
                remote: None,
            }),
            link: OnceLock::new(),
            cli_turn: tokio::sync::Mutex::new(()),
            collect_now: Notify::new(),
            tail: Mutex::new(None),
            noted: Mutex::new(HashSet::new()),
            use_plane_started: AtomicBool::new(false),
        }
    }

    /// Milliseconds since the demo started.
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Adds an entry to the activity log.
    pub fn event(&self, source: &str, kind: Kind, text: impl Into<String>) {
        let mut events = lock(&self.events);
        let event = Event {
            seq: events.len() as u64 + 1,
            at_ms: self.now_ms(),
            source: source.to_owned(),
            kind,
            text: text.into(),
        };
        // Nobody listening is fine: a page that connects later replays the log.
        let _ = self.sender.send(event.clone());
        events.push(event);
    }

    /// Adds a service's line to the activity log, except a dev-insecure
    /// warning already shown: every connection to a dev server repeats it.
    fn log_line(&self, source: &str, kind: Kind, text: String) {
        if text.contains("dev-insecure") && !lock(&self.noted).insert(format!("{source}\n{text}")) {
            return;
        }
        self.event(source, kind, text);
    }

    /// The log so far, and a receiver for what comes after it.
    pub fn subscribe(&self) -> (Vec<Event>, broadcast::Receiver<Event>) {
        let events = lock(&self.events);
        (events.clone(), self.sender.subscribe())
    }

    /// What the page shows.
    pub fn view(&self) -> View {
        lock(&self.view).clone()
    }

    /// The wallet, once the stack is up.
    pub fn link(&self) -> Option<&Link> {
        self.link.get()
    }

    /// A product of the release by id.
    pub fn product(&self, id: &str) -> Option<Product> {
        lock(&self.view).products.iter().find(|product| product.id == id).cloned()
    }

    /// Records a purchase whose checkout is open.
    pub fn add_purchase(&self, purchase: Purchase) {
        lock(&self.view).purchases.push(purchase);
        self.collect_now.notify_one();
    }

    /// Whether this call is the one to start the use-plane check.
    pub fn start_use_plane_check(&self) -> bool {
        !self.use_plane_started.swap(true, Ordering::SeqCst)
    }

    /// Records what the use-plane check found.
    pub fn set_use_plane_check(&self, check: UsePlaneCheck) {
        lock(&self.view).use_plane_check = Some(check);
    }

    /// Starts streaming the interesting lines of the logs in `logs` into
    /// the activity log.
    pub fn start_tail(self: &Arc<Self>, logs: PathBuf) {
        let tail = tokio::spawn(tail_logs(Arc::clone(self), logs));
        if let Some(previous) = lock(&self.tail).replace(tail.abort_handle()) {
            previous.abort();
        }
    }

    /// Stops reading the services' logs.
    pub fn stop_tail(&self) {
        if let Some(task) = lock(&self.tail).take() {
            task.abort();
        }
    }

    /// Marks the bring-up failed.
    pub fn fail(&self, error: &anyhow::Error) {
        let mut view = lock(&self.view);
        view.phase = Phase::Failed;
        view.error = Some(format!("{error:#}"));
        drop(view);
        self.event("xtask", Kind::Error, format!("bring-up failed: {error:#}"));
    }

    fn awaiting_payment(&self) -> bool {
        lock(&self.view).purchases.iter().any(|purchase| purchase.status == PurchaseStatus::AwaitingPayment)
    }

    /// Marks the oldest unpaid purchase of `product` collected.
    fn collected(&self, product: &str, chain: i64) {
        let mut view = lock(&self.view);
        let waiting = view
            .purchases
            .iter_mut()
            .find(|purchase| purchase.product == product && purchase.status == PurchaseStatus::AwaitingPayment);
        if let Some(purchase) = waiting {
            purchase.status = PurchaseStatus::Collected;
            purchase.chain = Some(chain);
        }
    }

    /// Runs `wallet-cli` on the wallet, logging the command and its output.
    pub async fn wallet_cli(&self, args: &[&str]) -> anyhow::Result<CliOutput> {
        let link = self.link().context("the wallet is not ready yet")?;
        self.run_cli(&link.cli, args, true).await
    }

    async fn run_cli(&self, cli: &WalletCli, args: &[&str], log: bool) -> anyhow::Result<CliOutput> {
        let _turn = self.cli_turn.lock().await;
        let output = cli.run(args, None).await?;
        if log {
            self.log_cli(args, &output);
        }
        Ok(output)
    }

    fn log_cli(&self, args: &[&str], output: &CliOutput) {
        self.event("wallet-cli", Kind::Command, format!("$ wallet-cli {}", args.join(" ")));
        for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
            self.event("wallet-cli", Kind::Output, line);
        }
        let failed = if output.code == Some(0) { Kind::Output } else { Kind::Error };
        for line in stderr_lines(&output.stderr) {
            let kind = if line.starts_with("WARN ") {
                Kind::Warn
            } else if line.starts_with("ERROR ") {
                Kind::Error
            } else {
                failed
            };
            self.log_line("wallet-cli", kind, line);
        }
    }
}

/// A command's stderr as lines, without blank ones and without the
/// timestamp tracing puts first.
pub fn stderr_lines(stderr: &str) -> Vec<String> {
    stderr.lines().filter(|line| !line.trim().is_empty()).map(|line| untimed(line).to_owned()).collect()
}

/// `2026-11-07T12:00:00.000000Z  WARN target: message` without the time.
fn untimed(line: &str) -> &str {
    let trimmed = line.trim_start();
    match trimmed.split_once(char::is_whitespace) {
        Some((first, rest)) if first.starts_with(|c: char| c.is_ascii_digit()) && first.ends_with('Z') => {
            rest.trim_start()
        }
        _ => line,
    }
}

/// Brings the stack up for the demo and starts walletd, logging each step.
pub async fn bring_up(console: &Arc<Console>, binaries: &Binaries, tor: Option<&Path>) -> anyhow::Result<World> {
    let report = |line: String| console.event("xtask", Kind::Ok, line);
    console.event("xtask", Kind::Info, "bringing up the stack in a fresh temp dir");
    let options =
        Options { tor, success_url: Some(&console.paid_url), token_delay_ms: TOKEN_DELAY_MS, report: &report };
    let mut world = World::bring_up_with(binaries, &options).await?;
    // From here C tor bootstraps and walletd starts: their logs show how far.
    console.start_tail(world.tree.logs());
    match start_wallet(console, &mut world).await {
        Ok(()) => Ok(world),
        Err(error) => {
            if let Some(dir) = world.finish(false).await {
                console.event("xtask", Kind::Info, format!("state and logs kept in {}", dir.display()));
            }
            Err(error)
        }
    }
}

/// Fills the page's view, makes the wallet's journal, and starts walletd
/// while the onion service is probed.
async fn start_wallet(console: &Console, world: &mut World) -> anyhow::Result<()> {
    let onion = world.tree.onion()?;
    fill_view(console, &world.tree.release(), onion.as_deref(), components(world, onion.as_deref()), None)?;
    let cli = world.wallet_cli();
    init_wallet(console, &cli).await?;

    let tree = world.tree.clone();
    let probe = async {
        if onion.is_some() {
            console.event("xtask", Kind::Info, "waiting for C tor to publish the onion service (up to 5 min)");
            // Usually under 30 s, but a slow descriptor upload has taken
            // minutes: say what the probes get so the wait isn't silent.
            let mut next_note = ONION_NOTE_EVERY;
            let target = tier2::wait_for_onion(&tree, |waited, error| {
                if waited >= next_note {
                    next_note = waited + ONION_NOTE_EVERY;
                    let secs = waited.as_secs();
                    console.event(
                        "xtask",
                        Kind::Info,
                        format!("no answer through the onion service after {secs} s yet: {error:#}"),
                    );
                }
            })
            .await?;
            console.event("xtask", Kind::Ok, format!("the gate answers `ready` at {target} through C tor"));
        }
        anyhow::Ok(())
    };
    if onion.is_some() {
        console.event("xtask", Kind::Info, "starting walletd: Arti bootstraps a Tor client first (up to 3 min)");
    }
    let (walletd, probed) = tokio::join!(world.start_walletd(), probe);
    let walletd = walletd?;
    probed?;
    let dialer = if onion.is_some() { "dials the gate's onion service through Arti" } else { "dials gate.sock" };
    link_wallet(console, cli, walletd, dialer)
}

/// Fills the page's view: the release's products and models, the gate's
/// onion address, the running services, and where they run.
pub fn fill_view(
    console: &Console,
    release: &Path,
    onion: Option<&str>,
    components: Vec<Component>,
    remote: Option<RemoteInfo>,
) -> anyhow::Result<()> {
    let (release, _) = ReleaseConfig::parse(&std::fs::read(release)?)?;
    let mut view = lock(&console.view);
    view.products = release
        .products
        .iter()
        .map(|product| Product { id: product.id.clone(), usd_cents: product.usd_cents, credits: product.credits })
        .collect();
    view.models = release
        .catalogue
        .iter()
        .map(|model| Model {
            id: model.id.clone(),
            engine: model.engine.clone(),
            input_rate: model.input_rate,
            output_rate: model.output_rate,
        })
        .collect();
    view.onion = onion.map(str::to_owned);
    view.components = components;
    view.remote = remote;
    Ok(())
}

/// Makes the wallet's journal with `wallet-cli init`.
pub async fn init_wallet(console: &Console, cli: &WalletCli) -> anyhow::Result<()> {
    let output = console.run_cli(cli, &["init"], true).await?;
    anyhow::ensure!(output.code == Some(0), "wallet-cli init failed: {}", output.stderr.trim());
    Ok(())
}

/// Hands the console the running walletd, which `dialer` says how it
/// reaches the gate, and marks the demo ready.
pub fn link_wallet(console: &Console, cli: WalletCli, walletd: SocketAddr, dialer: &str) -> anyhow::Result<()> {
    let token = Zeroizing::new(std::fs::read_to_string(cli.wallet().join("walletd.token"))?.trim().to_owned());
    console.event("xtask", Kind::Ok, format!("walletd serving on {walletd}; it {dialer}"));
    if console.link.set(Link { cli, walletd, walletd_token: token }).is_err() {
        anyhow::bail!("the wallet was linked twice");
    }

    let mut view = lock(&console.view);
    view.components.push(Component {
        name: "walletd",
        plane: "use",
        detail: format!("http://{walletd}: the wallet's local API; {dialer}"),
    });
    view.phase = Phase::Ready;
    view.ready_after_ms = Some(console.now_ms());
    drop(view);
    console.event("xtask", Kind::Ok, "ready: approve the manifest, buy credit, then chat");
    Ok(())
}

/// The services the bring-up started.
fn components(world: &World, onion: Option<&str>) -> Vec<Component> {
    let mut list = Vec::new();
    if let Some(host) = onion {
        list.push(Component {
            name: "C tor",
            plane: "use",
            detail: format!("publishes {host}:443, forwarding to the gate's Unix socket"),
        });
    }
    list.extend([
        Component {
            name: "gate",
            plane: "both",
            detail: "the attested boundary: issues credentials on issue.sock, meters sessions on gate.sock".to_owned(),
        },
        Component {
            name: "fake-engine",
            plane: "use",
            detail: format!("an OpenAI-compatible stand-in for vLLM on engine.sock, {TOKEN_DELAY_MS} ms per token"),
        },
        Component {
            name: "storefront",
            plane: "purchase",
            detail: format!("{}: sells credit, never sees what it buys", world.storefront_target),
        },
        Component {
            name: "fake-stripe",
            plane: "purchase",
            detail: format!("http://{}: a test checkout; no card is charged", world.fake_stripe_addr),
        },
    ]);
    list
}

/// Asks the storefront about unpaid orders every few seconds, and at once
/// when woken, and records what `wallet-cli collect` turned into credit.
pub async fn collect_loop(console: Arc<Console>) {
    let mut last = String::new();
    loop {
        tokio::select! {
            () = console.collect_now.notified() => {}
            () = tokio::time::sleep(COLLECT_POLL) => {}
        }
        let Some(link) = console.link() else { continue };
        if !console.awaiting_payment() {
            continue;
        }
        let output = match console.run_cli(&link.cli, &["collect"], false).await {
            Ok(output) => output,
            Err(error) => {
                console.event("wallet-cli", Kind::Error, format!("running collect: {error:#}"));
                continue;
            }
        };
        // Only a change is news: an unpaid order says the same every round,
        // once the timestamps are off its warnings.
        let text = format!("{}\n{}", output.stdout, stderr_lines(&output.stderr).join("\n"));
        if text != last {
            console.log_cli(&["collect"], &output);
            last = text;
        }
        for (product, chain) in output.stdout.lines().filter_map(parse_collected) {
            console.collected(&product, chain);
            console.event(
                "wallet",
                Kind::Ok,
                format!(
                    "credential for {product} unblinded and journalled as chain {chain}: \
                     the gate signed it blind, so nothing links it to the payment"
                ),
            );
        }
    }
}

/// `collected usd-1: chain 3 holds 1000000 credits (Ready)` as the product
/// and the chain.
fn parse_collected(line: &str) -> Option<(String, i64)> {
    let (product, rest) = line.strip_prefix("collected ")?.split_once(": chain ")?;
    let (chain, _) = rest.split_once(' ')?;
    Some((product.to_owned(), chain.parse().ok()?))
}

/// Streams the interesting lines of the services' logs into the activity
/// log: C tor's bootstrap progress, everything walletd says, vLLM's
/// throughput, and every warning or error.
async fn tail_logs(console: Arc<Console>, logs: PathBuf) {
    let names = ["tor", "walletd", "gate", "storefront", "fake-stripe", "fake-engine", "vllm"];
    let mut offsets = [0u64; 7];
    loop {
        for (name, offset) in names.iter().zip(offsets.iter_mut()) {
            for line in new_lines(&logs.join(format!("{name}.log")), offset) {
                if let Some((kind, text)) = interesting(name, &line) {
                    console.log_line(name, kind, text);
                }
            }
        }
        tokio::time::sleep(TAIL_POLL).await;
    }
}

/// The whole lines written after `offset`, moving it past them.
fn new_lines(path: &Path, offset: &mut u64) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let start = usize::try_from(*offset).unwrap_or(usize::MAX).min(bytes.len());
    let fresh = bytes.get(start..).unwrap_or_default();
    // A line still being written waits for its newline.
    let Some(end) = fresh.iter().rposition(|byte| *byte == b'\n') else { return Vec::new() };
    *offset += end as u64 + 1;
    String::from_utf8_lossy(fresh.get(..end).unwrap_or_default()).lines().map(str::to_owned).collect()
}

/// Whether a log line goes to the activity log, and as what.
fn interesting(service: &str, line: &str) -> Option<(Kind, String)> {
    if service == "tor" {
        // `Nov 07 12:00:00.000 [notice] Bootstrapped 45% (...): ...`
        let (level, text) = line.split_once(" [")?.1.split_once("] ")?;
        return match level {
            "warn" | "err" => Some((Kind::Warn, shorten(text))),
            _ if text.starts_with("Bootstrapped") => Some((Kind::Info, shorten(text))),
            _ => None,
        };
    }
    if service == "vllm" {
        // `(APIServer pid=7) INFO 11-07 12:00:00 [loggers.py:123] Engine 000: Avg prompt throughput: …`
        let text = line.split_once("] ").map_or(line, |(_, rest)| rest);
        return if line.contains(" ERROR ") || line.contains(" WARNING ") {
            Some((Kind::Warn, shorten(text)))
        } else if text.contains("throughput:") {
            Some((Kind::Info, shorten(text)))
        } else {
            None
        };
    }
    // `2026-11-07T12:00:00.000000Z  INFO target: message`, without the time.
    let mut words = line.split_whitespace();
    let first = words.next()?;
    let level = if first.starts_with(|c: char| c.is_ascii_digit()) { words.next()? } else { first };
    let rest = line.split_once(level).map_or(line, |(_, rest)| rest).trim_start();
    match level {
        "WARN" | "ERROR" => Some((Kind::Warn, shorten(rest))),
        "INFO" if service == "walletd" => Some((Kind::Info, shorten(rest))),
        _ => None,
    }
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_LOG_LINE {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(MAX_LOG_LINE).collect();
    short.push('…');
    short
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_collected_line_names_the_product_and_chain() {
        assert_eq!(
            parse_collected("collected usd-1: chain 3 holds 1000000 credits (Ready)"),
            Some(("usd-1".to_owned(), 3))
        );
        assert_eq!(parse_collected("order cs_test_1: 402 unpaid"), None);
    }

    #[test]
    fn log_lines_lose_their_timestamps_and_keep_what_matters() {
        let tor =
            "Nov 07 12:00:00.000 [notice] Bootstrapped 45% (requesting_descriptors): Asking for relay descriptors";
        assert_eq!(
            interesting("tor", tor),
            Some((Kind::Info, "Bootstrapped 45% (requesting_descriptors): Asking for relay descriptors".to_owned()))
        );
        assert_eq!(interesting("tor", "Nov 07 12:00:00.000 [notice] Opening Socks listener"), None);
        let walletd = "2026-11-07T12:00:00.000000Z  INFO walletd::dialer: bootstrapping Tor";
        assert_eq!(
            interesting("walletd", walletd),
            Some((Kind::Info, "walletd::dialer: bootstrapping Tor".to_owned()))
        );
        assert_eq!(interesting("gate", "2026-11-07T12:00:00.000000Z  INFO gate::state: unlocked"), None);
        let warning = "2026-11-07T12:00:00.000000Z  WARN storefront::gate: issuing failed";
        assert_eq!(
            interesting("storefront", warning),
            Some((Kind::Warn, "storefront::gate: issuing failed".to_owned()))
        );
        let stats = "(APIServer pid=7) INFO 11-07 12:00:00 [loggers.py:123] Engine 000: Avg prompt throughput: \
                     4.1 tokens/s, Avg generation throughput: 98.0 tokens/s";
        assert_eq!(
            interesting("vllm", stats),
            Some((
                Kind::Info,
                "Engine 000: Avg prompt throughput: 4.1 tokens/s, Avg generation throughput: 98.0 tokens/s".to_owned()
            ))
        );
        assert_eq!(interesting("vllm", "(APIServer pid=7) INFO 11-07 12:00:00 [api_server.py:1] Starting"), None);
        let warning = "(APIServer pid=7) WARNING 11-07 12:00:00 [config.py:9] something";
        assert_eq!(interesting("vllm", warning), Some((Kind::Warn, "something".to_owned())));
    }

    #[test]
    fn stderr_loses_tracing_timestamps_only() {
        let stderr = "2026-11-07T19:31:21.917427Z  WARN gate_conn::conn: connected to a dev-insecure server\n\n\
                      error: 402 unpaid\n";
        assert_eq!(
            stderr_lines(stderr),
            ["WARN gate_conn::conn: connected to a dev-insecure server", "error: 402 unpaid"]
        );
    }

    #[test]
    fn a_dev_insecure_warning_is_logged_once_per_source() {
        let console = Console::new(SocketAddr::from(([127, 0, 0, 1], 7700)), false);
        let warning = "WARN gate_conn::conn: connected to a dev-insecure server";
        console.log_line("wallet-cli", Kind::Warn, warning.to_owned());
        console.log_line("wallet-cli", Kind::Warn, warning.to_owned());
        console.log_line("walletd", Kind::Warn, warning.to_owned());
        console.log_line("walletd", Kind::Info, "POST /v1/chat/completions".to_owned());
        console.log_line("walletd", Kind::Info, "POST /v1/chat/completions".to_owned());
        let (events, _) = console.subscribe();
        let sources: Vec<_> = events.iter().map(|event| event.source.as_str()).collect();
        assert_eq!(sources, ["wallet-cli", "walletd", "walletd", "walletd"]);
    }

    #[test]
    fn only_whole_lines_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.log");
        std::fs::write(&path, "one\ntwo\nthr").unwrap();
        let mut offset = 0;
        assert_eq!(new_lines(&path, &mut offset), ["one", "two"]);
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(new_lines(&path, &mut offset), ["three"]);
        assert!(new_lines(&path, &mut offset).is_empty());
    }
}
