//! `xtask release`: the measured release configs, and the signed manifest
//! that lists them (docs 07 sections 6 and 7).

use anyhow::{Context as _, bail};
use manifest::{AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, Release, Service, SignedManifest};
use proto::Blob;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::keys::Keys;
use crate::tree::Tree;

/// The service id every dev tree uses.
pub const SERVICE_ID: &str = "counteractive-dev";
/// The issuer key epoch.
pub const KEY_EPOCH: u32 = 1;
/// The wallet's session sizes (SPEC section 2).
pub const TIER_LADDER: [u64; 3] = [500_000, 50_000, 5_000];
/// The release's largest session.
pub const MAXIMUM_SPEND: u64 = 500_000;
/// The image version both dev releases claim.
const IMAGE_VERSION: &str = "0.1.0-dev";
/// The idle timer of the normal release (docs 07).
const IDLE_SECONDS: u64 = 900;
/// The idle timer of `release-idle.json`, for e2e step 9.
pub const SHORT_IDLE_SECONDS: u64 = 2;

/// Credits in each product.
pub const USD_5_CREDITS: u64 = 5_000_000;

/// A model's per-token rates.
#[derive(Debug, Clone, Copy)]
pub struct Rates {
    /// Credits per prompt token.
    pub input: u64,
    /// Credits per output token.
    pub output: u64,
}

/// `fake-small`'s rates.
pub const FAKE_SMALL: Rates = Rates { input: 1, output: 3 };
/// `fake-large`'s rates.
pub const FAKE_LARGE: Rates = Rates { input: 4, output: 12 };

/// What the release's engine serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "engine", rename_all = "snake_case")]
pub enum Catalogue {
    /// fake-engine's two models.
    Fake,
    /// One model on vLLM.
    Vllm(VllmModel),
}

/// A model vLLM serves, under the id clients and the gate use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VllmModel {
    /// The id in the catalogue, which vLLM serves as `--served-model-name`.
    pub id: String,
    /// The Hugging Face repository vLLM loads.
    pub weights: String,
    /// vLLM's `--max-model-len`, also the catalogue's `context`.
    pub context: u32,
    /// The most output one turn may ask for.
    pub max_output: u32,
    /// Credits per prompt token.
    pub input_rate: u64,
    /// Credits per output token.
    pub output_rate: u64,
}

impl Catalogue {
    /// The release's catalogue entries.
    fn entries(&self) -> Vec<Value> {
        match self {
            Self::Fake => vec![catalogue_entry("fake-small", FAKE_SMALL), catalogue_entry("fake-large", FAKE_LARGE)],
            Self::Vllm(model) => vec![json!({
                "id": model.id, "engine": "vllm", "usage_trust": "inside_attested_boundary",
                "input_rate": model.input_rate, "output_rate": model.output_rate,
                "max_output": model.max_output, "context": model.context
            })],
        }
    }
}

/// One release config: products, catalogue and session policy.
fn config(keys: &Keys, idle_seconds: u64, catalogue: &Catalogue) -> anyhow::Result<Value> {
    Ok(json!({
        "schema": 1,
        "image_version": IMAGE_VERSION,
        "service_id": SERVICE_ID,
        "key_epoch": KEY_EPOCH,
        "balance_width": BALANCE_WIDTH,
        "operator_public_key": keys.operator_public_key()?.as_base64(),
        "k_sha256": hex::encode(keys.k_sha256()),
        "products": [
            {"id": "usd-5", "usd_cents": 500, "credits": USD_5_CREDITS},
            {"id": "usd-1", "usd_cents": 100, "credits": 1_000_000}
        ],
        "storefront": {"port": 8443, "operator_token_sha256": hex::encode(keys.storefront_token_sha256())},
        "session_policy": {
            "maximum_spend": MAXIMUM_SPEND, "idle_seconds": idle_seconds, "max_turns": 64, "max_age_seconds": 3600
        },
        "catalogue": catalogue.entries()
    }))
}

fn catalogue_entry(id: &str, rates: Rates) -> Value {
    json!({
        "id": id, "engine": "fake", "usage_trust": "inside_attested_boundary",
        "input_rate": rates.input, "output_rate": rates.output, "max_output": 4096, "context": 4096
    })
}

/// Writes `release.json`, and `release-idle.json` too when `short_idle`
/// gives its idle timer in seconds (e2e: [`SHORT_IDLE_SECONDS`], or longer
/// behind Tor).
pub fn write(tree: &Tree, keys: &Keys, short_idle: Option<u64>) -> anyhow::Result<()> {
    write_config(&tree.release(), &config(keys, IDLE_SECONDS, &Catalogue::Fake)?)?;
    if let Some(idle_seconds) = short_idle {
        write_config(&tree.idle_release(), &config(keys, idle_seconds, &Catalogue::Fake)?)?;
    }
    Ok(())
}

/// Writes `release.json` serving `catalogue`, for a deployment.
pub fn write_catalogue(tree: &Tree, keys: &Keys, catalogue: &Catalogue) -> anyhow::Result<()> {
    write_config(&tree.release(), &config(keys, IDLE_SECONDS, catalogue)?)
}

fn write_config(path: &std::path::Path, config: &Value) -> anyhow::Result<()> {
    manifest::ReleaseConfig::from_value(config).context("the generated release config is invalid")?;
    std::fs::write(path, serde_json::to_vec_pretty(config)?).with_context(|| format!("writing {}", path.display()))
}

/// The release configs in the tree, normal first.
fn configs(tree: &Tree) -> anyhow::Result<Vec<Value>> {
    let mut configs = Vec::new();
    for path in [tree.release(), tree.idle_release()] {
        match std::fs::read(&path) {
            Ok(bytes) => {
                configs.push(serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }
    if configs.is_empty() {
        bail!("no release.json in {}: run `xtask release`", tree.root().display());
    }
    Ok(configs)
}

/// A manifest listing every release in the tree under this issuer key.
pub fn manifest(tree: &Tree, keys: &Keys, issuer_public_key: &[u8], version: u64) -> anyhow::Result<Manifest> {
    let dev_public_key = Blob::new(keys.dev_public_key().to_vec());
    let releases = configs(tree)?
        .into_iter()
        .map(|config| Release {
            image_version: IMAGE_VERSION.into(),
            platform: "dev-insecure".into(),
            attestation_policy: AttestationPolicy {
                kind: "dev-insecure".into(),
                dev_public_key: Some(dev_public_key.clone()),
            },
            release_config_sha256: manifest::release_config_sha256(&config),
            release_config: config,
        })
        .collect();
    Ok(Manifest {
        schema: manifest::SCHEMA,
        version,
        service: Service {
            id: SERVICE_ID.into(),
            onion: tree.onion()?,
            credit_scheme: CREDIT_SCHEME.into(),
            issuer_public_key: Blob::new(issuer_public_key.to_vec()),
            key_epoch: KEY_EPOCH,
            balance_width: BALANCE_WIDTH,
        },
        releases,
    })
}

/// Signs `manifest` and writes `manifest.json` and `manifest.sig`, which
/// the storefront serves.
pub fn sign_and_write(tree: &Tree, keys: &Keys, manifest: &Manifest) -> anyhow::Result<SignedManifest> {
    let signed = manifest::sign_manifest(manifest, keys.operator())?;
    std::fs::write(tree.manifest(), signed.manifest.as_bytes())
        .with_context(|| format!("writing {}", tree.manifest().display()))?;
    std::fs::write(tree.manifest_sig(), &signed.signature)
        .with_context(|| format!("writing {}", tree.manifest_sig().display()))?;
    Ok(signed)
}

/// The signed manifest in the tree, if one was written.
pub fn read_signed(tree: &Tree) -> anyhow::Result<Option<SignedManifest>> {
    let bytes = match std::fs::read(tree.manifest()) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", tree.manifest().display())),
    };
    let signature = std::fs::read_to_string(tree.manifest_sig())
        .with_context(|| format!("reading {}", tree.manifest_sig().display()))?;
    Ok(Some(SignedManifest { manifest: Blob::new(bytes), signature }))
}
