//! `xtask remote`: the services on a rented GPU instance, a confidential
//! VM (AMD SEV-SNP) with an NVIDIA GPU in confidential-computing mode,
//! while this machine keeps the wallet and the operator's secrets.
//!
//! - `init`: keys, a release whose catalogue is one vLLM model, and an SSH
//!   key for the instance, in `.verda/`
//! - `deploy --host user@address`: copies the source, `release.json` and
//!   the dev attestation key over, installs Rust, tor, vLLM and the model
//!   (`deploy/verda/setup.sh`), records what the instance reports about
//!   its TEE (`attest.sh`), and starts vLLM, the gate, the storefront,
//!   fake-stripe and C tor (`start.sh`)
//! - `unlock`: through an SSH tunnel, attests and unlocks the gate with K,
//!   signs its issuer key into the manifest, and configures the storefront
//! - `tunnel`: holds the tunnel open, for `wallet-cli` by hand
//! - `status`, `down`
//!
//! `deploy --local DIR` rehearses all of it on this machine, with
//! fake-engine in place of vLLM.
//!
//! The instance gets only what it serves with: K, the operator's signing
//! key and the storefront token never leave this machine. They reach the
//! gate and the storefront as bearer tokens over attested connections
//! through the tunnel, as `xtask unlock` gives them to a local stack.
//!
//! Attestation is still dev-insecure: `attest.sh` records the SEV-SNP
//! report and the GPU's attestation result on the instance, but the gate
//! doesn't bind its key into them and the wallet doesn't verify them yet.

pub mod transport;

use std::path::PathBuf;

use anyhow::{Context as _, bail, ensure};
use gate_conn::Target;
use manifest::Manifest;
use proto::gate::GateState;
use proto::storefront::StorefrontState;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::keys::Keys;
use crate::release::{self, Catalogue, VllmModel};
use crate::tree::{Tree, create_private_dir};
use crate::updown::FAKE_STRIPE_KEY;
use crate::{operator, stack};
pub use transport::{Instance, Ports, Transport};

/// The remote settings, in the tree.
const CONFIG_FILE: &str = "remote.json";
/// The instance's attestation summary, as `attest.sh` writes it, relative
/// to its directory and to the tree here.
const ATTEST_SUMMARY: &str = "attest/summary.txt";

/// What `remote init` chose and `remote deploy` found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// The model vLLM serves.
    pub model: VllmModel,
    pub ports: Ports,
    /// Where the services run, once deployed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<Instance>,
}

impl RemoteConfig {
    /// The tree's settings.
    pub fn read(tree: &Tree) -> anyhow::Result<Self> {
        let path = tree.root().join(CONFIG_FILE);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading {}: run `cargo run -p xtask -- remote init` first", path.display()))?;
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
    }

    fn write(&self, tree: &Tree) -> anyhow::Result<()> {
        let path = tree.root().join(CONFIG_FILE);
        std::fs::write(&path, serde_json::to_vec_pretty(self)?).with_context(|| format!("writing {}", path.display()))
    }

    /// The deployed instance.
    pub fn instance(&self) -> anyhow::Result<&Instance> {
        self.instance.as_ref().context("no instance yet: run `cargo run -p xtask -- remote deploy --host user@address`")
    }

    /// The storefront's dial target, through the tunnel.
    pub fn storefront_target(&self) -> anyhow::Result<Target> {
        Ok(format!("tcp:{}", self.ports.storefront()).parse()?)
    }
}

/// Makes the keys, the vLLM release and the SSH key, keeping any that
/// exist, and records the model and ports.
pub fn init(tree: &Tree, model: VllmModel, ports: Ports) -> anyhow::Result<()> {
    for value in [&model.id, &model.weights] {
        // These go into the instance's ca.env, which bash sources.
        ensure!(
            !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c)),
            "{value:?} must be letters, digits and . _ / - only"
        );
    }
    create_private_dir(tree.root())?;
    let keys = if tree.k().exists() { Keys::read(tree)? } else { Keys::generate(tree)? };
    release::write_catalogue(tree, &keys, &Catalogue::Vllm(model.clone()))?;

    let key = transport::ssh_key(tree);
    if !key.exists() {
        create_private_dir(key.parent().context("the SSH key has no directory")?)?;
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "counteractive-remote", "-f"])
            .arg(&key)
            .status()
            .context("running ssh-keygen")?;
        ensure!(status.success(), "ssh-keygen failed ({status})");
    }
    println!("keys and release.json in {}: {} serves {}", tree.root().display(), model.id, model.weights);
    let instance = RemoteConfig::read(tree).ok().and_then(|config| config.instance);
    RemoteConfig { model, ports, instance }.write(tree)?;

    println!("the instance's SSH public key ({}.pub):", key.display());
    print!("{}", std::fs::read_to_string(key.with_extension("pub"))?);
    println!(
        "next: add it when you create the instance, then `cargo run -p xtask -- remote deploy --host root@ADDRESS`"
    );
    Ok(())
}

/// Where `remote deploy` puts the stack.
#[derive(Debug, Clone)]
pub struct DeployArgs {
    /// `user@address` of a new instance; the recorded one when `None`.
    pub host: Option<String>,
    /// A directory to rehearse in instead.
    pub local: Option<PathBuf>,
    /// Skip `setup.sh`, on an instance it already ran on.
    pub skip_setup: bool,
}

/// Copies what the instance needs, sets it up, records its attestation and
/// starts the stack, then fetches the onion address the manifest names.
pub fn deploy(tree: &Tree, args: DeployArgs) -> anyhow::Result<()> {
    let mut config = RemoteConfig::read(tree)?;
    let instance = match (args.host, args.local) {
        (Some(destination), None) => {
            let probe = Transport::new(tree, Instance::Ssh { destination: destination.clone(), home: String::new() });
            let home = probe.output("printf %s \"$HOME\"").context("reaching the instance over SSH")?;
            ensure!(home.starts_with('/'), "the instance's $HOME is {home:?}");
            Instance::Ssh { destination, home }
        }
        (None, Some(root)) => {
            create_private_dir(&root)?;
            let root = root.canonicalize()?;
            // Unix socket paths are limited to about 104 bytes.
            ensure!(
                root.join("tree/run/engine.sock").as_os_str().len() < 100,
                "{} is too long a path for the sockets under it; try one in /tmp",
                root.display()
            );
            Instance::Local { root }
        }
        (None, None) => config.instance()?.clone(),
        (Some(_), Some(_)) => bail!("--host and --local exclude each other"),
    };
    config.instance = Some(instance.clone());
    config.write(tree)?;
    let transport = Transport::new(tree, instance.clone());
    let rehearsal = matches!(instance, Instance::Local { .. });
    println!("deploying to {}", instance.describe());

    transport.push_source()?;
    transport.put("tree/release.json", &std::fs::read(tree.release())?, 0o644)?;
    transport.put("tree/keys/dev_attest.key", &Zeroizing::new(std::fs::read(tree.dev_attest_key())?), 0o600)?;
    transport.put("ca.env", env_file(&config, rehearsal).as_bytes(), 0o644)?;
    println!("copied the source, release.json, the dev attestation key and ca.env");

    if !rehearsal && !args.skip_setup {
        println!("setting up the instance: packages, Rust, the services, vLLM and the model (10 to 20 minutes)");
        transport.run(&script("setup.sh"))?;
    }
    transport.run(&script("attest.sh"))?;
    transport.run(&script("start.sh"))?;

    let onion = transport.output("cat tree/tor/onion/hostname")?;
    let onion = onion.trim();
    // A v3 onion address: 56 base32 characters.
    ensure!(
        onion.strip_suffix(".onion").is_some_and(|host| host.len() == 56),
        "the instance's onion hostname is {onion:?}"
    );
    // The manifest and the wallet's pins name it, as for a local C tor.
    create_private_dir(&tree.tor().join("onion"))?;
    std::fs::write(tree.onion_hostname(), format!("{onion}\n"))?;
    let summary = fetch_attestation(tree, &transport)?;
    println!("the gate's onion service: {onion}");
    println!("what the instance reports about its TEE (not verified by the wallet yet):\n{summary}");
    println!("next: `cargo run -p xtask -- demo --remote`, or `remote unlock` alone");
    Ok(())
}

/// `ca.env`, which the deploy scripts read: the model, the ports and the
/// engine. Every value is a plain word, so bash can source it.
fn env_file(config: &RemoteConfig, rehearsal: bool) -> String {
    let (model, ports) = (&config.model, config.ports);
    format!(
        "# Written by `xtask remote deploy`; deploy/verda/*.sh read it.\n\
         MODEL_ID={}\nMODEL_WEIGHTS={}\nMODEL_CONTEXT={}\n\
         STOREFRONT_LISTEN={}\nFAKE_STRIPE_LISTEN={}\nSUCCESS_URL={}\n\
         ENGINE={}\nREQUIRE_TEE={}\n",
        model.id,
        model.weights,
        model.context,
        ports.storefront(),
        ports.fake_stripe(),
        ports.paid_url(),
        if rehearsal { "fake" } else { "vllm" },
        u8::from(!rehearsal),
    )
}

/// The command running one of `deploy/verda`'s scripts on the instance.
fn script(name: &str) -> String {
    format!("bash \"${{CA_SRC:-$CA/src}}/deploy/verda/{name}\"")
}

/// Copies the instance's attestation summary into the tree and returns it.
pub fn fetch_attestation(tree: &Tree, transport: &Transport) -> anyhow::Result<String> {
    let summary = transport.output(&format!("cat tree/{ATTEST_SUMMARY}"))?;
    let path = tree.root().join(ATTEST_SUMMARY);
    create_private_dir(path.parent().context("no attest directory")?)?;
    std::fs::write(&path, &summary)?;
    Ok(summary)
}

/// Unlocks the instance's gate and configures its storefront through a
/// tunnel, then writes the wallet's pins in the tree.
pub async fn unlock(tree: &Tree) -> anyhow::Result<()> {
    let config = RemoteConfig::read(tree)?;
    let keys = Keys::read(tree)?;
    let transport = Transport::new(tree, config.instance()?.clone());
    let _tunnel = transport.tunnel(tree, config.ports).await?;
    make_ready(tree, &keys, &config, &transport, &|line| println!("{line}")).await?;
    stack::write_wallet_pins(tree, &keys, config.ports.storefront())?;
    println!("wallet pins: {}", tree.wallet().join(wallet_core::pins::PINS_FILE).display());
    Ok(())
}

/// Unlocks the gate through the tunnel if it is Locked, signing a manifest
/// for its issuer key on its first unlock; puts the manifest where the
/// storefront serves it from; and configures the storefront if it isn't.
/// Returns the manifest. Each step is idempotent, so a second run after a
/// restart, or after a run that failed halfway, finishes the job.
pub async fn make_ready(
    tree: &Tree,
    keys: &Keys,
    config: &RemoteConfig,
    transport: &Transport,
    report: &(dyn Fn(String) + Sync),
) -> anyhow::Result<Manifest> {
    let before = operator::gate_health(tree).await.context("asking the gate through the tunnel")?;
    let signed = operator::unlock_gate(tree, keys).await?;
    let manifest = manifest::verify_signed(&signed, &keys.operator_public_key()?)?;
    report(if before == GateState::Ready {
        format!("the gate was already unlocked, under manifest v{}", manifest.version)
    } else {
        format!(
            "attested the gate, unlocked it with K through the tunnel, and posted manifest v{} naming its issuer key",
            manifest.version
        )
    });
    transport.put("tree/manifest.json", signed.manifest.as_bytes(), 0o644)?;
    transport.put("tree/manifest.sig", signed.signature.as_bytes(), 0o644)?;

    let storefront = config.storefront_target()?;
    if operator::storefront_health(&storefront).await? == StorefrontState::Ready {
        report("the storefront was already configured".into());
    } else {
        operator::configure_storefront(tree, keys, &storefront, FAKE_STRIPE_KEY.into()).await?;
        report("attested the storefront and gave it fake-stripe's key".into());
    }
    Ok(manifest)
}

/// Holds the tunnel open until Ctrl-C, for `wallet-cli` against the
/// instance by hand.
pub async fn tunnel(tree: &Tree) -> anyhow::Result<()> {
    let config = RemoteConfig::read(tree)?;
    let transport = Transport::new(tree, config.instance()?.clone());
    let _tunnel = transport.tunnel(tree, config.ports).await?;
    println!(
        "tunnel up: storefront {}, fake-stripe http://{}, gate {}; Ctrl-C closes it",
        config.ports.storefront(),
        config.ports.fake_stripe(),
        tree.gate_socket().display()
    );
    tokio::signal::ctrl_c().await.context("waiting for Ctrl-C")
}

/// Prints the instance's attestation summary and what runs there.
pub fn status(tree: &Tree) -> anyhow::Result<()> {
    let config = RemoteConfig::read(tree)?;
    let transport = Transport::new(tree, config.instance()?.clone());
    println!("instance: {}", transport.instance().describe());
    transport.run(&script("status.sh"))
}

/// Stops vLLM and the services on the instance. The instance itself keeps
/// running, and billing, until it is deleted in the provider's console.
pub fn down(tree: &Tree) -> anyhow::Result<()> {
    let config = RemoteConfig::read(tree)?;
    let transport = Transport::new(tree, config.instance()?.clone());
    transport.run(&script("stop.sh"))?;
    if let Instance::Ssh { .. } = transport.instance() {
        println!("stopped; the instance still runs and bills until you delete it in the provider's console");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_file_is_plain_words() {
        let config = RemoteConfig {
            model: VllmModel {
                id: "qwen3-4b".into(),
                weights: "Qwen/Qwen3-4B-Instruct-2507".into(),
                context: 16384,
                max_output: 2048,
                input_rate: 1,
                output_rate: 3,
            },
            ports: Ports { storefront: 8443, fake_stripe: 8090, console: 7700 },
            instance: None,
        };
        let env = env_file(&config, false);
        assert!(env.contains("MODEL_WEIGHTS=Qwen/Qwen3-4B-Instruct-2507\n"));
        assert!(env.contains("STOREFRONT_LISTEN=127.0.0.1:8443\n"));
        assert!(env.contains("SUCCESS_URL=http://127.0.0.1:7700/paid\n"));
        assert!(env.contains("ENGINE=vllm\nREQUIRE_TEE=1\n"));
        assert!(env_file(&config, true).contains("ENGINE=fake\nREQUIRE_TEE=0\n"));
        for line in env.lines().filter(|line| !line.starts_with('#')) {
            let (_, value) = line.split_once('=').unwrap();
            assert!(!value.contains(char::is_whitespace) && !value.contains(['\'', '"', '$', '`']), "{line}");
        }
    }
}
