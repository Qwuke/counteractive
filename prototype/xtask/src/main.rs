//! `xtask`: the operator's and developer's tasks (SPEC section 9).
//!
//! - `keygen`: K, the operator's minisign key, the dev attestation key and
//!   the storefront token, in `keys/`
//! - `release`: `release.json`
//! - `up`: fake-engine, fake-stripe, the gate and the storefront in the
//!   background, logging to `logs/`; writes the wallet's pins
//! - `unlock`: unlocks the gate, signs and posts the manifest on its first
//!   boot, and gives the storefront its Stripe key
//! - `down`: stops what `up` started
//! - `e2e [--tor]`: the acceptance run of SPEC section 10, in a temp dir
//! - `demo [--local] [--check]`: the whole system in a temp dir, driven
//!   from a console page in the browser; chat goes over Tor
//! - `host`: `up` for a GPU instance: the services around a running
//!   engine, plus C tor, without the operator's secrets
//! - `remote init|deploy|unlock|tunnel|status|down`: that instance,
//!   driven from here, with its tree in `.verda/`
//! - `demo --remote`: the console for the instance's stack
//!
//! xtask is operator tooling, so unlike the wallet binaries it may use
//! both the TCP and the Unix dialer.

mod demo;
mod e2e;
mod keys;
mod operator;
mod process;
mod release;
mod remote;
mod stack;
mod tree;
mod updown;

use std::io::IsTerminal as _;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::keys::Keys;
use crate::tree::Tree;

/// Counteractive prototype tasks.
#[derive(Debug, Parser)]
struct Args {
    /// The dev tree. Defaults to `.dev/` in the prototype workspace, or
    /// `.verda/` for `remote` and `demo --remote`.
    #[arg(long, global = true)]
    dev_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate the operator's keys.
    Keygen,
    /// Write `release.json`.
    Release,
    /// Start the stack in the background.
    Up {
        /// Point the storefront at api.stripe.com instead of starting
        /// fake-stripe. `unlock` then reads the key from `STRIPE_SECRET_KEY`.
        #[arg(long)]
        real_stripe: bool,
    },
    /// Unlock the gate and configure the storefront.
    Unlock,
    /// Stop what `up` started.
    Down,
    /// Run the end-to-end acceptance steps in a temp dir.
    E2e {
        /// Also run tier 2: the gate behind a local C tor onion service.
        #[arg(long)]
        tor: bool,
    },
    /// Bring the whole system up in a temp dir and drive it from a browser:
    /// buy credit on the clear web, then chat over Tor.
    Demo {
        /// The console's port on 127.0.0.1 (default 7700).
        #[arg(long)]
        port: Option<u16>,
        /// Don't open the console in the browser.
        #[arg(long)]
        no_browser: bool,
        /// Run the headless check of the console's API, then stop.
        #[arg(long)]
        check: bool,
        /// Leave Tor out: walletd reaches the gate on its Unix socket.
        #[arg(long, conflicts_with = "remote")]
        local: bool,
        /// Use the stack `remote deploy` started on the GPU instance; only
        /// walletd and wallet-cli run here.
        #[arg(long)]
        remote: bool,
        /// Keep the temp dir, with every service's state and log.
        #[arg(long)]
        keep: bool,
    },
    /// On a GPU instance: start fake-stripe, the gate, the storefront and
    /// C tor around the engine on `run/engine.sock`. The gate stays Locked
    /// until `remote unlock`.
    Host {
        /// Where the storefront listens.
        #[arg(long, default_value = "127.0.0.1:8443")]
        storefront: SocketAddr,
        /// Where fake-stripe listens.
        #[arg(long, default_value = "127.0.0.1:8090")]
        fake_stripe: SocketAddr,
        /// Where checkout sends a buyer who paid.
        #[arg(long)]
        success_url: Option<String>,
        /// Start fake-engine instead of using a running engine.
        #[arg(long)]
        fake_engine: bool,
        /// The C tor program.
        #[arg(long, default_value = "tor")]
        tor: PathBuf,
    },
    /// Run the stack on a rented GPU instance, driven from here.
    Remote {
        #[command(subcommand)]
        command: RemoteCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RemoteCommand {
    /// Keys, a release serving one vLLM model, and an SSH key.
    Init {
        /// The model's id in the catalogue.
        #[arg(long, default_value = "qwen3-4b")]
        model: String,
        /// The Hugging Face repository vLLM loads.
        #[arg(long, default_value = "Qwen/Qwen3-4B-Instruct-2507")]
        weights: String,
        /// The context window: vLLM's --max-model-len.
        #[arg(long, default_value_t = 16384)]
        context: u32,
        /// The most output one turn may ask for.
        #[arg(long, default_value_t = 2048)]
        max_output: u32,
        /// Credits per prompt token.
        #[arg(long, default_value_t = 1)]
        input_rate: u64,
        /// Credits per output token.
        #[arg(long, default_value_t = 3)]
        output_rate: u64,
        /// The storefront's port, on the instance and through the tunnel.
        #[arg(long, default_value_t = 8443)]
        storefront_port: u16,
        /// fake-stripe's port, on the instance and through the tunnel.
        #[arg(long, default_value_t = 8090)]
        fake_stripe_port: u16,
        /// The demo console's port, where checkout sends a buyer back.
        #[arg(long, default_value_t = 7700)]
        console_port: u16,
    },
    /// Copy the source and release over, set the instance up and start
    /// vLLM and the services.
    Deploy {
        /// The instance, as `ssh` takes it: `root@203.0.113.7`. Defaults
        /// to the last one deployed to.
        #[arg(long, conflicts_with = "local")]
        host: Option<String>,
        /// Rehearse in this directory on this machine, with fake-engine.
        #[arg(long)]
        local: Option<PathBuf>,
        /// Skip installing, on an instance already set up.
        #[arg(long)]
        skip_setup: bool,
    },
    /// Unlock the gate and configure the storefront through an SSH tunnel.
    Unlock,
    /// Hold the SSH tunnel open until Ctrl-C.
    Tunnel,
    /// Show the instance's attestation summary and what runs there.
    Status,
    /// Stop vLLM and the services on the instance.
    Down,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let args = Args::parse();
    let on_instance = matches!(args.command, Command::Remote { .. } | Command::Demo { remote: true, .. });
    let default_dir = if on_instance { ".verda" } else { ".dev" };
    let tree = Tree::new(args.dev_dir.unwrap_or_else(|| process::workspace_root().join(default_dir)));
    let result = match args.command {
        Command::Keygen => Keys::generate(&tree).map(|_| println!("keys written to {}", tree.keys().display())),
        Command::Release => Keys::read(&tree).and_then(|keys| release::write(&tree, &keys, None)).map(|()| {
            println!("wrote {}", tree.release().display());
        }),
        Command::Up { real_stripe } => updown::up(&tree, real_stripe).await,
        Command::Unlock => updown::unlock(&tree).await,
        Command::Down => updown::down(&tree).await,
        Command::E2e { tor } => match Box::pin(e2e::run(tor)).await {
            Ok(true) => Ok(()),
            Ok(false) => return ExitCode::FAILURE,
            Err(error) => Err(error),
        },
        Command::Demo { port, no_browser, check, local, remote, keep } => {
            let mode = if check { demo::Mode::Check } else { demo::Mode::Browser { open: !no_browser } };
            let remote = remote.then_some(tree);
            Box::pin(demo::run(demo::DemoArgs { port, mode, local, remote, keep })).await
        }
        Command::Host { storefront, fake_stripe, success_url, fake_engine, tor } => {
            let args = updown::HostArgs { storefront, fake_stripe, success_url, fake_engine, tor };
            updown::host(&tree, &args).await
        }
        Command::Remote { command } => run_remote(&tree, command).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run_remote(tree: &Tree, command: RemoteCommand) -> anyhow::Result<()> {
    match command {
        RemoteCommand::Init {
            model,
            weights,
            context,
            max_output,
            input_rate,
            output_rate,
            storefront_port,
            fake_stripe_port,
            console_port,
        } => {
            let model = release::VllmModel { id: model, weights, context, max_output, input_rate, output_rate };
            let ports =
                remote::Ports { storefront: storefront_port, fake_stripe: fake_stripe_port, console: console_port };
            remote::init(tree, model, ports)
        }
        RemoteCommand::Deploy { host, local, skip_setup } => {
            remote::deploy(tree, remote::DeployArgs { host, local, skip_setup })
        }
        RemoteCommand::Unlock => remote::unlock(tree).await,
        RemoteCommand::Tunnel => remote::tunnel(tree).await,
        RemoteCommand::Status => remote::status(tree),
        RemoteCommand::Down => remote::down(tree),
    }
}
