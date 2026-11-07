//! Which dialer reaches the pinned gate target. walletd has only two: the
//! Unix socket (tier 1) and Arti (tier 2, feature `arti`). It never dials
//! TCP, so a clearnet route to the gate can't exist (ARCHITECTURE 8.2 rule 5).

use std::path::Path;

use anyhow::{Context as _, bail};
use gate_conn::{Dialer, Target, UnixDialer};

/// The dialer for `target`. An onion target bootstraps Arti, keeping its
/// state under `<wallet_dir>/arti/`.
pub async fn for_target(target: &Target, wallet_dir: &Path) -> anyhow::Result<Box<dyn Dialer>> {
    match target {
        Target::Unix(_) => Ok(Box::new(UnixDialer)),
        Target::Onion { .. } => arti(wallet_dir).await,
        Target::Tcp { .. } => bail!("walletd never dials TCP; the gate target must be unix: or onion:"),
    }
}

#[cfg(feature = "arti")]
async fn arti(wallet_dir: &Path) -> anyhow::Result<Box<dyn Dialer>> {
    let root = wallet_dir.join("arti");
    tracing::info!("bootstrapping Tor");
    let dialer = gate_conn::ArtiDialer::bootstrap(&root.join("state"), &root.join("cache"))
        .await
        .context("bootstrapping Tor")?;
    Ok(Box::new(dialer))
}

#[cfg(not(feature = "arti"))]
#[expect(clippy::unused_async, reason = "the same signature as the arti build's")]
async fn arti(_wallet_dir: &Path) -> anyhow::Result<Box<dyn Dialer>> {
    Err(anyhow::anyhow!("this walletd was built without Tor; rebuild with `--features arti`"))
        .context("the gate target is an onion service")
}
