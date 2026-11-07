//! Runs [`fake_engine`] on a Unix socket until SIGINT or SIGTERM.
//!
//! `fake-engine --socket <path> [--token-delay-ms <n>]`. A stale socket
//! file at the path is replaced; any other kind of file there is an error.

use std::io::IsTerminal as _;
use std::os::unix::fs::FileTypeExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use clap::Parser;
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

/// A deterministic OpenAI-compatible engine for tests (SPEC section 8).
#[derive(Parser)]
struct Args {
    /// Unix socket to listen on.
    #[arg(long)]
    socket: PathBuf,
    /// A pause before each output token, in milliseconds, so a demo shows
    /// tokens arriving as a real model sends them.
    #[arg(long, default_value_t = 0)]
    token_delay_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();
    remove_stale_socket(&args.socket)?;
    let listener = UnixListener::bind(&args.socket).with_context(|| format!("binding {}", args.socket.display()))?;
    tracing::info!(socket = %args.socket.display(), "fake engine listening");

    let shutdown = CancellationToken::new();
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let stop = shutdown.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
        stop.cancel();
    });
    fake_engine::serve(listener, Arc::default(), Duration::from_millis(args.token_delay_ms), shutdown).await?;
    let _ = std::fs::remove_file(&args.socket);
    Ok(())
}

fn remove_stale_socket(path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(path).with_context(|| format!("removing stale socket {}", path.display()))
        }
        Ok(_) => bail!("{} exists and is not a socket", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("checking {}", path.display())),
    }
}
