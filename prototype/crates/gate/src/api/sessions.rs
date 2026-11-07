//! The use plane: open, chat, close, recover. Served only in Ready.

use std::future::Future;
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse as _, Response};
use proto::gate::{
    CloseResponse, OpenSessionRequest, OpenSessionResponse, RecoverRequest, RecoverResponse, SessionChatRequest,
};
use proto::{ErrorBody, ErrorCode, SessionId};

use super::{ApiError, ApiJson, ReadyGate};
use crate::state::Gate;

/// `POST /v1/sessions`.
pub async fn open(
    State(gate): State<Arc<Gate>>,
    ReadyGate(ready): ReadyGate,
    ApiJson(request): ApiJson<OpenSessionRequest>,
) -> Result<Json<OpenSessionResponse>, ApiError> {
    let sessions = Arc::clone(&ready.sessions);
    to_completion(&gate, async move { sessions.open(request.spend).await }).await.map(Json)
}

/// `POST /v1/sessions/{id}/chat`: an event stream once the engine has started.
pub async fn chat(
    ReadyGate(ready): ReadyGate,
    Path(id): Path<String>,
    ApiJson(request): ApiJson<SessionChatRequest>,
) -> Result<Response, ApiError> {
    let turn = ready.sessions.chat(session_id(&id)?, request).await?;
    Ok(([(CONTENT_TYPE, "text/event-stream"), (CACHE_CONTROL, "no-cache")], Body::from_stream(turn.into_body()))
        .into_response())
}

/// `POST /v1/sessions/{id}/close`.
pub async fn close(
    State(gate): State<Arc<Gate>>,
    ReadyGate(ready): ReadyGate,
    Path(id): Path<String>,
) -> Result<Json<CloseResponse>, ApiError> {
    let (sessions, id) = (Arc::clone(&ready.sessions), session_id(&id)?);
    to_completion(&gate, async move { sessions.close(id).await }).await.map(Json)
}

/// `POST /v1/sessions/recover`.
pub async fn recover(
    State(gate): State<Arc<Gate>>,
    ReadyGate(ready): ReadyGate,
    ApiJson(request): ApiJson<RecoverRequest>,
) -> Result<Json<RecoverResponse>, ApiError> {
    let sessions = Arc::clone(&ready.sessions);
    to_completion(&gate, async move { sessions.recover(request.spend).await }).await.map(Json)
}

/// A malformed id names no session.
fn session_id(text: &str) -> Result<SessionId, ApiError> {
    text.parse().map_err(|_| ErrorCode::NotFound.into())
}

/// Runs a flow to the end even if the client disconnects: one cut off
/// between its durable steps would leave a live-looking session behind,
/// or a settled one whose answer nobody can ask for again.
async fn to_completion<T: Send + 'static>(
    gate: &Gate,
    flow: impl Future<Output = Result<T, ErrorBody>> + Send + 'static,
) -> Result<T, ApiError> {
    match gate.tasks().spawn(flow).await {
        Ok(result) => result.map_err(ApiError::from),
        Err(_) => Err(ErrorCode::Internal.into()),
    }
}
