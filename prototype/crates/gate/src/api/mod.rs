//! The HTTP surface: `gate.sock`'s routes and `issue.sock`'s (SPEC section 5).
//!
//! Handlers parse, call a flow, and respond (ARCHITECTURE 8.2 rule 3).
//! Every failure is one [`ErrorCode`] from the closed set, so no path,
//! prompt or internal message reaches a client, and no body is logged.

mod local;
mod operator;
mod public;
mod sessions;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use proto::{ErrorBody, ErrorCode};
use serde::de::DeserializeOwned;

use crate::state::{Gate, Ready};

/// Body limit for every request but chat: enough for a 3.5 KB spend.
const MAX_BODY: usize = 16 * 1024;
/// Body limit for chat, which carries the whole conversation.
const MAX_CHAT_BODY: usize = 1024 * 1024;

/// `gate.sock`'s routes, served inside inner TLS.
pub fn gate_router(gate: Arc<Gate>) -> Router {
    Router::new()
        .route("/v1/health", get(public::health))
        .route("/v1/manifest", get(public::manifest))
        .route("/v1/attestation", post(public::attestation))
        .route("/v1/sessions", post(sessions::open))
        .route("/v1/sessions/recover", post(sessions::recover))
        .route("/v1/sessions/{id}/chat", post(sessions::chat).layer(DefaultBodyLimit::max(MAX_CHAT_BODY)))
        .route("/v1/sessions/{id}/close", post(sessions::close))
        .route("/v1/operator/unlock", post(operator::unlock))
        .route("/v1/operator/manifest", post(operator::manifest))
        .route("/v1/operator/state", get(operator::state))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(gate)
}

/// `issue.sock`'s one route, plain HTTP for the storefront.
pub fn issue_router(gate: Arc<Gate>) -> Router {
    Router::new()
        .route("/local/issue", post(local::issue))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(gate)
}

async fn not_found() -> ApiError {
    ErrorCode::NotFound.into()
}

/// A failed request: the error body with its code's status.
#[derive(Debug)]
pub struct ApiError(ErrorBody);

impl From<ErrorCode> for ApiError {
    fn from(code: ErrorCode) -> Self {
        Self(ErrorBody::new(code))
    }
}

impl From<ErrorBody> for ApiError {
    fn from(body: ErrorBody) -> Self {
        Self(body)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0)).into_response()
    }
}

/// A JSON body, whatever its content type: `413 payload_too_large` over
/// the route's limit, `400 bad_request` if it isn't the expected shape.
/// Unknown fields are dropped, so the structs are the allowlist.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(request, state).await.map_err(|rejection| {
            if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ErrorCode::PayloadTooLarge
            } else {
                ErrorCode::BadRequest
            }
        })?;
        serde_json::from_slice(&bytes).map(Self).map_err(|_| ErrorCode::BadRequest.into())
    }
}

/// The gate's serving state. Extracted before the body, so every route
/// that needs Ready answers `503 locked` before it parses anything.
pub struct ReadyGate(pub Arc<Ready>);

impl FromRequestParts<Arc<Gate>> for ReadyGate {
    type Rejection = ApiError;

    async fn from_request_parts(_parts: &mut Parts, gate: &Arc<Gate>) -> Result<Self, ApiError> {
        Ok(Self(gate.ready()?))
    }
}
