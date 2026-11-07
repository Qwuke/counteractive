//! The signed manifest and `release.json`.
//!
//! Domain crate, no IO (ARCHITECTURE 8.2 rule 1). It defines the types,
//! canonical JSON, the minisign signature check and the accept rules for
//! the wallet ([`accept`]) and the gate ([`gate_accept`]). Signing is
//! behind the `sign` feature, which only `xtask` and tests enable.

mod accept;
mod canonical;
mod release;
#[cfg(feature = "sign")]
mod sign;
mod signature;
mod types;

pub use accept::{AcceptPins, GatePins, accept, gate_accept};
pub use canonical::{canonical_json, is_canonical, release_config_sha256, to_canonical_json};
pub use proto::SignedManifest;
pub use release::{CatalogueEntry, Product, RELEASE_SCHEMA, ReleaseConfig, StorefrontSettings};
#[cfg(feature = "sign")]
pub use sign::{OperatorSecretKey, sign_manifest};
pub use signature::{OperatorPublicKey, verify_signed};
pub use types::{AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, Platform, Release, SCHEMA, Service};

/// Why a manifest or release config was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// The minisign signature is missing, malformed, legacy, or by another key.
    #[error("manifest signature does not verify")]
    Signature,
    /// The signed bytes aren't canonical JSON.
    #[error("manifest bytes are not canonical JSON")]
    NotCanonical,
    /// The schema number isn't one this build reads.
    #[error("unknown schema {0}")]
    UnknownSchema(u32),
    /// The JSON doesn't have the expected shape.
    #[error("malformed manifest or release config")]
    Malformed,
    /// A minisign key couldn't be parsed or used.
    #[error("invalid operator key")]
    OperatorKey,
    /// The version is below the highest already accepted.
    #[error("manifest version {offered} is below accepted version {accepted}")]
    VersionRollback {
        /// Highest accepted version.
        accepted: u64,
        /// The version offered.
        offered: u64,
    },
    /// `service.id` differs from the pinned value.
    #[error("service id does not match")]
    ServiceMismatch,
    /// `service.onion` differs from the pinned value.
    #[error("onion address does not match")]
    OnionMismatch,
    /// `service.key_epoch` differs from the pinned value.
    #[error("key epoch does not match")]
    EpochMismatch,
    /// `service.credit_scheme` isn't one this build speaks.
    #[error("unknown credit scheme")]
    UnknownCreditScheme,
    /// `service.balance_width` isn't 24.
    #[error("unsupported balance width")]
    BalanceWidth,
    /// `service.issuer_public_key` is empty.
    #[error("issuer public key missing")]
    MissingIssuerKey,
    /// `service.issuer_public_key` differs from the gate's store.
    #[error("issuer public key does not match")]
    IssuerKeyMismatch,
    /// A release's config doesn't hash to its `release_config_sha256`.
    #[error("release config hash does not match")]
    ConfigHashMismatch,
    /// A known platform's release names another platform's policy kind.
    #[error("attestation policy does not fit the platform")]
    PolicyMismatch,
    /// No release has a platform this wallet can use.
    #[error("no usable release")]
    NoUsableRelease,
    /// No release matches this gate's own config.
    #[error("no release matches this config")]
    NoMatchingRelease,
    /// A release config breaks a rule services rely on.
    #[error("invalid release config: {0}")]
    InvalidReleaseConfig(&'static str),
}

#[cfg(test)]
mod tests;
