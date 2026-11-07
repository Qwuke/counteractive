//! The embedded page and the static `/paid` page.
//!
//! The page gets the bearer token only inside its HTML, which is served
//! only at this listener's own host (checked before any route runs). A
//! strict CSP allows nothing but this origin's script, style and API, so
//! model text, inserted with `textContent`, can't load or run anything.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderValue;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use axum::response::{IntoResponse as _, Response};

use crate::state::Walletd;

const INDEX: &str = include_str!("../../page/index.html");
const SCRIPT: &str = include_str!("../../page/app.js");
const STYLE: &str = include_str!("../../page/style.css");
const PAID: &str = include_str!("../../page/paid.html");

/// Where the token goes in `index.html`.
const TOKEN_SLOT: &str = "{{TOKEN}}";

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; \
                   img-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// `GET /`: the page, with the token for its API calls.
pub async fn index(State(walletd): State<Arc<Walletd>>) -> Response {
    let html = INDEX.replace(TOKEN_SLOT, walletd.token.expose());
    respond("text/html; charset=utf-8", html, "no-store")
}

/// `GET /app.js`.
pub async fn script() -> Response {
    respond("text/javascript; charset=utf-8", SCRIPT.to_owned(), "no-cache")
}

/// `GET /style.css`.
pub async fn style() -> Response {
    respond("text/css; charset=utf-8", STYLE.to_owned(), "no-cache")
}

/// `GET /paid`: where Checkout returns the buyer. Static, no token.
pub async fn paid() -> Response {
    respond("text/html; charset=utf-8", PAID.to_owned(), "no-cache")
}

fn respond(content_type: &'static str, body: String, cache: &'static str) -> Response {
    let headers = [
        (CONTENT_TYPE, HeaderValue::from_static(content_type)),
        (CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP)),
        (X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        (CACHE_CONTROL, HeaderValue::from_static(cache)),
    ];
    (headers, body).into_response()
}
