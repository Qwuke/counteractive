//! The evidence a service returns from `POST /v1/attestation`.

use proto::Blob;
use proto::encoding::hex_array;
use serde::{Deserialize, Serialize};

use crate::Role;

/// `{"platform", "role", "release_config_sha256", "gpu_evidence_sha256", "report"}` (SPEC section 3).
///
/// Every field here comes from the server and is checked, never trusted:
/// the verifier recomputes the report data from its own binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The platform name, such as `dev-insecure`.
    pub platform: String,
    /// The role the server claims.
    pub role: Role,
    /// Selects the manifest release this server claims to run.
    #[serde(with = "hex_array")]
    pub release_config_sha256: [u8; 32],
    /// The hash of the GPU evidence bound into the report.
    #[serde(with = "hex_array")]
    pub gpu_evidence_sha256: [u8; 32],
    /// The platform's report; for `dev-insecure`, a 64-byte Ed25519 signature.
    pub report: Blob,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evidence_has_the_spec_shape() {
        let evidence = Evidence {
            platform: "dev-insecure".into(),
            role: Role::Gate,
            release_config_sha256: [0xaa; 32],
            gpu_evidence_sha256: [0; 32],
            report: Blob::new(vec![1, 2, 3]),
        };
        let value = serde_json::to_value(&evidence).unwrap();
        assert_eq!(
            value,
            json!({
                "platform": "dev-insecure", "role": "gate",
                "release_config_sha256": "aa".repeat(32),
                "gpu_evidence_sha256": "00".repeat(32), "report": "AQID"
            })
        );
        assert_eq!(serde_json::from_value::<Evidence>(value).unwrap(), evidence);
    }
}
