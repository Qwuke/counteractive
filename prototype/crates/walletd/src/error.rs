//! How walletd's failures reach a local client: a status and a
//! [`LocalErrorBody`]. Gate errors keep the gate's code; walletd's own get
//! codes of their own and a message. No message carries secret material.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gate_conn::{AttestFailure, ConnError};
use proto::ErrorBody;
use proto::walletd::LocalErrorBody;
use wallet_core::WalletError;

/// A failed local API request.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: LocalErrorBody,
}

impl ApiError {
    /// An error of walletd's own.
    pub fn local(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self { status, body: LocalErrorBody { error: code.to_owned(), message: Some(message.into()), required: None } }
    }

    /// An error the gate sent, passed through with its status and code.
    pub fn gate(status: u16, body: &ErrorBody) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            body: LocalErrorBody { error: body.error.to_string(), message: None, required: body.required },
        }
    }

    /// No chain can open a session. With `required`, the prompt is too large
    /// for any session this wallet can open, which is the gate's
    /// `prompt_exceeds_budget`; without, there is simply nothing to spend.
    pub fn no_chain(required: Option<u64>) -> Self {
        match required {
            Some(required) => Self::gate(402, &ErrorBody::prompt_exceeds_budget(required)),
            None => Self::local(
                StatusCode::PAYMENT_REQUIRED,
                "no_spendable_chain",
                "no chain can open a session: buy credits, or finish the conversation using this wallet's chain",
            ),
        }
    }

    /// The `401` for a missing or wrong bearer token.
    pub fn unauthorized() -> Self {
        Self::local(StatusCode::UNAUTHORIZED, "unauthorized", "missing or wrong bearer token")
    }

    /// The gate's error code, if this came from the gate.
    pub fn code(&self) -> &str {
        &self.body.error
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<WalletError> for ApiError {
    fn from(error: WalletError) -> Self {
        let status = match error {
            WalletError::NoAcceptedManifest | WalletError::NoPendingManifest => StatusCode::CONFLICT,
            WalletError::Journal(_)
            | WalletError::File { .. }
            | WalletError::JournalKey(_)
            | WalletError::AlreadyInitialised
            | WalletError::Pins(_)
            | WalletError::Credit(_)
            | WalletError::Manifest(_)
            | WalletError::WrongState { .. }
            | WalletError::Corrupt(_)
            | WalletError::Entropy => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::local(status, "wallet", error.to_string())
    }
}

impl From<ConnError> for ApiError {
    fn from(error: ConnError) -> Self {
        match &error {
            ConnError::Api { status, error: Some(body) } => Self::gate(*status, body),
            _ => Self::local(StatusCode::BAD_GATEWAY, "gate_unreachable", error.to_string()),
        }
    }
}

impl From<AttestFailure> for ApiError {
    fn from(failure: AttestFailure) -> Self {
        match failure {
            AttestFailure::Conn(error) => error.into(),
            AttestFailure::Rejected(rejection) => Self::local(
                StatusCode::BAD_GATEWAY,
                "attestation_failed",
                format!("the gate's attestation was rejected, so nothing was sent: {rejection}"),
            ),
        }
    }
}
