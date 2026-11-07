//! The signed manifest (docs 07 section 7), as the prototype uses it.
//!
//! Fields this build doesn't model, such as SNP measurements, are skipped
//! when parsing. That's safe because the signature covers the canonical
//! bytes, which are checked before parsing.

use proto::Blob;
use proto::encoding::hex_array;
use proto::walletd::ManifestSummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ManifestError;
use crate::canonical::release_config_sha256;
use crate::release::ReleaseConfig;

/// The manifest schema this build reads.
pub const SCHEMA: u32 = 1;

/// The credit scheme this build speaks.
pub const CREDIT_SCHEME: &str = "ec-act-p256-v0";

/// The balance width this build's credentials use; equals `credit::BALANCE_WIDTH`.
pub const BALANCE_WIDTH: u8 = 24;

/// The whole manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Must be [`SCHEMA`].
    pub schema: u32,
    /// Only goes up.
    pub version: u64,
    /// The service's identity and issuer.
    pub service: Service,
    /// The releases a wallet may talk to.
    pub releases: Vec<Release>,
}

/// Who the service is and which issuer key it uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// Service id; wallets compare it with their pin.
    pub id: String,
    /// Onion address, when the service has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onion: Option<String>,
    /// Must be [`CREDIT_SCHEME`].
    pub credit_scheme: String,
    /// Compressed issuer public key.
    pub issuer_public_key: Blob,
    /// Names one issuer key; wallets compare it with their pin.
    pub key_epoch: u32,
    /// Credential balance width.
    pub balance_width: u8,
}

/// One release: an image on a platform, with its attested config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    /// The image version.
    pub image_version: String,
    /// `dev-insecure`, `snp-verda`, or a platform this build doesn't know.
    pub platform: String,
    /// How to check this release's evidence.
    pub attestation_policy: AttestationPolicy,
    /// SHA-256 of `release_config`'s canonical bytes; reports bind this.
    #[serde(with = "hex_array")]
    pub release_config_sha256: [u8; 32],
    /// The release's `release.json`: products, prices, catalogue, policy.
    pub release_config: Value,
}

/// The platform-specific part of a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationPolicy {
    /// `dev-insecure` or `snp`.
    pub kind: String,
    /// For `dev-insecure`: the Ed25519 key that signs reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_public_key: Option<Blob>,
}

/// The platforms this build recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Ed25519 stand-in for a hardware report. Laptop only.
    DevInsecure,
    /// SEV-SNP on Verda. Recognised, but not verifiable in the prototype.
    SnpVerda,
}

impl Platform {
    /// The platform's name in a release.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DevInsecure => "dev-insecure",
            Self::SnpVerda => "snp-verda",
        }
    }

    /// The `attestation_policy.kind` this platform requires.
    pub fn policy_kind(self) -> &'static str {
        match self {
            Self::DevInsecure => "dev-insecure",
            Self::SnpVerda => "snp",
        }
    }

    /// Looks a name up; `None` for a platform this build doesn't know.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "dev-insecure" => Some(Self::DevInsecure),
            "snp-verda" => Some(Self::SnpVerda),
            _ => None,
        }
    }
}

impl Manifest {
    /// The release a report selects by its bound config hash.
    pub fn release_by_config_sha256(&self, sha256: &[u8; 32]) -> Option<&Release> {
        self.releases.iter().find(|release| &release.release_config_sha256 == sha256)
    }

    /// What a user sees when asked to approve this manifest.
    pub fn summary(&self) -> ManifestSummary {
        ManifestSummary {
            version: self.version,
            image_versions: self.releases.iter().map(|r| r.image_version.clone()).collect(),
            platforms: self.releases.iter().map(|r| r.platform.clone()).collect(),
        }
    }
}

impl Release {
    /// The platform, if this build knows it.
    pub fn platform(&self) -> Option<Platform> {
        Platform::from_name(&self.platform)
    }

    /// Whether `release_config` hashes to `release_config_sha256`.
    pub fn config_hash_matches(&self) -> bool {
        release_config_sha256(&self.release_config) == self.release_config_sha256
    }

    /// The parsed and validated config, after checking its hash.
    pub fn config(&self) -> Result<ReleaseConfig, ManifestError> {
        if !self.config_hash_matches() {
            return Err(ManifestError::ConfigHashMismatch);
        }
        ReleaseConfig::from_value(&self.release_config)
    }
}
