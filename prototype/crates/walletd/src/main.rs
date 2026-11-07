//! `walletd`: spends credentials over the Unix or Arti dialer and serves the
//! local OpenAI-compatible API and page (SPEC section 7).
//!
//! `walletd --wallet-dir <dir> [--listen 127.0.0.1:7777]`
//!
//! Prints `listening on <addr>` once bound and writes the address to
//! `<wallet_dir>/walletd.addr`, so `wallet-cli` and a caller that passed
//! port 0 can find it. On SIGINT or SIGTERM it closes every live session
//! before exiting, so their refunds land in the journal. It refuses to
//! start while another walletd serves the same wallet directory.

mod api;
mod conversation;
mod dialer;
mod error;
mod gate;
mod lock;
mod reconcile;
mod session;
mod state;
#[cfg(test)]
mod tests;
mod token;
mod turn;

use std::io::IsTerminal as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use clap::Parser;
use gate_conn::Target;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;
use wallet_core::{Journal, Pins};

use crate::gate::GateClient;
use crate::lock::InstanceLock;
use crate::state::Walletd;
use crate::token::Token;

/// The address file's name inside the wallet directory.
const ADDR_FILE: &str = "walletd.addr";

/// How long shutdown waits for live sessions to close.
const SHUTDOWN_CLOSE: Duration = Duration::from_secs(10);

/// The wallet daemon: the local API and page, and every gate session.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// The wallet directory: `wallet.toml`, the journal and its key.
    #[arg(long, env = "COUNTERACTIVE_WALLET_DIR")]
    wallet_dir: PathBuf,
    /// Loopback address for the local API. Port 0 picks a free one.
    #[arg(long, default_value = "127.0.0.1:7777")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();
    if !args.listen.ip().is_loopback() {
        bail!("walletd listens on loopback only, not {}", args.listen);
    }
    let _instance = InstanceLock::acquire(&args.wallet_dir)?;

    let pins = Pins::read(&args.wallet_dir)?;
    let journal = Journal::open(&args.wallet_dir)?;
    let target: Target = pins.gate_target.parse()?;
    let dialer = dialer::for_target(&target, &args.wallet_dir).await?;
    if pins.allow_dev_attestation {
        tracing::warn!("dev-insecure attestation allowed: no hardware protects the gate this wallet talks to");
    }
    let gate = GateClient::new(dialer, target, pins.allow_dev_attestation);
    let token = Token::load_or_create(&args.wallet_dir)?;

    let listener = TcpListener::bind(args.listen).await.with_context(|| format!("binding {}", args.listen))?;
    let addr = listener.local_addr()?;
    let allowed_hosts = vec![addr.to_string(), format!("localhost:{}", addr.port())];
    let walletd = Arc::new(Walletd::new(pins, journal, gate, token, allowed_hosts)?);

    let addr_file = args.wallet_dir.join(ADDR_FILE);
    std::fs::write(&addr_file, addr.to_string()).with_context(|| format!("writing {}", addr_file.display()))?;
    println!("listening on {addr}");
    tracing::info!(%addr, "walletd serving; the page is at http://{addr}/");

    tokio::spawn(conversation::close_idle(Arc::clone(&walletd), conversation::IDLE_CLOSE));
    tokio::spawn(startup_reconcile(Arc::clone(&walletd)));

    let mut interrupt = signal(SignalKind::interrupt()).context("installing the SIGINT handler")?;
    let mut terminate = signal(SignalKind::terminate()).context("installing the SIGTERM handler")?;
    let stop = async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    };
    axum::serve(listener, api::router(Arc::clone(&walletd))).with_graceful_shutdown(stop).await?;

    tracing::info!("stopping: closing live sessions");
    if tokio::time::timeout(SHUTDOWN_CLOSE, close_all(&walletd)).await.is_err() {
        tracing::warn!("some sessions did not close in time; reconcile settles them on the next start");
    }
    remove_addr_file(&addr_file);
    Ok(())
}

/// Settles what a previous run left behind. Best-effort: a gate that is
/// down now is retried by the next reconcile.
async fn startup_reconcile(walletd: Arc<Walletd>) {
    let Ok(context) = walletd.context() else { return };
    match walletd.reconcile(&context).await {
        Ok(report) if report.recovered > 0 => tracing::info!(recovered = report.recovered, "settled earlier sessions"),
        Ok(_) => {}
        Err(error) => tracing::warn!(code = error.code(), "startup reconcile could not settle every session"),
    }
}

/// Closes every conversation's session.
async fn close_all(walletd: &Walletd) {
    let Ok(context) = walletd.context() else { return };
    for (id, conversation) in walletd.conversations.all() {
        let mut conversation = conversation.lock().await;
        if let Err(error) = walletd.close_conversation(&context, &mut conversation).await {
            tracing::warn!(code = error.code(), "closing a session at shutdown failed");
        }
        walletd.conversations.remove(&id);
    }
}

fn remove_addr_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(%error, "could not remove the address file");
    }
}
