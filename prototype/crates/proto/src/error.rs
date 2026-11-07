//! The JSON error body every service returns: `{"error": "<code>", ...}`.
//!
//! Codes are a closed set, so no free text from inside a service (a path, a
//! prompt, a Stripe message) can reach a client through an error. Each code
//! has one HTTP status, which keeps a wallet's reaction table (docs 04
//! section 8) keyed on one thing.

use serde::{Deserialize, Serialize};

/// Every error code a gate, storefront, walletd or fake service may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 400: malformed or out-of-policy request.
    BadRequest,
    /// 400: the chat names a model the release doesn't list.
    Model,
    /// 400: the chat asked for zero output tokens.
    ZeroOutput,
    /// 401: missing or wrong bearer token, or a wrong order secret.
    Unauthorized,
    /// 402: the Checkout Session isn't paid yet.
    Unpaid,
    /// 402: the session can't cover the next hold; a fresh session would help.
    SessionExhausted,
    /// 402: even a full session of this size can't cover the prompt. Carries `required`.
    PromptExceedsBudget,
    /// 404: no such route, session or order.
    NotFound,
    /// 404: recover found no row for the nullifier; resend to open.
    NoRow,
    /// 409: this nullifier is spent by different bytes.
    Spent,
    /// 409: this spend was settled, or the gate crashed; call recover.
    UseRecover,
    /// 409: another turn is still running in this session.
    TurnActive,
    /// 409: the session is closing; no more turns.
    Closing,
    /// 409: a different issue request was already answered for this order.
    OrderIssued,
    /// 409: the paid amount or currency doesn't match the product.
    AmountMismatch,
    /// 409: the posted manifest's signature, key, epoch, version or config hash doesn't match.
    ManifestMismatch,
    /// 413: the body is over the endpoint's limit.
    PayloadTooLarge,
    /// 500: an internal fault, with no detail.
    Internal,
    /// 502: the payment provider failed or answered unexpectedly.
    PaymentProvider,
    /// 503: the gate is locked, or the route isn't served in its state.
    Locked,
    /// 503: transient contention; retry the same bytes later.
    Retry,
    /// 503: the attestation report queue is full.
    Busy,
    /// 503: below the disk reserve; new work is refused.
    LowDisk,
    /// 503: the storefront has no Stripe key yet.
    Unconfigured,
    /// 503: no signed manifest is available to serve.
    NoManifest,
    /// 503: the inference engine couldn't start the turn. Nothing was charged.
    EngineUnavailable,
    /// A code this build doesn't know, from a newer peer.
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// The HTTP status this code is always sent with.
    pub fn http_status(self) -> u16 {
        use ErrorCode::{
            AmountMismatch, BadRequest, Busy, Closing, EngineUnavailable, Internal, Locked, LowDisk, ManifestMismatch,
            Model, NoManifest, NoRow, NotFound, OrderIssued, PayloadTooLarge, PaymentProvider, PromptExceedsBudget,
            Retry, SessionExhausted, Spent, TurnActive, Unauthorized, Unconfigured, Unknown, Unpaid, UseRecover,
            ZeroOutput,
        };
        match self {
            BadRequest | Model | ZeroOutput => 400,
            Unauthorized => 401,
            Unpaid | SessionExhausted | PromptExceedsBudget => 402,
            NotFound | NoRow => 404,
            Spent | UseRecover | TurnActive | Closing | OrderIssued | AmountMismatch | ManifestMismatch => 409,
            PayloadTooLarge => 413,
            Internal | Unknown => 500,
            PaymentProvider => 502,
            Locked | Retry | Busy | LowDisk | Unconfigured | NoManifest | EngineUnavailable => 503,
        }
    }

    /// The code as it appears on the wire, for messages and headers.
    pub fn as_str(self) -> &'static str {
        use ErrorCode::{
            AmountMismatch, BadRequest, Busy, Closing, EngineUnavailable, Internal, Locked, LowDisk, ManifestMismatch,
            Model, NoManifest, NoRow, NotFound, OrderIssued, PayloadTooLarge, PaymentProvider, PromptExceedsBudget,
            Retry, SessionExhausted, Spent, TurnActive, Unauthorized, Unconfigured, Unknown, Unpaid, UseRecover,
            ZeroOutput,
        };
        match self {
            BadRequest => "bad_request",
            Model => "model",
            ZeroOutput => "zero_output",
            Unauthorized => "unauthorized",
            Unpaid => "unpaid",
            SessionExhausted => "session_exhausted",
            PromptExceedsBudget => "prompt_exceeds_budget",
            NotFound => "not_found",
            NoRow => "no_row",
            Spent => "spent",
            UseRecover => "use_recover",
            TurnActive => "turn_active",
            Closing => "closing",
            OrderIssued => "order_issued",
            AmountMismatch => "amount_mismatch",
            ManifestMismatch => "manifest_mismatch",
            PayloadTooLarge => "payload_too_large",
            Internal => "internal",
            PaymentProvider => "payment_provider",
            Locked => "locked",
            Retry => "retry",
            Busy => "busy",
            LowDisk => "low_disk",
            Unconfigured => "unconfigured",
            NoManifest => "no_manifest",
            EngineUnavailable => "engine_unavailable",
            Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The body of every non-2xx response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// The code.
    pub error: ErrorCode,
    /// Credits a turn needs, only with `prompt_exceeds_budget`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<u64>,
}

impl ErrorBody {
    /// A body with just a code.
    pub fn new(error: ErrorCode) -> Self {
        Self { error, required: None }
    }

    /// `402 prompt_exceeds_budget {required}`.
    pub fn prompt_exceeds_budget(required: u64) -> Self {
        Self { error: ErrorCode::PromptExceedsBudget, required: Some(required) }
    }

    /// The HTTP status for this body.
    pub fn http_status(&self) -> u16 {
        self.error.http_status()
    }
}

impl From<ErrorCode> for ErrorBody {
    fn from(code: ErrorCode) -> Self {
        Self::new(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case_on_the_wire() {
        let body = ErrorBody::new(ErrorCode::UseRecover);
        assert_eq!(serde_json::to_string(&body).unwrap(), r#"{"error":"use_recover"}"#);
        let body = ErrorBody::prompt_exceeds_budget(42);
        assert_eq!(serde_json::to_string(&body).unwrap(), r#"{"error":"prompt_exceeds_budget","required":42}"#);
    }

    #[test]
    fn statuses_match_the_spec_tables() {
        assert_eq!(ErrorCode::Spent.http_status(), 409);
        assert_eq!(ErrorCode::NoRow.http_status(), 404);
        assert_eq!(ErrorCode::SessionExhausted.http_status(), 402);
        assert_eq!(ErrorCode::Locked.http_status(), 503);
        assert_eq!(ErrorCode::Unpaid.http_status(), 402);
        assert_eq!(ErrorCode::AmountMismatch.http_status(), 409);
    }

    #[test]
    fn names_match_the_wire() {
        use ErrorCode::*;
        let every = [
            BadRequest,
            Model,
            ZeroOutput,
            Unauthorized,
            Unpaid,
            SessionExhausted,
            PromptExceedsBudget,
            NotFound,
            NoRow,
            Spent,
            UseRecover,
            TurnActive,
            Closing,
            OrderIssued,
            AmountMismatch,
            ManifestMismatch,
            PayloadTooLarge,
            Internal,
            PaymentProvider,
            Locked,
            Retry,
            Busy,
            LowDisk,
            Unconfigured,
            NoManifest,
            EngineUnavailable,
        ];
        for code in every {
            assert_eq!(serde_json::to_value(code).unwrap(), code.as_str(), "{code:?}");
        }
    }

    #[test]
    fn unknown_codes_from_newer_peers_still_parse() {
        let body: ErrorBody = serde_json::from_str(r#"{"error":"draining"}"#).unwrap();
        assert_eq!(body.error, ErrorCode::Unknown);
    }
}
