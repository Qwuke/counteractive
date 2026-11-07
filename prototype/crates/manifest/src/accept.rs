//! Who accepts a manifest, and on what terms.
//!
//! The wallet's rule is docs 04 section 6 with compiled-in values replaced
//! by pins from `wallet.toml` (SPEC section 7). The gate's rule is SPEC
//! section 5, Manifest. Both compare against values they hold themselves,
//! never against other fields of the same signed file: a hostile signer
//! could set those to anything.

use proto::SignedManifest;

use crate::ManifestError;
use crate::signature::{OperatorPublicKey, verify_signed};
use crate::types::{BALANCE_WIDTH, CREDIT_SCHEME, Manifest, Platform};

/// The wallet's pins that a manifest must match.
#[derive(Debug, Clone)]
pub struct AcceptPins {
    /// The operator's signing key.
    pub operator_public_key: OperatorPublicKey,
    /// The service id the wallet's contexts derive from.
    pub service_id: String,
    /// The issuer key epoch the wallet's contexts derive from.
    pub key_epoch: u32,
    /// The gate's onion address. Tier 1 runs without one, so the check is
    /// skipped when this is unset.
    pub onion: Option<String>,
    /// Whether `dev-insecure` releases count as usable.
    pub allow_dev_attestation: bool,
}

/// The wallet's accept rule.
///
/// Passing this doesn't make a manifest trusted: a fetched manifest still
/// waits for the user's approval (docs 04 section 6, "Fetched isn't accepted").
/// `highest_accepted` is the version of the manifest accepted before, if any.
pub fn accept(
    signed: &SignedManifest,
    pins: &AcceptPins,
    highest_accepted: Option<u64>,
) -> Result<Manifest, ManifestError> {
    let manifest = verify_signed(signed, &pins.operator_public_key)?;
    check_version(&manifest, highest_accepted.unwrap_or(0))?;

    let service = &manifest.service;
    if service.id != pins.service_id {
        return Err(ManifestError::ServiceMismatch);
    }
    if let Some(onion) = &pins.onion
        && service.onion.as_deref() != Some(onion)
    {
        return Err(ManifestError::OnionMismatch);
    }
    if service.key_epoch != pins.key_epoch {
        return Err(ManifestError::EpochMismatch);
    }
    if service.credit_scheme != CREDIT_SCHEME {
        return Err(ManifestError::UnknownCreditScheme);
    }
    if service.balance_width != BALANCE_WIDTH {
        return Err(ManifestError::BalanceWidth);
    }
    if service.issuer_public_key.is_empty() {
        return Err(ManifestError::MissingIssuerKey);
    }

    // Unknown platforms are skipped, so adding one doesn't break older
    // wallets. A known release must be internally consistent, though.
    let mut usable = 0;
    for release in &manifest.releases {
        let Some(platform) = release.platform() else { continue };
        release.config()?;
        if release.attestation_policy.kind != platform.policy_kind() {
            return Err(ManifestError::PolicyMismatch);
        }
        if platform == Platform::DevInsecure && !pins.allow_dev_attestation {
            continue;
        }
        usable += 1;
    }
    if usable == 0 {
        return Err(ManifestError::NoUsableRelease);
    }
    Ok(manifest)
}

/// What the gate holds that a posted manifest must match.
#[derive(Debug, Clone)]
pub struct GatePins<'a> {
    /// From `release.json`.
    pub operator_public_key: &'a OperatorPublicKey,
    /// From `release.json`.
    pub service_id: &'a str,
    /// The store's issuer public key.
    pub issuer_public_key: &'a [u8],
    /// The store's key epoch.
    pub key_epoch: u32,
    /// The store's `manifest_version`; 0 before the first.
    pub stored_version: u64,
    /// SHA-256 of the gate's own canonical `release.json`.
    pub release_config_sha256: &'a [u8; 32],
}

/// The gate's rule for `POST /v1/operator/manifest`. Any failure is `409 manifest_mismatch`.
pub fn gate_accept(signed: &SignedManifest, pins: &GatePins<'_>) -> Result<Manifest, ManifestError> {
    let manifest = verify_signed(signed, pins.operator_public_key)?;
    check_version(&manifest, pins.stored_version)?;
    if manifest.service.id != pins.service_id {
        return Err(ManifestError::ServiceMismatch);
    }
    if manifest.service.issuer_public_key.as_bytes() != pins.issuer_public_key {
        return Err(ManifestError::IssuerKeyMismatch);
    }
    if manifest.service.key_epoch != pins.key_epoch {
        return Err(ManifestError::EpochMismatch);
    }
    let own = manifest.release_by_config_sha256(pins.release_config_sha256).ok_or(ManifestError::NoMatchingRelease)?;
    if !own.config_hash_matches() {
        return Err(ManifestError::ConfigHashMismatch);
    }
    Ok(manifest)
}

fn check_version(manifest: &Manifest, floor: u64) -> Result<(), ManifestError> {
    if manifest.version < floor {
        return Err(ManifestError::VersionRollback { accepted: floor, offered: manifest.version });
    }
    Ok(())
}
