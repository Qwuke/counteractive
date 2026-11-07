//! `issue.sock`: issuance for the storefront, served only in Ready.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use proto::gate::{LocalIssueRequest, LocalIssueResponse};

use super::{ApiError, ApiJson, ReadyGate};
use crate::issue;
use crate::state::Gate;

/// `POST /local/issue`.
pub async fn issue(
    State(gate): State<Arc<Gate>>,
    ReadyGate(ready): ReadyGate,
    ApiJson(request): ApiJson<LocalIssueRequest>,
) -> Result<Json<LocalIssueResponse>, ApiError> {
    Ok(Json(issue::issue(&ready.unlocked, &gate.release.config, request).await?))
}
