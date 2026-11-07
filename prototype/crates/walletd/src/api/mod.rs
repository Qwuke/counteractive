//! The local API on loopback (docs 04 section 3, SPEC section 7).
//!
//! Browsers can reach localhost, so every request must carry a `Host` of
//! this listener (which stops DNS rebinding), and every route but the page
//! and `/paid` needs the bearer token. No body is ever logged.

mod openai;
mod page;
mod wallet;

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRequest, Request, State};
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, HOST};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use crate::state::Walletd;

/// Body limit for every request but chat.
const MAX_BODY: usize = 16 * 1024;
/// Body limit for chat, which carries the whole conversation; the gate's own.
const MAX_CHAT_BODY: usize = 1024 * 1024;

/// The router, with the host check on every route and the token on most.
pub fn router(walletd: Arc<Walletd>) -> Router {
    let protected = Router::new()
        .route("/v1/models", get(openai::models))
        .route("/v1/chat/completions", post(openai::chat).layer(DefaultBodyLimit::max(MAX_CHAT_BODY)))
        .route("/wallet/balance", get(wallet::balance))
        .route("/wallet/close", post(wallet::close))
        .route("/wallet/reconcile", post(wallet::reconcile))
        .route("/wallet/manifest", get(wallet::manifest_status).post(wallet::manifest_action))
        .route_layer(middleware::from_fn_with_state(Arc::clone(&walletd), require_token));
    Router::new()
        .route("/", get(page::index))
        .route("/app.js", get(page::script))
        .route("/style.css", get(page::style))
        .route("/paid", get(page::paid))
        .merge(protected)
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(Arc::clone(&walletd), require_host))
        .with_state(walletd)
}

async fn not_found() -> ApiError {
    ApiError::local(StatusCode::NOT_FOUND, "not_found", "no such route")
}

/// Runs a flow that talks to the gate to its end even if the client hangs
/// up, as the gate runs its own. A client that leaves drops its handler,
/// and a flow dropped between two journalled steps would strand its chain
/// in a session nobody finishes until the next reconcile, or lose a
/// receipt the gate charged for.
async fn to_completion<T: Send + 'static>(
    flow: impl Future<Output = Result<T, ApiError>> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::spawn(flow).await.unwrap_or_else(|_| {
        Err(ApiError::local(StatusCode::INTERNAL_SERVER_ERROR, "internal", "the request's task failed"))
    })
}

/// Refuses a request whose `Host` isn't this listener's.
async fn require_host(State(walletd): State<Arc<Walletd>>, request: Request, next: Next) -> Result<Response, ApiError> {
    let host = request.headers().get(HOST).and_then(|value| value.to_str().ok());
    if !host.is_some_and(|host| walletd.allowed_hosts.iter().any(|allowed| allowed == host)) {
        return Err(ApiError::local(StatusCode::FORBIDDEN, "bad_host", "requests must name this walletd's address"));
    }
    Ok(next.run(request).await)
}

/// Refuses a request without the bearer token.
async fn require_token(
    State(walletd): State<Arc<Walletd>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !token.is_some_and(|token| walletd.token.matches(token)) {
        return Err(ApiError::unauthorized());
    }
    Ok(next.run(request).await)
}

/// A JSON body: `413` over the route's limit, `400` if malformed. Unknown
/// fields are dropped, so the request structs are the allowlist.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(request, state).await.map_err(|rejection| {
            if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::local(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", "request body too large")
            } else {
                ApiError::local(StatusCode::BAD_REQUEST, "bad_request", "could not read the request body")
            }
        })?;
        // An empty body means the request's defaults.
        let bytes = if bytes.is_empty() { Bytes::from_static(b"{}") } else { bytes };
        serde_json::from_slice(&bytes)
            .map(Self)
            .map_err(|_| ApiError::local(StatusCode::BAD_REQUEST, "bad_request", "malformed JSON body"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn a_flow_finishes_after_its_client_leaves() {
        let (finished, finish) = tokio::sync::oneshot::channel();
        let flow = to_completion(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = finished.send(());
            Ok(())
        });
        // The client hangs up: its handler, awaiting the flow, is dropped.
        tokio::time::timeout(Duration::from_millis(5), flow).await.unwrap_err();
        finish.await.expect("the flow ran on to its end");
    }
}
