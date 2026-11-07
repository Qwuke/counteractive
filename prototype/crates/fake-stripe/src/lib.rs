//! A local stand-in for the two Stripe Checkout Session endpoints the
//! storefront calls (SPEC section 6), so tests run the storefront's real
//! HTTP client against a server on the same machine.
//!
//! - `POST /v1/checkout/sessions` and `GET /v1/checkout/sessions/{id}`:
//!   Stripe's shapes, bearer key required (any non-empty key), sessions in
//!   memory only.
//! - `GET /pay/{id}`: the page a session's `url` points at, with a Pay
//!   button that POSTs to `/pay/{id}` and is redirected to the session's
//!   `success_url`, as Checkout does.
//! - `POST /test/pay/{id}`: marks a session paid from a test, no browser.
//!
//! It's a library so the storefront's tests can run it in-process; the
//! binary wraps [`FakeStripe::start`].

mod page;
mod session;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::rejection::FormRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse as _, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use proto::stripe::{CheckoutCreate, StripeErrorBody};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::session::Sessions;

/// Stripe's error type for bad parameters, unknown ids and missing keys alike.
const INVALID_REQUEST: &str = "invalid_request_error";

/// A running fake Stripe on a TCP port.
#[derive(Debug)]
pub struct FakeStripe {
    addr: SocketAddr,
    shutdown: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
}

impl FakeStripe {
    /// Binds `listen` (port 0 picks a free one) and serves until [`FakeStripe::stop`].
    pub async fn start(listen: SocketAddr) -> std::io::Result<Self> {
        let listener = TcpListener::bind(listen).await?;
        let addr = listener.local_addr()?;
        let shutdown = CancellationToken::new();
        let app = router(Arc::new(App { sessions: Sessions::default(), base_url: format!("http://{addr}") }));
        let signal = shutdown.clone().cancelled_owned();
        let task = tokio::spawn(async move { axum::serve(listener, app).with_graceful_shutdown(signal).await });
        Ok(Self { addr, shutdown, task })
    }

    /// The bound address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://<addr>`: the storefront's `--stripe-base-url`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Stops accepting, lets open requests finish, and returns the server's result.
    pub async fn stop(self) -> std::io::Result<()> {
        self.shutdown.cancel();
        self.task.await.map_err(std::io::Error::other)?
    }
}

struct App {
    sessions: Sessions,
    /// Where this server is reached, for pay page URLs.
    base_url: String,
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/checkout/sessions", post(create_session))
        .route("/v1/checkout/sessions/{id}", get(get_session))
        .route("/pay/{id}", get(pay_page).post(pay_and_return))
        .route("/test/pay/{id}", post(test_pay))
        .with_state(app)
}

/// `POST /v1/checkout/sessions`, form-encoded as the storefront sends it.
async fn create_session(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if !has_api_key(&headers) {
        return stripe_error(StatusCode::UNAUTHORIZED, None);
    }
    let Ok(Form(fields)) = form else {
        return stripe_error(StatusCode::BAD_REQUEST, Some("parameter_invalid"));
    };
    match CheckoutCreate::from_form(&fields) {
        Ok(order) => Json(app.sessions.create(order).to_json(&app.base_url)).into_response(),
        Err(_) => stripe_error(StatusCode::BAD_REQUEST, Some("parameter_missing")),
    }
}

/// `GET /v1/checkout/sessions/{id}`.
async fn get_session(State(app): State<Arc<App>>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if !has_api_key(&headers) {
        return stripe_error(StatusCode::UNAUTHORIZED, None);
    }
    match app.sessions.get(&id) {
        Some(session) => Json(session.to_json(&app.base_url)).into_response(),
        None => missing(),
    }
}

/// The page a buyer's browser opens from `checkout_url`.
async fn pay_page(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app.sessions.get(&id) {
        Some(session) => Html(page::pay_page(&session)).into_response(),
        None => (StatusCode::NOT_FOUND, Html("<p>No such checkout.</p>")).into_response(),
    }
}

/// The Pay button: marks the session paid and sends the browser to `success_url`.
async fn pay_and_return(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app.sessions.pay(&id) {
        Some(session) => Redirect::to(&session.order.success_url).into_response(),
        None => (StatusCode::NOT_FOUND, Html("<p>No such checkout.</p>")).into_response(),
    }
}

/// `POST /test/pay/{id}`: what a buyer pressing Pay does, for tests. Returns the session.
async fn test_pay(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app.sessions.pay(&id) {
        Some(session) => Json(session.to_json(&app.base_url)).into_response(),
        None => missing(),
    }
}

/// Whether the request carries `Authorization: Bearer <non-empty key>`.
/// Any key is accepted: this fake has one account and no real keys.
fn has_api_key(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|key| !key.trim().is_empty())
}

/// Stripe's answer for an unknown id: 404 with code `resource_missing`.
fn missing() -> Response {
    stripe_error(StatusCode::NOT_FOUND, Some(StripeErrorBody::RESOURCE_MISSING))
}

fn stripe_error(status: StatusCode, code: Option<&str>) -> Response {
    (status, Json(StripeErrorBody::new(INVALID_REQUEST, code))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn api_key_must_be_a_non_empty_bearer() {
        let with = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::AUTHORIZATION, HeaderValue::from_static(value));
            headers
        };
        assert!(has_api_key(&with("Bearer sk_test_1")));
        assert!(!has_api_key(&with("Bearer ")));
        assert!(!has_api_key(&with("Basic sk_test_1")));
        assert!(!has_api_key(&HeaderMap::new()));
    }
}
