//! Starting each service of the stack, shared by `up`, `host` and `e2e`.
//!
//! TCP services listen on port 0 and print the address they got, unless
//! `host` gives them the fixed ports an SSH tunnel forwards.

use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wallet_core::Pins;

use anyhow::ensure;

use crate::keys::Keys;
use crate::process::{Binaries, START_TIMEOUT, Service};
use crate::release::{KEY_EPOCH, SERVICE_ID, TIER_LADDER};
use crate::tree::{Tree, create_private_dir};

/// Any free loopback port.
const ANY_PORT: &str = "127.0.0.1:0";
/// The port the gate's onion service offers (tier 2).
const ONION_PORT: u16 = 443;
/// How long walletd may take to start when it bootstraps Arti first.
const ARTI_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(180);

/// Starts `fake-engine` on `run/engine.sock`, pausing `token_delay_ms`
/// before each token it streams.
pub async fn engine(binaries: &Binaries, tree: &Tree, token_delay_ms: u64) -> anyhow::Result<Service> {
    create_private_dir(&tree.run())?;
    let (socket, delay) = (tree.engine_socket(), token_delay_ms.to_string());
    let args = [os("--socket"), socket.as_os_str(), os("--token-delay-ms"), os(&delay)];
    let mut service = Service::spawn(binaries, "fake-engine", &args, &tree.logs())?;
    service.accepting(&[socket]).await?;
    Ok(service)
}

/// Starts `fake-stripe` on `listen`, or on a free port when `None`.
pub async fn fake_stripe(
    binaries: &Binaries,
    tree: &Tree,
    listen: Option<SocketAddr>,
) -> anyhow::Result<(Service, SocketAddr)> {
    let skip = log_len(&tree.logs(), "fake-stripe");
    let listen = listen_arg(listen);
    let mut service = Service::spawn(binaries, "fake-stripe", &[os("--listen"), os(&listen)], &tree.logs())?;
    let addr = service.listening(skip, START_TIMEOUT).await?;
    Ok((service, addr))
}

/// Starts the gate on `release`, Locked, and waits for both sockets.
pub async fn gate(binaries: &Binaries, tree: &Tree, release: &Path) -> anyhow::Result<Service> {
    create_private_dir(&tree.gate_data())?;
    let (data, run, engine, key) = (tree.gate_data(), tree.run(), tree.engine_socket(), tree.dev_attest_key());
    let args = [
        os("--release"),
        release.as_os_str(),
        os("--data"),
        data.as_os_str(),
        os("--run"),
        run.as_os_str(),
        os("--engine"),
        engine.as_os_str(),
        os("--dev-attest-key"),
        key.as_os_str(),
    ];
    let mut service = Service::spawn(binaries, "gate", &args, &tree.logs())?;
    service.accepting(&[tree.gate_socket(), tree.issue_socket()]).await?;
    Ok(service)
}

/// Starts the storefront on `listen` (a free port when `None`),
/// unconfigured, talking to Stripe at `stripe_base_url`. Checkout sends a
/// buyer who paid to `success_url`, or to the storefront's default
/// (walletd's `/paid` page) when `None`.
pub async fn storefront(
    binaries: &Binaries,
    tree: &Tree,
    listen: Option<SocketAddr>,
    stripe_base_url: &str,
    success_url: Option<&str>,
) -> anyhow::Result<(Service, SocketAddr)> {
    let (release, issue, key) = (tree.release(), tree.issue_socket(), tree.dev_attest_key());
    let listen = listen_arg(listen);
    let mut args = vec![
        os("--release"),
        release.as_os_str(),
        os("--listen"),
        os(&listen),
        os("--issue-socket"),
        issue.as_os_str(),
        os("--stripe-base-url"),
        os(stripe_base_url),
        os("--manifest"),
        tree.root().as_os_str(),
        os("--dev-attest-key"),
        key.as_os_str(),
    ];
    if let Some(url) = success_url {
        args.extend([os("--success-url"), os(url)]);
    }
    let skip = log_len(&tree.logs(), "storefront");
    let mut service = Service::spawn(binaries, "storefront", &args, &tree.logs())?;
    let addr = service.listening(skip, START_TIMEOUT).await?;
    Ok((service, addr))
}

/// Starts walletd for the tree's wallet on a free port. Behind an onion
/// service it bootstraps Arti before it listens, which takes longer.
pub async fn walletd(binaries: &Binaries, tree: &Tree) -> anyhow::Result<(Service, SocketAddr)> {
    walletd_for(binaries, tree, tree.onion()?.is_some()).await
}

/// [`walletd`] for a wallet whose pins name an onion service when
/// `over_tor`, wherever that service runs.
pub async fn walletd_for(binaries: &Binaries, tree: &Tree, over_tor: bool) -> anyhow::Result<(Service, SocketAddr)> {
    let wallet = tree.wallet();
    let args = [os("--wallet-dir"), wallet.as_os_str(), os("--listen"), os(ANY_PORT)];
    let skip = log_len(&tree.logs(), "walletd");
    let mut service = Service::spawn(binaries, "walletd", &args, &tree.logs())?;
    let timeout = if over_tor { ARTI_BOOTSTRAP_TIMEOUT } else { START_TIMEOUT };
    let addr = service.listening(skip, timeout).await?;
    Ok((service, addr))
}

/// Starts C tor with an onion service in front of the gate's socket and a
/// SOCKS socket for xtask's own probes, and waits for the onion address.
/// The gate needn't be running yet: tor connects to it per stream.
pub async fn tor(program: &Path, tree: &Tree) -> anyhow::Result<Service> {
    let dir = tree.tor();
    create_private_dir(&dir)?;
    let paths = [tor_socks(tree), dir.join("data"), dir.join("onion"), tree.gate_socket()];
    // torrc values end at whitespace; the temp dirs xtask makes have none.
    ensure!(
        paths.iter().all(|path| !path.to_string_lossy().contains(char::is_whitespace)),
        "tor paths must not contain whitespace: {}",
        dir.display()
    );
    let [socks, data, onion, gate] = paths.map(|path| path.display().to_string());
    let torrc = dir.join("torrc");
    std::fs::write(
        &torrc,
        format!(
            "SocksPort unix:{socks}\nDataDirectory {data}\nHiddenServiceDir {onion}\n\
             HiddenServicePort {ONION_PORT} unix:{gate}\n"
        ),
    )?;
    let mut service = Service::spawn_program(program, "tor", &[os("-f"), torrc.as_os_str()], &tree.logs())?;
    service.file_written(&tree.onion_hostname()).await?;
    Ok(service)
}

/// C tor's SOCKS socket.
pub fn tor_socks(tree: &Tree) -> PathBuf {
    tree.tor().join("socks.sock")
}

/// Writes `wallet/wallet.toml`: the pins a wallet release would compile in.
/// The gate is the tree's onion service if it has one, else its socket.
pub fn write_wallet_pins(tree: &Tree, keys: &Keys, storefront: SocketAddr) -> anyhow::Result<()> {
    let gate_target = match tree.onion()? {
        Some(host) => onion_target(&host),
        None => format!("unix:{}", tree.gate_socket().display()),
    };
    write_pins(&tree.wallet(), keys, gate_target, storefront)
}

/// Writes `wallet.toml` into `wallet` with these targets.
pub fn write_pins(wallet: &Path, keys: &Keys, gate_target: String, storefront: SocketAddr) -> anyhow::Result<()> {
    create_private_dir(wallet)?;
    let pins = Pins {
        service_id: SERVICE_ID.into(),
        key_epoch: KEY_EPOCH,
        gate_target,
        storefront_addr: storefront.to_string(),
        operator_public_key: keys.operator_public_key()?.as_base64().to_owned(),
        tier_ladder: TIER_LADDER.to_vec(),
        allow_dev_attestation: true,
    };
    std::fs::write(wallet.join(wallet_core::pins::PINS_FILE), pins.to_toml()?)?;
    Ok(())
}

/// The pins' target for the gate's onion service on `host`.
pub fn onion_target(host: &str) -> String {
    format!("onion:{host}:{ONION_PORT}")
}

fn os(text: &str) -> &OsStr {
    OsStr::new(text)
}

/// `--listen`'s value: `addr`, or any free loopback port.
fn listen_arg(addr: Option<SocketAddr>) -> String {
    addr.map_or_else(|| ANY_PORT.to_owned(), |addr| addr.to_string())
}

/// The current length of a service's log, so a restart reads only new lines.
fn log_len(logs: &Path, name: &str) -> u64 {
    std::fs::metadata(logs.join(format!("{name}.log"))).map_or(0, |metadata| metadata.len())
}
