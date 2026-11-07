//! Starts the storefront: reads `release.json` and the dev attestation key,
//! generates this boot's TLS key, and serves inner TLS on TCP until
//! interrupted. It starts `unconfigured`; the operator sends the Stripe key.
//!
//! Prints `listening on <addr>` to stdout once bound, so a caller that
//! passed port 0 can find it.

use std::io::IsTerminal as _;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context as _, anyhow};
use clap::Parser;
use reqwest::Url;
use storefront::Storefront;
use storefront::config::{self, Settings};
use storefront::stripe::StripeEndpoint;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

/// The Counteractive storefront: Stripe Checkout and issuance through the gate.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// The release config: products, prices and the operator token's hash.
    #[arg(long)]
    release: PathBuf,
    /// Address to serve inner TLS on. Port 0 picks a free one.
    #[arg(long, default_value = "127.0.0.1:8443")]
    listen: SocketAddr,
    /// The gate's issuance socket.
    #[arg(long)]
    issue_socket: PathBuf,
    /// Stripe's API, or a fake-stripe for tests.
    #[arg(long, default_value = "https://api.stripe.com")]
    stripe_base_url: Url,
    /// Directory holding `manifest.json` and `manifest.sig`, served at `/v1/manifest`.
    #[arg(long)]
    manifest: PathBuf,
    /// The dev-insecure attestation key: 32 bytes, raw or as hex.
    #[arg(long)]
    dev_attest_key: PathBuf,
    /// Where Checkout sends the buyer after paying: walletd's local page.
    #[arg(long, default_value = "http://127.0.0.1:7777/paid")]
    success_url: Url,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();

    let (release, release_config_sha256) = config::read_release(&args.release)?;
    let attestation = config::dev_attestation_source(&args.dev_attest_key, release_config_sha256)?;
    tracing::warn!("dev-insecure attestation: no hardware protects this storefront");
    let settings = Settings {
        release,
        attestation: Box::new(attestation),
        issue_socket: args.issue_socket,
        stripe: StripeEndpoint::new(args.stripe_base_url, args.success_url).map_err(|error| anyhow!(error))?,
        manifest_dir: args.manifest,
    };
    let storefront = Storefront::new(settings).context("generating the TLS identity")?;

    let listener = TcpListener::bind(args.listen).await.with_context(|| format!("binding {}", args.listen))?;
    let addr = listener.local_addr()?;
    println!("listening on {addr}");
    tracing::info!(%addr, state = "unconfigured", "storefront serving");

    let shutdown = CancellationToken::new();
    let mut terminate = signal(SignalKind::terminate()).context("installing the SIGTERM handler")?;
    let trigger = shutdown.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
        tracing::info!("shutting down");
        trigger.cancel();
    });
    storefront.serve(listener, shutdown).await;
    Ok(())
}
