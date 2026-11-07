//! Operator routes: K as a bearer token, over an attested connection.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use proto::gate::{OperatorStateResponse, StateResponse, UnlockResponse};
use proto::{ErrorCode, SignedManifest};

use super::{ApiError, ApiJson};
use crate::custody::StoreKey;
use crate::state::Gate;

/// An authenticated operator call, and the K it carried. Extracted before
/// the body, so a wrong K gets `401` before anything is parsed.
pub struct Operator(StoreKey);

impl FromRequestParts<Arc<Gate>> for Operator {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, gate: &Arc<Gate>) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(ErrorCode::Unauthorized)?;
        gate.custody.accept(token).map(Self).ok_or_else(|| ErrorCode::Unauthorized.into())
    }
}

/// `POST /v1/operator/unlock`.
pub async fn unlock(State(gate): State<Arc<Gate>>, Operator(key): Operator) -> Result<Json<UnlockResponse>, ApiError> {
    Ok(Json(gate.unlock(key).await?))
}

/// `POST /v1/operator/manifest`.
pub async fn manifest(
    State(gate): State<Arc<Gate>>,
    _operator: Operator,
    ApiJson(signed): ApiJson<SignedManifest>,
) -> Result<Json<StateResponse>, ApiError> {
    Ok(Json(gate.accept_manifest(signed).await?))
}

/// `GET /v1/operator/state`, in every boot state.
pub async fn state(State(gate): State<Arc<Gate>>, _operator: Operator) -> Json<OperatorStateResponse> {
    Json(OperatorStateResponse { state: gate.state(), live_sessions: gate.live_sessions() })
}
