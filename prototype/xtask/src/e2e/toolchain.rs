//! Step 11, the two-stack check on the dependency graph, and the tor
//! check that decides step 12.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, bail, ensure};

use super::ok;
use crate::process::workspace_root;

/// Step 11: the wallet binaries each link exactly one network stack, and
/// the gate links neither clearnet client.
pub fn two_stack_check() -> anyhow::Result<()> {
    let cli = tree(&["-p", "wallet-cli"])?;
    ensure!(!cli.contains("arti-client"), "wallet-cli depends on arti-client");
    ok("cargo tree -p wallet-cli: no arti-client");

    let walletd = tree(&["-p", "walletd", "-e", "features"])?;
    ensure!(!walletd.contains("reqwest"), "walletd depends on reqwest");
    ensure!(!walletd.contains("gate-conn feature \"tcp\""), "walletd enables gate-conn's tcp dialer");
    let control = tree(&["-p", "wallet-cli", "-e", "features"])?;
    ensure!(control.contains("gate-conn feature \"tcp\""), "control: the feature check found nothing in wallet-cli");
    ok("cargo tree -p walletd -e features: no reqwest, no gate-conn \"tcp\" (control: wallet-cli shows it)");

    let gate = tree(&["-p", "gate"])?;
    ensure!(!gate.contains("reqwest") && !gate.contains("arti-client"), "the gate depends on reqwest or arti-client");
    ok("cargo tree -p gate: no reqwest, no arti-client");
    Ok(())
}

/// The output of `cargo tree --locked` with these arguments.
pub(super) fn tree(args: &[&str]) -> anyhow::Result<String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("tree")
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .arg("--locked")
        .args(args)
        .output()
        .context("running cargo tree")?;
    if !output.status.success() {
        bail!("cargo tree {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Where `tor` is, per `command -v tor`.
pub fn tor_available() -> Option<PathBuf> {
    let output = Command::new("sh").args(["-c", "command -v tor"]).output().ok()?;
    let path = String::from_utf8(output.stdout).ok()?;
    (output.status.success() && !path.trim().is_empty()).then(|| PathBuf::from(path.trim()))
}
