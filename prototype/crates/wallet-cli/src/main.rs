//! `wallet-cli`: init, manifest and purchase over the TCP dialer to the
//! storefront, and the use-plane commands through `walletd` (SPEC section 7).
//!
//! It never reaches the gate and never links Arti (ARCHITECTURE 8.2 rule 5):
//! buying is clearnet by design, spending is walletd's job.
//!
//! Exit status: 0 on success, 1 on an error, and 2 from `collect` when
//! orders are still waiting for payment.

mod local;
mod purchase;
mod storefront;
mod use_plane;

use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::local::Walletd;
use crate::purchase::Wallet;

/// The Counteractive wallet's command line.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// The wallet directory: `wallet.toml`, the journal and its key.
    #[arg(long, env = "COUNTERACTIVE_WALLET_DIR", global = true, default_value = ".dev/wallet")]
    wallet_dir: PathBuf,
    /// Print walletd's answers as JSON (balance, models, chat, close, reconcile).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create the journal and its key next to `wallet.toml`.
    Init,
    /// Fetch or approve the service manifest.
    Manifest {
        #[command(subcommand)]
        action: ManifestCommand,
    },
    /// Order credits and print where to pay.
    Buy {
        /// A product id from the accepted release, such as `usd-5`.
        #[arg(long)]
        product: String,
        /// Print the checkout URL without opening a browser.
        #[arg(long)]
        no_browser: bool,
    },
    /// Turn paid orders into credit.
    Collect {
        /// Keep asking for up to this many seconds while orders are unpaid.
        #[arg(long, default_value_t = 0)]
        wait: u64,
    },
    /// Show the balance (through walletd).
    Balance,
    /// List the models (through walletd).
    Models,
    /// Send one message and stream the answer (through walletd).
    Chat {
        /// Model id.
        #[arg(long)]
        model: String,
        /// Turns with the same conversation id share one session.
        #[arg(long)]
        conversation: Option<String>,
        /// The message, or `-` to read it from stdin.
        prompt: String,
    },
    /// Close one conversation's session, or every session (through walletd).
    Close {
        /// The conversation; all of them when absent.
        #[arg(long)]
        conversation: Option<String>,
    },
    /// Settle sessions left unsettled and report every settled one (through walletd).
    Reconcile,
}

#[derive(Debug, Subcommand)]
enum ManifestCommand {
    /// Fetch the storefront's manifest and check it against the pins.
    Fetch {
        /// Accept it at once instead of leaving it pending.
        #[arg(long)]
        approve: bool,
    },
    /// Accept the fetched manifest waiting for approval.
    Approve,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    // A usage error exits 1 like any other error: clap's own status for
    // it, 2, would read as `collect`'s "orders still unpaid".
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
    };
    match run(args).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> anyhow::Result<ExitCode> {
    let dir = &args.wallet_dir;
    let json = args.json;
    match args.command {
        Command::Init => purchase::init(dir)?,
        Command::Manifest { action: ManifestCommand::Fetch { approve } } => {
            purchase::fetch_manifest(&mut Wallet::open(dir)?, approve).await?;
        }
        Command::Manifest { action: ManifestCommand::Approve } => purchase::approve_manifest(&mut Wallet::open(dir)?)?,
        Command::Buy { product, no_browser } => purchase::buy(&mut Wallet::open(dir)?, &product, !no_browser).await?,
        Command::Collect { wait } => {
            let outcome = purchase::collect(&mut Wallet::open(dir)?, Duration::from_secs(wait)).await?;
            if outcome.frozen > 0 {
                return Ok(ExitCode::FAILURE);
            }
            if outcome.waiting > 0 {
                return Ok(ExitCode::from(2));
            }
        }
        Command::Balance => use_plane::balance(&Walletd::locate(dir)?, json).await?,
        Command::Models => use_plane::models(&Walletd::locate(dir)?, json).await?,
        Command::Chat { model, conversation, prompt } => {
            let prompt = if prompt == "-" { std::io::read_to_string(std::io::stdin())? } else { prompt };
            use_plane::chat(&Walletd::locate(dir)?, &model, conversation.as_deref(), prompt, json).await?;
        }
        Command::Close { conversation } => use_plane::close(&Walletd::locate(dir)?, conversation, json).await?,
        Command::Reconcile => use_plane::reconcile(&Walletd::locate(dir)?, json).await?,
    }
    Ok(ExitCode::SUCCESS)
}
