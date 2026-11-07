//! Step 12: steps 3 to 10 again, with the wallet reaching the gate only as
//! an onion service of a local C tor, through walletd's Arti dialer.
//!
//! This uses the live Tor network: C tor publishes the gate's onion
//! service and Arti connects to it through Tor relays. xtask's own gate
//! clients (step 7's direct proofs) go through C tor's SOCKS socket. Only
//! the operator's unlock after a gate restart stays on the gate's local
//! socket, as an operator on the host would do it.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use async_trait::async_trait;
use attest::{PlatformVerifier, Role};
use gate_conn::{AttestedConn, BoxedIo, DialError, DialScope, Dialer, Target, UnverifiedConn};
use manifest::Manifest;
use proto::gate::{GateHealth, GateState};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;
use wallet_core::Pins;

use super::world::World;
use super::{STEPS, failures, ok, purchase, sessions};
use crate::process::Binaries;
use crate::stack;
use crate::tree::Tree;

/// How long C tor may take to bootstrap and publish the onion service.
const ONION_TIMEOUT: Duration = Duration::from_secs(300);
/// How often the onion service is probed until it answers.
const ONION_POLL: Duration = Duration::from_secs(5);

/// The idle timer of tier 2's `release-idle.json`. Tier 1 uses 2 s, but
/// behind Tor one `wallet-cli chat` takes several onion round trips after
/// the gate has already started that session's timer (the open, the turn,
/// then walletd answering), so 2 s could expire the session before step 9
/// means it to. 10 s still lets the gate close it on its own well within
/// the step.
pub const IDLE_SECONDS: u64 = 10;

/// Brings up a second stack with the gate behind C tor and runs steps 3
/// to 10 against it. Its temp dir is kept if anything fails.
pub async fn run(binaries: &Binaries, tor: &Path) -> anyhow::Result<()> {
    let mut world = World::bring_up(binaries, Some(tor)).await?;
    let result = async {
        let target = wait_for_onion(&world.tree, |_, error| {
            tracing::info!("the onion service does not answer yet: {error:#}");
        })
        .await?;
        ok(format!("the gate answers `ready` at {target} through C tor"));
        steps(&mut world).await
    }
    .await;
    if let Some(dir) = world.finish(result.is_ok()).await {
        println!("    tier 2 logs and state kept in {}", dir.display());
    }
    result
}

/// Steps 3 to 10, in order, each named in the output and in its error.
async fn steps(world: &mut World) -> anyhow::Result<()> {
    over_tor(3, purchase::purchase(world)).await?;
    over_tor(4, purchase::issuance_idempotency(world)).await?;
    over_tor(5, sessions::chat_and_metering(world)).await?;
    over_tor(6, sessions::close_and_reconcile(world)).await?;
    over_tor(7, sessions::replay(world)).await?;
    over_tor(8, failures::gate_crash(world)).await?;
    over_tor(9, failures::idle_close(world)).await?;
    over_tor(10, failures::budget_errors(world)).await
}

/// Runs tier 1's step `number` against the stack behind Tor, printing its
/// name before and how long it took after.
async fn over_tor(number: usize, step: impl Future<Output = anyhow::Result<()>>) -> anyhow::Result<()> {
    let name = STEPS[number - 1];
    println!("  step {number} over Tor: {name}");
    let started = Instant::now();
    step.await.with_context(|| format!("step {number} ({name}) over Tor"))?;
    println!("  step {number} over Tor: passed in {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}

/// An attested connection to the gate through its onion service, made
/// with C tor's SOCKS socket, for the checks xtask makes as a client of
/// its own.
pub async fn attest_through_tor(tree: &Tree, manifest: &Manifest) -> anyhow::Result<AttestedConn> {
    let target = gate_onion(tree)?;
    let dialer = SocksDialer { socket: stack::tor_socks(tree) };
    let conn = UnverifiedConn::connect(&dialer, &target, Role::Gate, &DialScope::new()).await?;
    Ok(conn.attest(&PlatformVerifier::new(true), manifest).await?)
}

/// The gate's onion address, as the wallet's pins name it.
fn gate_onion(tree: &Tree) -> anyhow::Result<Target> {
    Ok(Pins::read(&tree.wallet())?.gate_target.parse()?)
}

/// Waits until the gate answers `ready` through its onion service, so
/// walletd's first turn doesn't race C tor publishing the descriptor.
/// `on_miss` hears each failed probe, with how long the wait has run.
/// Returns the onion target.
pub async fn wait_for_onion(tree: &Tree, on_miss: impl FnMut(Duration, &anyhow::Error)) -> anyhow::Result<Target> {
    let target = gate_onion(tree)?;
    let state = wait_for_answer(tree, &target, on_miss).await?;
    if state != GateState::Ready {
        bail!("the gate is {state:?} through its onion service");
    }
    Ok(target)
}

/// Waits until the gate answers its health check at `target` through C
/// tor, in whatever state, and returns that state.
pub async fn wait_for_answer(
    tree: &Tree,
    target: &Target,
    mut on_miss: impl FnMut(Duration, &anyhow::Error),
) -> anyhow::Result<GateState> {
    let dialer = SocksDialer { socket: stack::tor_socks(tree) };
    let started = Instant::now();
    let deadline = started + ONION_TIMEOUT;
    loop {
        match health(&dialer, target).await {
            Ok(state) => return Ok(state),
            Err(error) if Instant::now() < deadline => {
                on_miss(started.elapsed(), &error);
                tokio::time::sleep(ONION_POLL).await;
            }
            Err(error) => return Err(error).context("the gate's onion service never answered"),
        }
    }
}

async fn health(dialer: &SocksDialer, target: &Target) -> anyhow::Result<GateState> {
    let mut conn = UnverifiedConn::connect(dialer, target, Role::Gate, &DialScope::new()).await?;
    Ok(conn.health::<GateHealth>().await?.state)
}

/// Dials onion services through C tor's SOCKS socket: xtask's own client
/// of the onion service, independent of walletd's Arti.
struct SocksDialer {
    socket: PathBuf,
}

#[async_trait]
impl Dialer for SocksDialer {
    async fn dial(&self, target: &Target, _scope: &DialScope) -> Result<BoxedIo, DialError> {
        let Target::Onion { host, port } = target else {
            return Err(DialError::UnsupportedTarget(target.kind()));
        };
        let mut stream = UnixStream::connect(&self.socket).await?;
        socks5_connect(&mut stream, host, *port).await?;
        Ok(Box::new(stream))
    }
}

/// A SOCKS5 CONNECT by host name without authentication (RFC 1928).
async fn socks5_connect(stream: &mut UnixStream, host: &str, port: u16) -> io::Result<()> {
    stream.write_all(&[5, 1, 0]).await?;
    let mut method = [0; 2];
    stream.read_exact(&mut method).await?;
    if method != [5, 0] {
        return Err(io::Error::other("the SOCKS server wants authentication"));
    }
    let length = u8::try_from(host.len()).map_err(|_| io::Error::other("host name too long for SOCKS"))?;
    let mut request = vec![5, 1, 0, 3, length];
    request.extend_from_slice(host.as_bytes());
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;

    let mut reply = [0; 4];
    stream.read_exact(&mut reply).await?;
    if reply[1] != 0 {
        return Err(io::Error::other(format!("SOCKS CONNECT failed with reply code {}", reply[1])));
    }
    let address = match reply[3] {
        1 => 4,
        4 => 16,
        3 => usize::from(stream.read_u8().await?),
        other => return Err(io::Error::other(format!("SOCKS reply has address type {other}"))),
    };
    let mut bound = vec![0; address + 2];
    stream.read_exact(&mut bound).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::net::UnixListener;

    use super::*;

    /// A SOCKS server that accepts one CONNECT, checks it, replies with a
    /// domain-name bound address, then echoes one byte.
    async fn serve_once(listener: UnixListener, host: &'static str, port: u16) {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut greeting = [0; 3];
        stream.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting, [5, 1, 0]);
        stream.write_all(&[5, 0]).await.unwrap();
        let mut head = [0; 5];
        stream.read_exact(&mut head).await.unwrap();
        assert_eq!(head, [5, 1, 0, 3, u8::try_from(host.len()).unwrap()]);
        let mut rest = vec![0; host.len() + 2];
        stream.read_exact(&mut rest).await.unwrap();
        assert_eq!(&rest[..host.len()], host.as_bytes());
        assert_eq!(rest[host.len()..], port.to_be_bytes());
        stream.write_all(&[5, 0, 0, 3, 4, b'h', b'o', b's', b't', 0, 80]).await.unwrap();
        let byte = stream.read_u8().await.unwrap();
        stream.write_u8(byte).await.unwrap();
    }

    #[tokio::test]
    async fn socks5_connect_leaves_the_stream_at_the_payload() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("socks.sock");
        let server = tokio::spawn(serve_once(UnixListener::bind(&socket).unwrap(), "abc.onion", 443));
        let mut stream = UnixStream::connect(&socket).await.unwrap();
        socks5_connect(&mut stream, "abc.onion", 443).await.unwrap();
        stream.write_u8(42).await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), 42);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_refused_connect_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("socks.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 3];
            stream.read_exact(&mut buffer).await.unwrap();
            stream.write_all(&[5, 0]).await.unwrap();
            let mut request = [0; 5 + 9 + 2];
            stream.read_exact(&mut request).await.unwrap();
            // Reply 4: host unreachable, as tor answers for an unpublished onion.
            stream.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
        });
        let mut stream = UnixStream::connect(&socket).await.unwrap();
        let error = socks5_connect(&mut stream, "abc.onion", 443).await.unwrap_err();
        assert!(error.to_string().contains("reply code 4"), "{error}");
        server.await.unwrap();
    }
}
