//! What the gate reads at startup: `release.json` and the dev attestation
//! key (docs 07 section 6).

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use attest::DevAttestationKey;
use manifest::{OperatorPublicKey, ReleaseConfig};
use zeroize::Zeroizing;

/// Where the gate's inputs and sockets are: its command-line flags.
#[derive(Debug, Clone)]
pub struct GateOptions {
    /// `release.json`, the measured per-image config.
    pub release: PathBuf,
    /// The data directory, holding the money store and the latest signed manifest.
    pub data: PathBuf,
    /// The run directory, where `gate.sock` and `issue.sock` are created.
    pub run: PathBuf,
    /// The inference engine's Unix socket.
    pub engine: PathBuf,
    /// The dev-insecure attestation key: 32 bytes, raw or as 64 hex digits.
    pub dev_attest_key: PathBuf,
}

/// `release.json`, parsed and checked, with what derives from it.
#[derive(Debug)]
pub struct Release {
    /// The config: products, catalogue, session policy, pins.
    pub config: ReleaseConfig,
    /// SHA-256 of its canonical form. Reports bind it, and an accepted
    /// manifest must name a release with it.
    pub sha256: [u8; 32],
    /// The key manifests must be signed with.
    pub operator_key: OperatorPublicKey,
}

impl Release {
    /// Reads and validates `release.json`.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let (config, sha256) = ReleaseConfig::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
        let operator_key =
            OperatorPublicKey::from_base64(&config.operator_public_key).context("release.json operator_public_key")?;
        Ok(Self { config, sha256, operator_key })
    }
}

/// Reads the dev-insecure attestation key file (see
/// [`DevAttestationKey::from_file_contents`] for its format).
pub fn read_dev_attestation_key(path: &Path) -> anyhow::Result<DevAttestationKey> {
    let contents = Zeroizing::new(std::fs::read(path).with_context(|| format!("reading {}", path.display()))?);
    DevAttestationKey::from_file_contents(&contents).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_key_reads_raw_or_hex() {
        let dir = tempfile::tempdir().unwrap();
        let seed = [0x5a; 32];
        let expected = DevAttestationKey::from_bytes(&seed).public_key();

        let raw = dir.path().join("raw");
        std::fs::write(&raw, seed).unwrap();
        assert_eq!(read_dev_attestation_key(&raw).unwrap().public_key(), expected);

        let text = dir.path().join("hex");
        std::fs::write(&text, format!("{}\n", hex::encode(seed))).unwrap();
        assert_eq!(read_dev_attestation_key(&text).unwrap().public_key(), expected);

        let short = dir.path().join("short");
        std::fs::write(&short, [1u8; 31]).unwrap();
        read_dev_attestation_key(&short).unwrap_err();
    }
}
