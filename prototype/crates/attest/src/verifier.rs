//! The platform axis: producing evidence, and checking it against a manifest.

use manifest::{Manifest, Platform, ReleaseConfig};

use crate::dev::DevInsecureVerifier;
use crate::snp::SnpVerifier;
use crate::{Evidence, ReportBinding, Role};

/// Produces evidence for a binding. Implemented by the gate and the storefront.
///
/// Synchronous: the dev source only signs, and a hardware source's device
/// call can run under `spawn_blocking`.
pub trait AttestationSource: Send + Sync {
    /// The platform this source reports for.
    fn platform(&self) -> Platform;

    /// Evidence binding `binding` to this source's release.
    fn evidence(&self, binding: &ReportBinding) -> Result<Evidence, AttestError>;
}

/// Checks evidence against the binding the verifier expects and a manifest.
pub trait AttestationVerifier: Send + Sync {
    /// Accepts the evidence and names the release it selected, or rejects it.
    fn verify(&self, evidence: &Evidence, binding: &ReportBinding, manifest: &Manifest) -> Result<Verdict, Rejection>;
}

/// What a successful verification established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The role the report was bound to.
    pub role: Role,
    /// The platform that vouched for it.
    pub platform: Platform,
    /// The TLS key the report was bound to.
    pub tls_spki_sha256: [u8; 32],
    /// The manifest version the release was selected from.
    pub manifest_version: u64,
    /// The selected release's image version.
    pub image_version: String,
    /// The selected release's config hash.
    pub release_config_sha256: [u8; 32],
    /// The selected release's config: products, prices, catalogue and policy.
    pub release_config: ReleaseConfig,
}

impl Verdict {
    /// Whether the platform gives no hardware guarantee; callers warn on every connection.
    pub fn is_insecure(&self) -> bool {
        self.platform == Platform::DevInsecure
    }
}

/// Why evidence was refused. Any rejection means: close, send nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// The evidence names a platform this verifier doesn't handle.
    #[error("evidence platform `{0}` is not handled by this verifier")]
    WrongPlatform(String),
    /// `dev-insecure` evidence when the pins don't allow it.
    #[error("dev-insecure attestation is not allowed by this wallet's pins")]
    DevAttestationNotAllowed,
    /// The server claims another role than the one this connection expects.
    #[error("expected role {expected:?}, evidence claims {claimed:?}")]
    WrongRole {
        /// The role this connection expects.
        expected: Role,
        /// The role in the evidence.
        claimed: Role,
    },
    /// The GPU evidence hash differs from the expected one.
    #[error("GPU evidence hash does not match")]
    GpuEvidenceMismatch,
    /// No release in the manifest has the evidence's config hash.
    #[error("no manifest release matches the evidence")]
    NoMatchingRelease,
    /// The selected release's platform or policy doesn't fit the evidence.
    #[error("release attestation policy does not fit the evidence")]
    PolicyMismatch,
    /// The selected release's config is inconsistent or invalid.
    #[error("selected release config is invalid")]
    ReleaseConfig,
    /// The evidence is structurally wrong, such as a report of the wrong length.
    #[error("malformed evidence")]
    MalformedEvidence,
    /// The report doesn't verify over the expected report data.
    #[error("report does not match the expected binding")]
    ReportMismatch,
    /// The platform is recognised but can't be verified by this build.
    #[error("platform verification is not supported in this build")]
    Unsupported,
}

/// Why a source couldn't produce evidence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttestError {
    /// The platform can't produce reports in this build.
    #[error("attestation is not supported on this platform")]
    Unsupported,
}

/// Dispatches on the evidence's platform, as a wallet does.
#[derive(Debug, Clone, Copy)]
pub struct PlatformVerifier {
    allow_dev_attestation: bool,
}

impl PlatformVerifier {
    /// A verifier for every known platform; `dev-insecure` only if allowed by the pins.
    pub fn new(allow_dev_attestation: bool) -> Self {
        Self { allow_dev_attestation }
    }
}

impl AttestationVerifier for PlatformVerifier {
    fn verify(&self, evidence: &Evidence, binding: &ReportBinding, manifest: &Manifest) -> Result<Verdict, Rejection> {
        match Platform::from_name(&evidence.platform) {
            Some(Platform::DevInsecure) if self.allow_dev_attestation => {
                DevInsecureVerifier.verify(evidence, binding, manifest)
            }
            Some(Platform::DevInsecure) => Err(Rejection::DevAttestationNotAllowed),
            Some(Platform::SnpVerda) => SnpVerifier.verify(evidence, binding, manifest),
            None => Err(Rejection::WrongPlatform(evidence.platform.clone())),
        }
    }
}
