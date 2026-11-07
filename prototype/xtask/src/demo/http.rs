//! The console's HTTP side, on loopback: the page, its API, and the proxy
//! to walletd.
//!
//! As walletd's own API does, every request must name this listener in
//! `Host` (no DNS rebinding) and every `/api/` route needs the page's
//! token (no cross-site requests). The proxy forwards only the wallet's
//! routes the page uses, and adds walletd's token itself.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{any, get, post};
use axum::{Json, http};
use futures::{Stream, StreamExt as _};
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use proto::walletd::{
    CONVERSATION_HEADER, ManifestAction, ManifestActionRequest, ManifestStatus, RECOVERED_HEADER, ROLLOVER_HEADER,
};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use zeroize::Zeroizing;

use super::console::{Console, Event, Kind, Link, Purchase, PurchaseStatus, UsePlaneCheck, stderr_lines};
use crate::e2e::world::CliOutput;

/// Body limit for the console's own routes.
const MAX_BODY: usize = 16 * 1024;
/// Body limit for chat, which carries the whole conversation; walletd's own.
const MAX_CHAT_BODY: usize = 1024 * 1024;
/// The page loads nothing from anywhere else and runs no inline code.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; \
                   base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const INDEX: &str = include_str!("page/index.html");
const SCRIPT: &str = include_str!("page/app.js");
const STYLE: &str = include_str!("page/style.css");
const PAID: &str = include_str!("page/paid.html");

/// Serves the console on `listener` until the task is dropped.
pub async fn serve(listener: TcpListener, console: Arc<Console>) -> std::io::Result<()> {
    axum::serve(listener, router(console)).await
}

/// The router, with the host check on every route and the token on `/api/`.
pub fn router(console: Arc<Console>) -> Router {
    let api = Router::new()
        .route("/api/state", get(state))
        .route("/api/events", get(events))
        .route("/api/manifest", post(manifest))
        .route("/api/buy", post(buy))
        .route("/api/wallet/{*path}", any(proxy).layer(DefaultBodyLimit::max(MAX_CHAT_BODY)))
        .route_layer(middleware::from_fn_with_state(Arc::clone(&console), require_token));
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(|| async { asset("text/javascript; charset=utf-8", SCRIPT) }))
        .route("/style.css", get(|| async { asset("text/css; charset=utf-8", STYLE) }))
        .route("/paid", get(paid))
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .merge(api)
        .fallback(|| async { error(StatusCode::NOT_FOUND, "not_found", "no such route") })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(Arc::clone(&console), require_host))
        .layer(middleware::from_fn(security_headers))
        .with_state(console)
}

/// A JSON error, in walletd's shape.
fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    #[derive(Serialize)]
    struct Body<'a> {
        error: &'a str,
        message: String,
    }
    (status, Json(Body { error: code, message: message.into() })).into_response()
}

fn asset(content_type: &'static str, text: &'static str) -> Response {
    ([(CONTENT_TYPE, content_type)], text).into_response()
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // The checkout link opens fake-stripe; it learns nothing of this page.
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.entry(CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
    response
}

/// Refuses a request whose `Host` isn't this listener's.
async fn require_host(State(console): State<Arc<Console>>, request: Request, next: Next) -> Response {
    let host = request.headers().get(HOST).and_then(|value| value.to_str().ok());
    if !host.is_some_and(|host| console.hosts.iter().any(|allowed| allowed == host)) {
        return error(StatusCode::FORBIDDEN, "bad_host", "requests must name this console's address");
    }
    next.run(request).await
}

/// Refuses an `/api/` request without the page's token.
async fn require_token(State(console): State<Arc<Console>>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !presented.is_some_and(|token| same(token.as_bytes(), console.token.as_bytes())) {
        return error(StatusCode::UNAUTHORIZED, "unauthorized", "the console's token is missing or wrong");
    }
    next.run(request).await
}

/// Compares in time independent of where the bytes differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// The page, carrying the console's token (never walletd's).
async fn index(State(console): State<Arc<Console>>) -> Response {
    let page = INDEX.replace("{{CONSOLE_TOKEN}}", console.token.as_str());
    ([(CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
}

/// Where Checkout sends the buyer: it wakes the collector, which turns the
/// paid order into credit while the page watches.
async fn paid(State(console): State<Arc<Console>>) -> Response {
    console.collect_now.notify_one();
    console.event("fake-stripe", Kind::Info, "the buyer paid and checkout sent them back to the console");
    asset("text/html; charset=utf-8", PAID)
}

async fn state(State(console): State<Arc<Console>>) -> Response {
    Json(console.view()).into_response()
}

/// The activity log as server-sent events: everything so far, then live.
async fn events(State(console): State<Arc<Console>>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let (history, receiver) = console.subscribe();
    let last = history.last().map_or(0, |event| event.seq);
    let live = futures::stream::unfold((receiver, last), |(mut receiver, last)| async move {
        loop {
            match receiver.recv().await {
                Ok(event) if event.seq > last => {
                    let seq = event.seq;
                    return Some((event, (receiver, seq)));
                }
                // A page too slow to keep up misses a few lines.
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return None,
            }
        }
    });
    let stream = futures::stream::iter(history)
        .chain(live)
        .map(|event: Event| Ok(SseEvent::default().json_data(&event).unwrap_or_default()));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// What one `wallet-cli` run printed, for the page's terminal box.
#[derive(Serialize)]
struct CommandRun {
    command: String,
    ok: bool,
    stdout: Vec<String>,
    stderr: Vec<String>,
}

impl CommandRun {
    fn new(args: &[&str], output: &CliOutput) -> Self {
        Self {
            command: format!("wallet-cli {}", args.join(" ")),
            ok: output.code == Some(0),
            stdout: output.stdout.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect(),
            stderr: stderr_lines(&output.stderr),
        }
    }
}

#[derive(Deserialize)]
struct ManifestRequest {
    action: String,
}

/// `wallet-cli manifest fetch` or `approve`.
async fn manifest(State(console): State<Arc<Console>>, Json(request): Json<ManifestRequest>) -> Response {
    let args: &[&str] = match request.action.as_str() {
        "fetch" => &["manifest", "fetch"],
        "approve" => &["manifest", "approve"],
        _ => return error(StatusCode::BAD_REQUEST, "bad_request", "the action is `fetch` or `approve`"),
    };
    match console.wallet_cli(args).await {
        Ok(output) => {
            if request.action == "approve" && output.code == Some(0) && console.start_use_plane_check() {
                tokio::spawn(check_use_plane(Arc::clone(&console)));
            }
            Json(CommandRun::new(args, &output)).into_response()
        }
        Err(cause) => error(StatusCode::SERVICE_UNAVAILABLE, "not_ready", format!("{cause:#}")),
    }
}

/// Once the user approves the manifest the storefront served, walletd
/// fetches the gate's own over the use plane: through Arti and the onion
/// service when over Tor. Getting the same manifest, byte for byte, leaves
/// nothing pending; anything else would wait for approval. It also builds
/// Arti's first path to the onion service before the first chat needs one.
async fn check_use_plane(console: Arc<Console>) {
    let Some(link) = console.link() else { return };
    let via = if console.view().over_tor { "through Arti and the gate's onion service" } else { "on gate.sock" };
    console.event(
        "walletd",
        Kind::Info,
        format!("fetching the gate's manifest {via}, to compare with the approved one"),
    );
    let started = Instant::now();
    let found = async {
        let fetch = serde_json::to_vec(&ManifestActionRequest { action: ManifestAction::Fetch })?;
        let (status, body) = walletd_call(link, Method::POST, "wallet/manifest", fetch.into()).await?;
        anyhow::ensure!(status.is_success(), "walletd answered {status}: {}", String::from_utf8_lossy(&body));
        anyhow::Ok(serde_json::from_slice::<ManifestStatus>(&body)?)
    }
    .await;
    let elapsed = started.elapsed();
    let (ok, text) = match found {
        Ok(ManifestStatus { accepted: Some(accepted), pending: None }) => (
            true,
            format!(
                "walletd reached the gate {via} in {:.1} s: it serves manifest v{}, byte for byte the one you approved",
                elapsed.as_secs_f64(),
                accepted.version
            ),
        ),
        Ok(ManifestStatus { pending: Some(pending), .. }) => {
            (false, format!("the gate serves manifest v{} {via}, not the approved one", pending.version))
        }
        Ok(ManifestStatus { accepted: None, pending: None }) => (false, "walletd has no accepted manifest".to_owned()),
        Err(cause) => (false, format!("walletd could not fetch the gate's manifest {via}: {cause:#}")),
    };
    console.event("walletd", if ok { Kind::Ok } else { Kind::Warn }, text.clone());
    let after_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    console.set_use_plane_check(UsePlaneCheck { ok, after_ms, text });
}

/// One whole request to walletd, as the console itself.
async fn walletd_call(link: &Link, method: Method, path: &str, body: Bytes) -> anyhow::Result<(StatusCode, Bytes)> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let response = forward(link, &method, path, &headers, body).await?;
    let status = response.status();
    Ok((status, response.into_body().collect().await?.to_bytes()))
}

#[derive(Deserialize)]
struct BuyRequest {
    product: String,
}

#[derive(Serialize)]
struct BuyResponse {
    run: CommandRun,
    order_id: Option<String>,
    checkout_url: Option<String>,
}

/// `wallet-cli buy --product <id> --no-browser`: the page opens checkout.
async fn buy(State(console): State<Arc<Console>>, Json(request): Json<BuyRequest>) -> Response {
    let Some(product) = console.product(&request.product) else {
        return error(StatusCode::BAD_REQUEST, "unknown_product", "the release sells no such product");
    };
    let args = ["buy", "--product", product.id.as_str(), "--no-browser"];
    let output = match console.wallet_cli(&args).await {
        Ok(output) => output,
        Err(cause) => return error(StatusCode::SERVICE_UNAVAILABLE, "not_ready", format!("{cause:#}")),
    };
    let field =
        |name: &str| output.stdout.lines().find_map(|line| line.strip_prefix(name)).map(|v| v.trim().to_owned());
    let order_id = field("order_id:");
    // Only a web URL becomes a link on the page.
    let checkout_url = field("checkout_url:").filter(|url| url.starts_with("http://") || url.starts_with("https://"));
    if let (true, Some(order_id), Some(checkout_url)) = (output.code == Some(0), &order_id, &checkout_url) {
        console.add_purchase(Purchase {
            order_id: order_id.clone(),
            product: product.id.clone(),
            usd_cents: product.usd_cents,
            credits: product.credits,
            checkout_url: checkout_url.clone(),
            status: PurchaseStatus::AwaitingPayment,
            chain: None,
        });
    }
    let status = if output.code == Some(0) { StatusCode::OK } else { StatusCode::BAD_GATEWAY };
    (status, Json(BuyResponse { run: CommandRun::new(&args, &output), order_id, checkout_url })).into_response()
}

/// Forwards one of the wallet's routes to walletd with its token.
async fn proxy(
    State(console): State<Arc<Console>>,
    method: Method,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let allowed = matches!(
        (method.as_str(), path.as_str()),
        ("GET", "v1/models" | "wallet/balance" | "wallet/manifest")
            | ("POST", "v1/chat/completions" | "wallet/close" | "wallet/reconcile")
    );
    if !allowed {
        return error(StatusCode::NOT_FOUND, "not_found", "the console forwards only the wallet routes the page uses");
    }
    let Some(link) = console.link() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "starting", "walletd is still starting");
    };
    let logged = method == Method::POST;
    let started = Instant::now();
    match forward(link, &method, &path, &headers, body).await {
        Ok(response) => {
            if logged {
                let status = response.status();
                let kind = if status.is_success() { Kind::Info } else { Kind::Warn };
                let elapsed = started.elapsed().as_secs_f64();
                console.event("walletd", kind, format!("POST /{path}: {status} after {elapsed:.2} s"));
            }
            response
        }
        Err(cause) => {
            console.event("walletd", Kind::Error, format!("{method} /{path}: {cause:#}"));
            error(StatusCode::BAD_GATEWAY, "walletd_unreachable", format!("{cause:#}"))
        }
    }
}

async fn forward(
    link: &Link,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> anyhow::Result<Response> {
    let authorization = Zeroizing::new(format!("Bearer {}", link.walletd_token.as_str()));
    let mut request = http::Request::builder()
        .method(method)
        .uri(format!("/{path}"))
        .header(HOST, link.walletd.to_string())
        .header(AUTHORIZATION, authorization.as_str());
    for name in [CONTENT_TYPE, HeaderName::from_static(CONVERSATION_HEADER)] {
        if let Some(value) = headers.get(&name) {
            request = request.header(name, value);
        }
    }
    let request = request.body(Full::new(body))?;
    let stream = TcpStream::connect(link.walletd).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let (parts, body) = sender.send_request(request).await?.into_parts();
    let mut response = Response::builder().status(parts.status);
    let passed = [
        CONTENT_TYPE,
        CACHE_CONTROL,
        HeaderName::from_static(ROLLOVER_HEADER),
        HeaderName::from_static(RECOVERED_HEADER),
    ];
    for name in passed {
        if let Some(value) = parts.headers.get(&name) {
            response = response.header(name, value);
        }
    }
    // The body streams through as walletd sends it: chat is SSE.
    Ok(response.body(Body::new(body))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_whole() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"ab"));
    }
}
