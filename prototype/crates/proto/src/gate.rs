//! The gate's API: `gate.sock` (inner TLS) and `issue.sock` (local), SPEC section 5.
//!
//! Spend bytes are [`SecretBlob`]s, because their hash keys the sealed
//! refund. Refunds and issuance messages are [`Blob`]s: public on their own,
//! but kept out of `Debug`.

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::encoding::{Blob, SecretBlob, hex_array, hex_decode_array};
use crate::openai::ChatMessage;

/// Where the gate is in its boot sequence (docs 05 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    /// Waiting for K.
    Locked,
    /// Store open, waiting for a signed manifest naming this release.
    AwaitingManifest,
    /// Serving sessions and issuance.
    Ready,
}

/// `GET /v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateHealth {
    /// Boot state.
    pub state: GateState,
    /// The gate's image version from `release.json`.
    pub version: String,
}

/// `POST /v1/attestation`, on the gate and the storefront alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationRequest {
    /// The verifier's fresh nonce, bound into the report.
    #[serde(with = "crate::encoding::b64_array")]
    pub nonce: [u8; 32],
}

/// `GET /v1/manifest` on either service, and the body of `POST /v1/operator/manifest`.
///
/// `manifest` is the exact canonical JSON the operator signed, so a
/// verifier checks the signature over these bytes before parsing them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedManifest {
    /// Canonical manifest bytes.
    pub manifest: Blob,
    /// The full minisign signature text.
    pub signature: String,
}

/// A session's 128-bit random identifier.
///
/// Possession of it is the only authority over a session (docs 05
/// section 3), so `Debug` hides it like a bearer token.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 16]);

impl SessionId {
    /// Wraps 16 random bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Lowercase hex, as used in URLs.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId([redacted])")
    }
}

impl FromStr for SessionId {
    type Err = hex::FromHexError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        hex_decode_array(text).map(Self)
    }
}

impl Serialize for SessionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}

/// Session limits from the release (docs 07 section 6), shown to the wallet on open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicy {
    /// Largest `s` the gate opens a session for; larger spends settle with a full refund.
    pub maximum_spend: u64,
    /// The gate closes a session after this long without a turn.
    pub idle_seconds: u64,
    /// The gate closes a session after this many turns.
    pub max_turns: u32,
    /// The gate closes a session this long after it opened.
    pub max_age_seconds: u64,
}

/// `POST /v1/sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSessionRequest {
    /// The spend message, sent byte-identical on every retry.
    pub spend: SecretBlob,
}

/// The answer to `POST /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OpenSessionResponse {
    /// A live session with budget `s`.
    Open {
        /// The session's id.
        session_id: SessionId,
        /// `s`, the amount spent into the session.
        budget: u64,
        /// The limits that apply.
        policy: SessionPolicy,
    },
    /// The spend was out of policy and settled at once with `t = s`.
    Settled {
        /// The refund message.
        refund: Blob,
    },
}

/// `POST /v1/sessions/{id}/chat`. Allowlisted: unknown fields vanish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChatRequest {
    /// Model id from the release catalogue.
    pub model: String,
    /// The whole conversation; the gate keeps no history.
    pub messages: Vec<ChatMessage>,
    /// Output limit; the gate's hold may allow less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
}

/// The answer to `POST /v1/sessions/{id}/close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseResponse {
    /// The refund message for `t = s − debits`.
    pub refund: Blob,
}

/// `POST /v1/sessions/recover`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverRequest {
    /// The same spend bytes the session was opened with.
    pub spend: SecretBlob,
}

/// The answer to `POST /v1/sessions/recover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RecoverResponse {
    /// The session is settled; these are the refund bytes, identical on every call.
    Settled {
        /// The refund message.
        refund: Blob,
    },
    /// The session is still open.
    Live {
        /// The session's id.
        session_id: SessionId,
    },
}

/// The answer to `POST /v1/operator/unlock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlockResponse {
    /// State after unlocking.
    pub state: GateState,
    /// The issuer's public key, compressed point bytes.
    pub issuer_public_key: Blob,
}

/// The answer to `POST /v1/operator/manifest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateResponse {
    /// State after the call.
    pub state: GateState,
}

/// The answer to `GET /v1/operator/state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorStateResponse {
    /// Boot state.
    pub state: GateState,
    /// Session slots currently held.
    pub live_sessions: u64,
}

/// `issue.sock POST /local/issue`, from the storefront.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIssueRequest {
    /// `SHA-256(order_id)`, the idempotency key.
    #[serde(with = "hex_array")]
    pub order_ref: [u8; 32],
    /// The paid product's credits; must equal a listed product's.
    pub credits: u64,
    /// The wallet's issue request, unchanged.
    pub issue_request: Blob,
}

/// The answer to `POST /local/issue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIssueResponse {
    /// The issue response, identical for a repeated request.
    pub issue_response: Blob,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn open_response_is_tagged_by_status() {
        let open = OpenSessionResponse::Open {
            session_id: SessionId::from_bytes([0xab; 16]),
            budget: 500_000,
            policy: SessionPolicy { maximum_spend: 500_000, idle_seconds: 900, max_turns: 64, max_age_seconds: 3600 },
        };
        let value = serde_json::to_value(&open).unwrap();
        assert_eq!(
            value,
            json!({
                "status": "open", "session_id": "ab".repeat(16), "budget": 500_000,
                "policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64,
                           "max_age_seconds": 3600}
            })
        );
        assert_eq!(serde_json::from_value::<OpenSessionResponse>(value).unwrap(), open);

        let settled: OpenSessionResponse =
            serde_json::from_value(json!({"status": "settled", "refund": "AQID"})).unwrap();
        assert_eq!(settled, OpenSessionResponse::Settled { refund: Blob::new(vec![1, 2, 3]) });
    }

    #[test]
    fn recover_response_shapes() {
        let live: RecoverResponse =
            serde_json::from_value(json!({"status": "live", "session_id": "00".repeat(16)})).unwrap();
        assert_eq!(live, RecoverResponse::Live { session_id: SessionId::from_bytes([0; 16]) });
        serde_json::from_value::<RecoverResponse>(json!({"status": "gone"})).unwrap_err();
    }

    #[test]
    fn session_ids_parse_from_hex_and_hide_in_debug() {
        let id: SessionId = "0f".repeat(16).parse().unwrap();
        assert_eq!(id.to_hex(), "0f".repeat(16));
        "0f".repeat(15).parse::<SessionId>().unwrap_err();
        assert_eq!(format!("{id:?}"), "SessionId([redacted])");
    }

    #[test]
    fn spend_requests_never_debug_their_bytes() {
        let request = OpenSessionRequest { spend: SecretBlob::new(vec![9; 40]) };
        assert!(format!("{request:?}").contains("redacted"));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["spend"].as_str().unwrap().len(), 54);
    }

    #[test]
    fn local_issue_request_shape() {
        let request =
            LocalIssueRequest { order_ref: [0x11; 32], credits: 5_000_000, issue_request: Blob::new(vec![1]) };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"order_ref": "11".repeat(32), "credits": 5_000_000, "issue_request": "AQ"})
        );
    }

    #[test]
    fn states_are_snake_case() {
        let health = GateHealth { state: GateState::AwaitingManifest, version: "0.1.0".into() };
        assert_eq!(serde_json::to_value(&health).unwrap(), json!({"state": "awaiting_manifest", "version": "0.1.0"}));
    }
}
