//! `xtask up`, `unlock` and `down`: the stack in the background for
//! manual use, with logs in `.dev/logs/` and pids in `.dev/up.json`.
//!
//! `xtask host` is `up` for a GPU instance: the same services around an
//! engine that is already running, plus C tor, and none of the operator's
//! secrets.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};

use crate::keys::Keys;
use crate::operator;
use crate::process::{self, Binaries, Service};
use crate::stack;
use crate::tree::Tree;

/// The environment variable `unlock` reads a real Stripe key from.
const STRIPE_KEY_VAR: &str = "STRIPE_SECRET_KEY";
/// Stripe's API, for `up --real-stripe`.
const STRIPE_API: &str = "https://api.stripe.com";
/// The key `unlock` gives a storefront that talks to fake-stripe.
pub const FAKE_STRIPE_KEY: &str = "sk_test_fake_counteractive";
/// How long `down` waits for each process after SIGTERM.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// fake-engine's pause per token when `host` runs it, as the demo's.
const HOST_FAKE_TOKEN_DELAY_MS: u64 = 40;
/// How often `host` says it is still waiting for the onion service.
const ONION_REPORT: Duration = Duration::from_secs(30);

/// What `up` started.
#[derive(Debug, Serialize, Deserialize)]
struct UpState {
    /// Whether the storefront talks to real Stripe instead of fake-stripe.
    real_stripe: bool,
    /// The storefront's address.
    storefront: SocketAddr,
    /// fake-stripe's address, when it runs.
    fake_stripe: Option<SocketAddr>,
    /// Every process, in start order.
    processes: Vec<Recorded>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Recorded {
    name: String,
    pid: u32,
}

/// Starts fake-engine, fake-stripe (unless `real_stripe`), the gate and
/// the storefront in the background, and writes the wallet's pins.
pub async fn up(tree: &Tree, real_stripe: bool) -> anyhow::Result<()> {
    if tree.up_state().exists() {
        bail!("{} exists: the stack is already up; run `xtask down` first", tree.up_state().display());
    }
    let keys = Keys::read(tree)?;
    let binaries = Binaries::build(false)?;

    // Started services are killed again if a later one fails.
    let engine = stack::engine(&binaries, tree, 0).await?;
    let (stripe, stripe_base_url) = if real_stripe {
        (None, STRIPE_API.to_owned())
    } else {
        let (service, addr) = stack::fake_stripe(&binaries, tree, None).await?;
        (Some((service, addr)), format!("http://{addr}"))
    };
    let gate = stack::gate(&binaries, tree, &tree.release()).await?;
    let (storefront, storefront_addr) = stack::storefront(&binaries, tree, None, &stripe_base_url, None).await?;
    stack::write_wallet_pins(tree, &keys, storefront_addr)?;

    let fake_stripe = stripe.as_ref().map(|(_, addr)| *addr);
    let mut services = vec![engine];
    services.extend(stripe.map(|(service, _)| service));
    services.extend([gate, storefront]);
    detach_all(
        tree,
        UpState { real_stripe, storefront: storefront_addr, fake_stripe, processes: Vec::new() },
        services,
    )?;
    println!("storefront: {storefront_addr}");
    match fake_stripe {
        Some(addr) => println!("fake-stripe: http://{addr}"),
        None => println!("stripe: {STRIPE_API} (the key comes from ${STRIPE_KEY_VAR} at unlock)"),
    }
    println!("wallet pins: {}", tree.wallet().join(wallet_core::pins::PINS_FILE).display());
    println!("next: `cargo run -p xtask -- unlock`");
    Ok(())
}

/// Records `services` in `up.json` and leaves them running after xtask.
fn detach_all(tree: &Tree, mut state: UpState, services: Vec<Service>) -> anyhow::Result<()> {
    state.processes =
        services.iter().map(|service| Recorded { name: service.name().into(), pid: service.pid() }).collect();
    std::fs::write(tree.up_state(), serde_json::to_vec_pretty(&state)?)?;
    for service in services {
        println!("{} running (pid {}), log {}", service.name(), service.pid(), service.log().display());
        service.detach();
    }
    Ok(())
}

/// What `xtask host` starts.
#[derive(Debug, Clone)]
pub struct HostArgs {
    /// Where the storefront listens; the operator's tunnel forwards it.
    pub storefront: SocketAddr,
    /// Where fake-stripe listens; the tunnel forwards it for the buyer's
    /// browser, so checkout URLs name this address.
    pub fake_stripe: SocketAddr,
    /// Where checkout sends a buyer who paid.
    pub success_url: Option<String>,
    /// Start fake-engine instead of using the engine on `run/engine.sock`.
    pub fake_engine: bool,
    /// The C tor program that publishes the gate's onion service.
    pub tor: PathBuf,
}

/// Starts fake-stripe, the gate, the storefront and C tor in the
/// background around the engine already serving `run/engine.sock` (or a
/// fake-engine it starts), and records them for `down`.
///
/// It reads only `release.json` and `keys/dev_attest.key`. The gate stays
/// Locked and the storefront unconfigured until the operator, holding K,
/// the operator key and the storefront token elsewhere, unlocks them
/// through a tunnel (`xtask remote unlock`).
pub async fn host(tree: &Tree, args: &HostArgs) -> anyhow::Result<()> {
    if tree.up_state().exists() {
        bail!("{} exists: the stack is already up; run `xtask down` first", tree.up_state().display());
    }
    for path in [tree.release(), tree.dev_attest_key()] {
        ensure!(path.is_file(), "{} is missing; `xtask remote deploy` copies it here", path.display());
    }
    let binaries = Binaries::build(false)?;

    let mut services = Vec::new();
    if args.fake_engine {
        services.push(stack::engine(&binaries, tree, HOST_FAKE_TOKEN_DELAY_MS).await?);
    } else {
        let socket = tree.engine_socket();
        ensure!(
            std::os::unix::net::UnixStream::connect(&socket).is_ok(),
            "no engine accepts on {}: start vLLM first (deploy/verda/start.sh does), or pass --fake-engine",
            socket.display()
        );
    }
    let (stripe, stripe_addr) = stack::fake_stripe(&binaries, tree, Some(args.fake_stripe)).await?;
    services.push(stripe);
    services.push(stack::gate(&binaries, tree, &tree.release()).await?);
    let (storefront, storefront_addr) = stack::storefront(
        &binaries,
        tree,
        Some(args.storefront),
        &format!("http://{stripe_addr}"),
        args.success_url.as_deref(),
    )
    .await?;
    services.push(storefront);
    services.push(stack::tor(&args.tor, tree).await?);
    // C tor writes the hostname before the descriptor is out, and a wallet
    // that dials then can wait minutes; the stack is up once the gate
    // answers through its onion service, in whatever state.
    let onion = tree.onion()?.context("C tor wrote no onion hostname")?;
    let target = stack::onion_target(&onion).parse()?;
    let started = Instant::now();
    let mut reported = Duration::ZERO;
    let state = crate::e2e::tier2::wait_for_answer(tree, &target, |waited, error| {
        if waited >= reported + ONION_REPORT {
            reported = waited;
            println!("the onion service doesn't answer yet after {} s: {error:#}", waited.as_secs());
        }
    })
    .await?;
    println!("the gate answers through its onion service ({state:?}) after {:.1} s", started.elapsed().as_secs_f64());

    let state = UpState {
        real_stripe: false,
        storefront: storefront_addr,
        fake_stripe: Some(stripe_addr),
        processes: Vec::new(),
    };
    detach_all(tree, state, services)?;
    println!("storefront: {storefront_addr}, fake-stripe: http://{stripe_addr}");
    println!("onion service: {onion}");
    println!("the gate is Locked: unlock it from the operator's machine with `xtask remote unlock`");
    Ok(())
}

/// Unlocks the gate (signing the manifest on the first boot) and gives
/// the storefront its Stripe key. A real key is read from
/// `STRIPE_SECRET_KEY` and goes only to the attested storefront: never to
/// disk, logs or the command line.
pub async fn unlock(tree: &Tree) -> anyhow::Result<()> {
    let state = read_state(tree)?;
    let keys = Keys::read(tree)?;
    let signed = operator::unlock_gate(tree, &keys).await?;
    let version = manifest::verify_signed(&signed, &keys.operator_public_key()?)?.version;
    println!("gate ready, manifest version {version}");

    let stripe_key = stripe_key(state.real_stripe)?;
    let target = format!("tcp:{}", state.storefront).parse()?;
    operator::configure_storefront(tree, &keys, &target, stripe_key).await?;
    println!("storefront ready");
    Ok(())
}

/// The key for the storefront, checked against how `up` started it.
fn stripe_key(real_stripe: bool) -> anyhow::Result<SecretString> {
    let from_env = std::env::var(STRIPE_KEY_VAR).ok().map(SecretString::from);
    match (real_stripe, from_env) {
        (true, Some(key)) => {
            // A prototype never takes live money.
            if !key.expose_secret().starts_with("sk_test_") && !key.expose_secret().starts_with("rk_test_") {
                bail!("${STRIPE_KEY_VAR} must be a test-mode key (sk_test_ or rk_test_)");
            }
            Ok(key)
        }
        (true, None) => bail!("the stack runs against real Stripe: set ${STRIPE_KEY_VAR} to a sandbox key"),
        (false, Some(_)) => bail!(
            "the storefront talks to fake-stripe; unset ${STRIPE_KEY_VAR}, or restart with `xtask down` and `xtask up --real-stripe`"
        ),
        (false, None) => Ok(SecretString::from(FAKE_STRIPE_KEY)),
    }
}

/// Stops what `up` started, newest first: SIGTERM, then SIGKILL after a wait.
pub async fn down(tree: &Tree) -> anyhow::Result<()> {
    let state = read_state(tree)?;
    for process in state.processes.iter().rev() {
        if !process::is_running(process.pid, &process.name) {
            println!("{} (pid {}) is not running", process.name, process.pid);
            continue;
        }
        process::terminate(process.pid)?;
        let deadline = Instant::now() + STOP_TIMEOUT;
        while process::is_running(process.pid, &process.name) {
            if Instant::now() > deadline {
                process::kill(process.pid)?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!("{} stopped", process.name);
    }
    std::fs::remove_file(tree.up_state()).with_context(|| format!("removing {}", tree.up_state().display()))?;
    Ok(())
}

fn read_state(tree: &Tree) -> anyhow::Result<UpState> {
    let bytes = std::fs::read(tree.up_state())
        .with_context(|| format!("reading {}: run `xtask up` first", tree.up_state().display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}
