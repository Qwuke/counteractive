//! Runs [`fake_stripe::FakeStripe`] on a TCP port until interrupted.
//!
//! Prints `listening on <addr>` to stdout once bound, so a caller that
//! passed port 0 can find it.

use std::io::IsTerminal as _;
use std::net::SocketAddr;

use anyhow::Context as _;
use clap::Parser;
use fake_stripe::FakeStripe;
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

/// A local stand-in for Stripe's Checkout Session API. Test use only.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Address to listen on. Port 0 picks a free one.
    #[arg(long, default_value = "127.0.0.1:12111")]
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

    let server = FakeStripe::start(args.listen).await.with_context(|| format!("binding {}", args.listen))?;
    println!("listening on {}", server.addr());
    tracing::info!(addr = %server.addr(), "fake-stripe serving");

    let mut terminate = signal(SignalKind::terminate()).context("installing the SIGTERM handler")?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.context("waiting for Ctrl-C")?,
        _ = terminate.recv() => {}
    }
    tracing::info!("shutting down");
    server.stop().await.context("serving")
}
