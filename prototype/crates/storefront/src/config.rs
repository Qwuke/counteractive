//! What the storefront reads at startup: `release.json` (products and the
//! operator token's hash), the dev attestation key, and where Stripe, the
//! gate and the signed manifest are.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use attest::{AttestationSource, DevAttestationKey, DevInsecureSource};
use manifest::ReleaseConfig;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use zeroize::Zeroizing;

use crate::stripe::StripeEndpoint;

/// Everything a [`crate::Storefront`] needs. Holds no Stripe key: that
/// arrives later, from the operator, and lives only in memory.
pub struct Settings {
    /// The measured release config: products, prices and the operator token's hash.
    pub release: ReleaseConfig,
    /// Produces evidence binding this boot's TLS key, with role `storefront`.
    pub attestation: Box<dyn AttestationSource>,
    /// The gate's `issue.sock`.
    pub issue_socket: PathBuf,
    /// Where Stripe's API is, and where Checkout returns the buyer.
    pub stripe: StripeEndpoint,
    /// The directory holding `manifest.json` and `manifest.sig`.
    pub manifest_dir: PathBuf,
}

/// Reads and validates `release.json`, returning it with the SHA-256 of
/// its canonical form, which reports bind.
pub fn read_release(path: &Path) -> anyhow::Result<(ReleaseConfig, [u8; 32])> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    ReleaseConfig::parse(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// The dev-insecure attestation source for this release, with its key read
/// from `path` (see [`DevAttestationKey::from_file_contents`] for its format).
pub fn dev_attestation_source(path: &Path, release_config_sha256: [u8; 32]) -> anyhow::Result<DevInsecureSource> {
    let contents = Zeroizing::new(std::fs::read(path).with_context(|| format!("reading {}", path.display()))?);
    let key =
        DevAttestationKey::from_file_contents(&contents).with_context(|| format!("parsing {}", path.display()))?;
    Ok(DevInsecureSource::new(key, release_config_sha256))
}

/// Whether `token` is the storefront operator token: its SHA-256 equals
/// `release.json`'s `operator_token_sha256`, compared in constant time.
pub fn is_operator_token(release: &ReleaseConfig, token: &[u8]) -> bool {
    let digest: [u8; 32] = Sha256::digest(token).into();
    digest.ct_eq(&release.storefront.operator_token_sha256).into()
}
