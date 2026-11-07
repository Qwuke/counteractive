//! What a report binds: role, TLS key, GPU evidence, nonce and release (SPEC section 3).

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha512};

/// The prototype's GPU evidence hash: there is no GPU evidence.
pub const NO_GPU_EVIDENCE: [u8; 32] = [0; 32];

const REPORT_LABEL: &[u8] = b"counteractive-report-v1";

/// Which service produced a report.
///
/// The gate and storefront share one image and so one measurement; the
/// role in the binding is what stops one standing in for the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The gate, reached over Tor.
    Gate,
    /// The storefront, reached over clearnet.
    Storefront,
}

impl Role {
    /// The name hashed into the report and written in evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gate => "gate",
            Self::Storefront => "storefront",
        }
    }
}

/// The values a verifier expects a report to bind, all from its own side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportBinding {
    /// The role this connection expects.
    pub role: Role,
    /// SHA-256 of the TLS leaf's `SubjectPublicKeyInfo`, as recorded in the handshake.
    pub tls_spki_sha256: [u8; 32],
    /// SHA-256 of the GPU evidence supplied; [`NO_GPU_EVIDENCE`] in the prototype.
    pub gpu_evidence_sha256: [u8; 32],
    /// The verifier's fresh nonce.
    pub wallet_nonce: [u8; 32],
}

impl ReportBinding {
    /// A binding with no GPU evidence, as every prototype binding is.
    pub fn new(role: Role, tls_spki_sha256: [u8; 32], wallet_nonce: [u8; 32]) -> Self {
        Self { role, tls_spki_sha256, gpu_evidence_sha256: NO_GPU_EVIDENCE, wallet_nonce }
    }

    /// `REPORT_DATA` for a report from the release with this config hash:
    ///
    /// `SHA-512(lp(label) || lp(role) || spki_sha256 || gpu_evidence_sha256 || nonce || release_config_sha256)`
    /// where `lp(x) = u16be(len(x)) || x`.
    pub fn report_data(&self, release_config_sha256: &[u8; 32]) -> [u8; 64] {
        let mut hash = Sha512::new();
        for field in [REPORT_LABEL, self.role.as_str().as_bytes()] {
            let len = u16::try_from(field.len()).expect("labels are short constants");
            hash.update(len.to_be_bytes());
            hash.update(field);
        }
        hash.update(self.tls_spki_sha256);
        hash.update(self.gpu_evidence_sha256);
        hash.update(self.wallet_nonce);
        hash.update(release_config_sha256);
        hash.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_data_known_answer() {
        // Computed independently with Python's hashlib.
        let binding = ReportBinding::new(Role::Gate, [1; 32], [2; 32]);
        assert_eq!(
            hex::encode(binding.report_data(&[3; 32])),
            "50478edada8391a823f0871ba245a6cfe11efbb483a5119439897c3e234fcd32\
             f4a3241f9f61850994b277d8c01032d6dbf303d327923cbf2466554d8009db81"
        );
    }

    #[test]
    fn every_field_changes_the_result() {
        let base = ReportBinding::new(Role::Gate, [1; 32], [2; 32]);
        let release = [3; 32];
        let reference = base.report_data(&release);
        let variants = [
            ReportBinding { role: Role::Storefront, ..base },
            ReportBinding { tls_spki_sha256: [9; 32], ..base },
            ReportBinding { gpu_evidence_sha256: [9; 32], ..base },
            ReportBinding { wallet_nonce: [9; 32], ..base },
        ];
        for variant in variants {
            assert_ne!(variant.report_data(&release), reference);
        }
        assert_ne!(base.report_data(&[4; 32]), reference);
    }
}
