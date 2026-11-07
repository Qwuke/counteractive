//! The `dev-insecure` platform: an Ed25519 signature over `REPORT_DATA`.
//!
//! It exercises every check above the hardware (binding, role, release
//! selection, TLS key) with no hardware at all. Anyone holding the dev key
//! can forge reports, so a wallet accepts it only when its pins allow, and
//! warns on every connection.

use std::fmt;

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use manifest::{Manifest, Platform};
use proto::Blob;
use rand::TryRng as _;
use zeroize::Zeroizing;

use crate::verifier::{AttestError, AttestationSource, AttestationVerifier, Rejection, Verdict};
use crate::{Evidence, ReportBinding};

/// The key that signs dev reports. Stands in for the CPU's attestation key.
pub struct DevAttestationKey(SigningKey);

impl DevAttestationKey {
    /// A fresh key from the operating system's random number generator.
    pub fn generate() -> Result<Self, rand::rngs::SysError> {
        let mut seed = Zeroizing::new([0u8; 32]);
        rand::rngs::SysRng.try_fill_bytes(seed.as_mut())?;
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    /// A key from its 32-byte seed.
    pub fn from_bytes(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }

    /// A key from the contents of a dev key file: the 32-byte seed, raw or
    /// as 64 hex digits with surrounding whitespace, so both binary and
    /// text tools can write it.
    pub fn from_file_contents(contents: &[u8]) -> Result<Self, DevKeyError> {
        let mut seed = Zeroizing::new([0u8; 32]);
        if let Ok(raw) = <&[u8; 32]>::try_from(contents) {
            seed.copy_from_slice(raw);
        } else {
            let text = contents.trim_ascii();
            if text.len() != 64 || hex::decode_to_slice(text, seed.as_mut()).is_err() {
                return Err(DevKeyError);
            }
        }
        Ok(Self::from_bytes(&seed))
    }

    /// The 32-byte seed, for writing to the dev key file.
    pub fn to_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The public key a manifest release lists as `dev_public_key`.
    pub fn public_key(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }
}

/// A dev key file that holds neither 32 raw bytes nor 64 hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected a 32-byte key, raw or as 64 hex digits")]
pub struct DevKeyError;

impl fmt::Debug for DevAttestationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevAttestationKey([redacted])")
    }
}

/// Signs reports for one release, as a gate or storefront in dev does.
#[derive(Debug)]
pub struct DevInsecureSource {
    key: DevAttestationKey,
    release_config_sha256: [u8; 32],
}

impl DevInsecureSource {
    /// A source for the release with this config hash.
    pub fn new(key: DevAttestationKey, release_config_sha256: [u8; 32]) -> Self {
        Self { key, release_config_sha256 }
    }
}

impl AttestationSource for DevInsecureSource {
    fn platform(&self) -> Platform {
        Platform::DevInsecure
    }

    fn evidence(&self, binding: &ReportBinding) -> Result<Evidence, AttestError> {
        let report_data = binding.report_data(&self.release_config_sha256);
        let report = self.key.0.sign(&report_data);
        Ok(Evidence {
            platform: Platform::DevInsecure.as_str().into(),
            role: binding.role,
            release_config_sha256: self.release_config_sha256,
            gpu_evidence_sha256: binding.gpu_evidence_sha256,
            report: Blob::new(report.to_bytes().to_vec()),
        })
    }
}

/// Verifies `dev-insecure` evidence (SPEC section 3).
#[derive(Debug, Clone, Copy, Default)]
pub struct DevInsecureVerifier;

impl AttestationVerifier for DevInsecureVerifier {
    fn verify(&self, evidence: &Evidence, binding: &ReportBinding, manifest: &Manifest) -> Result<Verdict, Rejection> {
        if evidence.platform != Platform::DevInsecure.as_str() {
            return Err(Rejection::WrongPlatform(evidence.platform.clone()));
        }
        if evidence.role != binding.role {
            return Err(Rejection::WrongRole { expected: binding.role, claimed: evidence.role });
        }
        if evidence.gpu_evidence_sha256 != binding.gpu_evidence_sha256 {
            return Err(Rejection::GpuEvidenceMismatch);
        }

        let release =
            manifest.release_by_config_sha256(&evidence.release_config_sha256).ok_or(Rejection::NoMatchingRelease)?;
        let policy = &release.attestation_policy;
        if release.platform() != Some(Platform::DevInsecure) || policy.kind != Platform::DevInsecure.policy_kind() {
            return Err(Rejection::PolicyMismatch);
        }
        let key_bytes: [u8; 32] = policy
            .dev_public_key
            .as_ref()
            .and_then(|key| key.as_bytes().try_into().ok())
            .ok_or(Rejection::PolicyMismatch)?;
        let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| Rejection::PolicyMismatch)?;

        let report: [u8; 64] = evidence.report.as_bytes().try_into().map_err(|_| Rejection::MalformedEvidence)?;
        // Recomputed from the verifier's own binding: its role, nonce and recorded SPKI.
        let report_data = binding.report_data(&release.release_config_sha256);
        key.verify_strict(&report_data, &Signature::from_bytes(&report)).map_err(|_| Rejection::ReportMismatch)?;

        let release_config = release.config().map_err(|_| Rejection::ReleaseConfig)?;
        Ok(Verdict {
            role: binding.role,
            platform: Platform::DevInsecure,
            tls_spki_sha256: binding.tls_spki_sha256,
            manifest_version: manifest.version,
            image_version: release.image_version.clone(),
            release_config_sha256: release.release_config_sha256,
            release_config,
        })
    }
}
