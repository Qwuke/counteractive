//! The gate binary: starts Locked and serves until SIGINT or SIGTERM.
//!
//! `gate --release <release.json> --data <dir> --run <dir> --engine <socket> --dev-attest-key <file>`

use std::io::IsTerminal as _;
use std::path::PathBuf;

use clap::Parser;
use gate::GateOptions;
use tokio::signal::unix::{SignalKind, signal};

/// The CVM service: inner TLS, attestation, issuance, sessions and refunds.
#[derive(Parser)]
struct Args {
    /// The measured release config.
    #[arg(long)]
    release: PathBuf,
    /// Data directory for the money store and the signed manifest.
    #[arg(long)]
    data: PathBuf,
    /// Run directory for gate.sock and issue.sock.
    #[arg(long)]
    run: PathBuf,
    /// The inference engine's Unix socket.
    #[arg(long)]
    engine: PathBuf,
    /// The dev-insecure attestation key: 32 bytes, raw or hex.
    #[arg(long)]
    dev_attest_key: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;

    let gate = gate::start(GateOptions {
        release: args.release,
        data: args.data,
        run: args.run,
        engine: args.engine,
        dev_attest_key: args.dev_attest_key,
    })
    .await?;
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }
    tracing::info!("stopping");
    gate.shutdown().await;
    Ok(())
}
