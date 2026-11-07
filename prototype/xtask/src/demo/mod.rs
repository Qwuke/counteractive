//! `xtask demo`: the whole system on one machine, driven from a browser.
//!
//! It brings up the e2e stack in a temp dir (keys, release, C tor with
//! the gate as an onion service, fake-engine, fake-stripe, the gate and
//! the storefront, unlocked and configured), starts walletd with its Arti
//! dialer, and serves a console page on loopback that drives the real
//! wallet:
//!
//! - the manifest and the purchase through `wallet-cli`, the purchase
//!   plane: clearnet to the storefront, with fake-stripe's checkout page
//!   in a browser tab;
//! - chat through walletd's own API, the use plane: walletd reaches the
//!   gate's onion service through Arti. The console proxies those calls
//!   and adds walletd's token on this side, so the page never holds it.
//!
//! `--check` drives the same HTTP API the page uses, headlessly, and
//! checks every number; `--local` leaves Tor out. `--remote` uses the
//! stack `xtask remote deploy` started on a GPU instance instead of
//! starting one here.

mod check;
mod console;
mod http;
mod remote;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use tokio::net::TcpListener;

use crate::e2e::toolchain;
use crate::e2e::world::World;
use crate::process::Binaries;
use crate::remote::RemoteConfig;
use crate::tree::Tree;
use console::Console;

/// The console's port when none is given.
const DEFAULT_PORT: u16 = 7700;

/// What `xtask demo` was asked for.
#[derive(Debug, Clone)]
pub struct DemoArgs {
    /// The console's port on 127.0.0.1; any free port when checking a
    /// local stack.
    pub port: Option<u16>,
    pub mode: Mode,
    /// Leave Tor out: walletd reaches the gate on its Unix socket.
    pub local: bool,
    /// The tree of `xtask remote`, to use the instance's stack.
    pub remote: Option<Tree>,
    /// Keep the temp dir with every service's state and log.
    pub keep: bool,
}

/// The services the console drives.
enum Stack {
    /// All of them, here.
    Local(Box<World>),
    /// The wallet here, the rest on the GPU instance.
    Remote(remote::RemoteStack),
}

impl Stack {
    fn fake_stripe_addr(&self) -> SocketAddr {
        match self {
            Self::Local(world) => world.fake_stripe_addr,
            Self::Remote(stack) => stack.fake_stripe_addr,
        }
    }

    async fn finish(self, passed: bool) -> Option<PathBuf> {
        match self {
            Self::Local(world) => world.finish(passed).await,
            Self::Remote(stack) => stack.finish(passed).await,
        }
    }
}

/// The console's port. On an instance the storefront sends a buyer who
/// paid to the port `remote init` chose, so the console must be there.
fn console_port(args: &DemoArgs) -> anyhow::Result<u16> {
    let Some(tree) = &args.remote else {
        return Ok(if args.mode == Mode::Check { 0 } else { args.port.unwrap_or(DEFAULT_PORT) });
    };
    let deployed = RemoteConfig::read(tree)?.ports.console;
    match args.port {
        Some(port) if port != deployed => bail!(
            "the instance's checkout sends buyers back to port {deployed}; `remote init --console-port {port}` \
             and `remote deploy` to move it"
        ),
        _ => Ok(deployed),
    }
}

/// Who drives the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A person, until Ctrl-C; `open` opens the default browser on it.
    Browser { open: bool },
    /// The headless check, which then stops everything.
    Check,
}

/// Runs the demo until Ctrl-C, or the check until it ends.
pub async fn run(args: DemoArgs) -> anyhow::Result<()> {
    // Remote, the instance runs C tor; walletd here needs only Arti.
    let tor = if args.local || args.remote.is_some() {
        None
    } else {
        Some(
            toolchain::tor_available()
                .context("`tor` is not installed (`brew install tor`); `xtask demo --local` runs without it")?,
        )
    };
    let over_tor = tor.is_some() || args.remote.is_some();
    let binaries = Binaries::build(over_tor)?;

    let check = args.mode == Mode::Check;
    let port = console_port(&args)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("binding 127.0.0.1:{port}; is another demo running?"))?;
    let addr = listener.local_addr()?;
    let console = Arc::new(Console::new(addr, over_tor));
    let server = tokio::spawn(http::serve(listener, Arc::clone(&console)));
    let url = format!("http://{addr}/");
    println!("demo console: {url}");
    if args.mode == (Mode::Browser { open: true }) {
        open_browser(&url);
    }

    let bring_up = async {
        match &args.remote {
            Some(tree) => remote::bring_up(&console, &binaries, tree).await.map(Stack::Remote),
            None => {
                console::bring_up(&console, &binaries, tor.as_deref()).await.map(|world| Stack::Local(Box::new(world)))
            }
        }
    };
    let stack = tokio::select! {
        stack = bring_up => stack,
        _ = tokio::signal::ctrl_c() => {
            println!("interrupted during bring-up; stopping");
            server.abort();
            return Ok(());
        }
    };
    let stack = match stack {
        Ok(stack) => stack,
        Err(error) => {
            console.stop_tail();
            console.fail(&error);
            if !check {
                eprintln!("bring-up failed: {error:#}\nthe console shows it; Ctrl-C to quit");
                let _ = tokio::signal::ctrl_c().await;
            }
            server.abort();
            return Err(error);
        }
    };

    let collector = tokio::spawn(console::collect_loop(Arc::clone(&console)));
    let result = if check {
        check::run(&console, addr, stack.fake_stripe_addr()).await
    } else {
        let stops = if args.remote.is_some() { "walletd and the tunnel" } else { "every service" };
        println!("ready: {url}  (Ctrl-C stops {stops})");
        tokio::signal::ctrl_c().await.context("waiting for Ctrl-C")
    };
    collector.abort();
    console.stop_tail();
    println!("stopping the services");
    let kept = stack.finish(result.is_ok() && !args.keep).await;
    server.abort();
    if let Some(dir) = kept {
        println!("state and logs kept in {}", dir.display());
    }
    result
}

/// Opens `url` in the default browser, or says to.
fn open_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    if let Err(error) = std::process::Command::new(opener).arg(url).status() {
        eprintln!("could not open a browser ({error}); open {url} yourself");
    }
}
