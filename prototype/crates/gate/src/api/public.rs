//! Public routes, served in every state: health, manifest, attestation.

use std::sync::Arc;

use attest::Evidence;
use axum::Json;
use axum::extract::State;
use proto::gate::{AttestationRequest, GateHealth};
use proto::{ErrorCode, SignedManifest};

use super::{ApiError, ApiJson};
use crate::state::Gate;

/// `GET /v1/health`.
pub async fn health(State(gate): State<Arc<Gate>>) -> Json<GateHealth> {
    Json(GateHealth { state: gate.state(), version: gate.release.config.image_version.clone() })
}

/// `GET /v1/manifest`: the latest signed manifest, or `503 no_manifest`.
pub async fn manifest(State(gate): State<Arc<Gate>>) -> Result<Json<SignedManifest>, ApiError> {
    gate.served_manifest().map(Json).ok_or_else(|| ErrorCode::NoManifest.into())
}

/// `POST /v1/attestation`: evidence bound to this connection's TLS key and the client's nonce.
pub async fn attestation(
    State(gate): State<Arc<Gate>>,
    ApiJson(request): ApiJson<AttestationRequest>,
) -> Result<Json<Evidence>, ApiError> {
    Ok(Json(gate.attestor.evidence(request.nonce).await?))
}
