//! `xtask demo --check`: what a person does on the console page, done
//! through the same HTTP API the page calls, checking every number.
//!
//! The page, its CSP and the console's guards; the manifest, and walletd
//! fetching the same one over the use plane; a purchase
//! paid on fake-stripe's checkout page and collected; two chat turns
//! streamed through the proxy and walletd (over Tor unless `--local`),
//! their receipts against the release's prices; then the close, the
//! reconcile, and a balance that is exactly the purchase minus the charges.
//!
//! It chats with the release's first model. fake-engine's replies are
//! paced words, one token each, so against it the check also counts the
//! words and the pacing; a real model's tokens aren't words.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use axum::http;
use axum::http::header::{AUTHORIZATION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, LOCATION};
use axum::http::{HeaderMap, Method, StatusCode};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use proto::walletd::{
    BalanceView, CONVERSATION_HEADER, ManifestStatus, ReconcileReport, SettlementOutcome, WalletCloseResponse,
};
use proto::{FinishReason, Receipt};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::net::TcpStream;

use super::console::{Console, Model, TOKEN_DELAY_MS};

/// How long a paid order may take to become credit.
const COLLECT_TIMEOUT: Duration = Duration::from_secs(60);
/// How long walletd's first fetch over the use plane may take: over Tor,
/// Arti's first path to a fresh onion service.
const USE_PLANE_TIMEOUT: Duration = Duration::from_secs(300);
/// The product the check buys, and what it holds.
const PRODUCT: &str = "usd-1";
const PRODUCT_CREDITS: u64 = 1_000_000;
/// The first prompt: one that has a real model answer at some length, so
/// its reply streams in many pieces.
const FIRST_PROMPT: &str = "Count from one to twenty in words, separated by commas.";

/// Runs the check against the console on `console_addr`.
pub async fn run(console: &Console, console_addr: SocketAddr, fake_stripe: SocketAddr) -> anyhow::Result<()> {
    let client = Client { addr: console_addr, token: console.token.as_str().to_owned() };
    let started = Instant::now();
    println!("\ndemo check against http://{console_addr}/");
    let model = console.view().models.first().cloned().context("the release has no model")?;

    page(&client).await?;
    guards(&client).await?;
    let manifest = approve_manifest(&client).await?;
    ok(format!("manifest v{manifest} fetched from the storefront, checked against the pinned operator key, approved"));
    let order = buy_and_pay(&client, console, fake_stripe).await?;
    ok(format!("order {order}: bought {PRODUCT}, paid on fake-stripe's page, sent back to the console's /paid"));
    collected(&client, &order).await?;
    let balance: BalanceView = client.get_json("/api/wallet/wallet/balance").await?;
    ensure!(balance.total_credits == PRODUCT_CREDITS, "balance {} after buying {PRODUCT}", balance.total_credits);
    ok(format!("walletd's balance through the proxy: {} credits ({})", balance.total_credits, balance.total_usd));

    let models: Value = client.get_json("/api/wallet/v1/models").await?;
    let ids: Vec<&str> =
        models["data"].as_array().into_iter().flatten().filter_map(|model| model["id"].as_str()).collect();
    ensure!(ids.contains(&model.id.as_str()), "models through the proxy: {ids:?}");
    ok(format!("models through the proxy: {}", ids.join(", ")));
    // A person takes this long to buy and pay; the check waits for it.
    use_plane(&client).await?;

    let conversation = "c0ffee00c0ffee00c0ffee00c0ffee00";
    let mut history = vec![json!({"role": "user", "content": FIRST_PROMPT})];
    let first = chat(&client, &model.id, conversation, &history).await?;
    first.check(1, &model, "twenty")?;
    ok(first.describe());
    history.push(json!({"role": "assistant", "content": first.text}));
    history.push(json!({"role": "user", "content": "And now from twenty-one to thirty."}));
    let second = chat(&client, &model.id, conversation, &history).await?;
    second.check(2, &model, "thirty")?;
    ensure!(
        second.receipt.remaining == first.receipt.remaining - second.receipt.charge,
        "the session had {} left and turn 2 charged {}, but {} remain",
        first.receipt.remaining,
        second.receipt.charge,
        second.receipt.remaining
    );
    ok(second.describe());
    let charged = first.receipt.charge + second.receipt.charge;

    let closed: WalletCloseResponse =
        client.post_json("/api/wallet/wallet/close", &json!({"conversation_id": conversation})).await?;
    ensure!(closed.closed == 1, "closing the conversation: {closed:?}");
    let report: ReconcileReport = client.post_json("/api/wallet/wallet/reconcile", &json!({})).await?;
    let [session] = report.sessions.as_slice() else { bail!("expected one settled session: {:?}", report.sessions) };
    ensure!(
        session.outcome == SettlementOutcome::Exact
            && session.receipts_sum == charged
            && session.s - session.t == charged,
        "the settlement: {session:?}, charged {charged}"
    );
    ok(format!("closed and reconciled: s {} - t {} = receipts {charged}, exact", session.s, session.t));
    let balance: BalanceView = client.get_json("/api/wallet/wallet/balance").await?;
    ensure!(
        balance.total_credits == PRODUCT_CREDITS - charged && balance.chains.iter().all(|chain| chain.resolved),
        "balance {} != {PRODUCT_CREDITS} - {charged}: {:?}",
        balance.total_credits,
        balance.chains
    );
    ok(format!("balance {} = {PRODUCT_CREDITS} - {charged} charged, exactly", balance.total_credits));

    let (log, _) = console.subscribe();
    ensure!(
        log.iter().any(|event| event.text.starts_with("collected usd-1: chain")),
        "the activity log never showed the collect"
    );
    ok(format!("the activity log has {} entries, the collect among them", log.len()));
    println!("demo check: PASS in {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}

fn ok(what: impl std::fmt::Display) {
    println!("    ok  {what}");
}

/// The page, its assets and its headers.
async fn page(client: &Client) -> anyhow::Result<()> {
    let reply = client.send(Method::GET, "/", None, None, &[]).await?;
    ensure!(reply.status == StatusCode::OK, "GET /: {}", reply.status);
    let csp = reply.header(CONTENT_SECURITY_POLICY.as_str());
    ensure!(
        csp.contains("default-src 'none'")
            && csp.contains("script-src 'self'")
            && csp.contains("frame-ancestors 'none'"),
        "the page's CSP: {csp}"
    );
    let html = String::from_utf8_lossy(&reply.body);
    ensure!(
        html.contains(&format!("name=\"console-token\" content=\"{}\"", client.token)),
        "the page does not carry the console's token"
    );
    ensure!(!html.contains("{{"), "the page has an unfilled placeholder");
    for (path, kind) in [("/app.js", "text/javascript"), ("/style.css", "text/css"), ("/paid", "text/html")] {
        let reply = client.send(Method::GET, path, None, None, &[]).await?;
        ensure!(
            reply.status == StatusCode::OK && reply.header(CONTENT_TYPE.as_str()).starts_with(kind),
            "GET {path}: {} {}",
            reply.status,
            reply.header(CONTENT_TYPE.as_str())
        );
    }
    let icon = client.send(Method::GET, "/favicon.ico", None, None, &[]).await?;
    ensure!(icon.status == StatusCode::NO_CONTENT, "GET /favicon.ico: {}", icon.status);
    ok("the page, app.js, style.css and /paid are served, with a CSP that allows only this origin and no framing");
    Ok(())
}

/// The console's guards: the host check, the token, the proxy's allowlist.
async fn guards(client: &Client) -> anyhow::Result<()> {
    let token = Some(client.token.as_str());
    let rebound = client.send(Method::GET, "/api/state", Some("evil.example:80"), token, &[]).await?;
    ensure!(rebound.status == StatusCode::FORBIDDEN, "a foreign Host got {}", rebound.status);
    let anonymous = client.send(Method::GET, "/api/state", None, None, &[]).await?;
    ensure!(anonymous.status == StatusCode::UNAUTHORIZED, "no token got {}", anonymous.status);
    let wrong = client.send(Method::GET, "/api/wallet/wallet/balance", None, Some("0000"), &[]).await?;
    ensure!(wrong.status == StatusCode::UNAUTHORIZED, "a wrong token got {}", wrong.status);
    for (method, path) in [(Method::GET, "/api/wallet/wallet/close"), (Method::POST, "/api/wallet/v1/sessions")] {
        let reply = client.send(method.clone(), path, None, token, &[]).await?;
        ensure!(reply.status == StatusCode::NOT_FOUND, "{method} {path} got {}", reply.status);
    }
    let state: Value = client.get_json("/api/state").await?;
    ensure!(state["phase"] == "ready", "the console is {}", state["phase"]);
    let unknown = client.send_json(Method::POST, "/api/buy", &json!({"product": "usd-1000"})).await?;
    ensure!(unknown.status == StatusCode::BAD_REQUEST, "an unknown product got {}", unknown.status);
    ok("a foreign Host gets 403, a missing or wrong token 401, a route outside the allowlist 404, \
        an unknown product 400");
    Ok(())
}

/// Fetches and approves the manifest; returns its version.
async fn approve_manifest(client: &Client) -> anyhow::Result<u64> {
    let fetched: Value = client.post_json("/api/manifest", &json!({"action": "fetch"})).await?;
    ensure!(fetched["ok"] == true, "manifest fetch: {fetched}");
    let status: ManifestStatus = client.get_json("/api/wallet/wallet/manifest").await?;
    ensure!(status.accepted.is_none() && status.pending.is_some(), "after fetch: {status:?}");
    let approved: Value = client.post_json("/api/manifest", &json!({"action": "approve"})).await?;
    ensure!(approved["ok"] == true, "manifest approve: {approved}");
    let status: ManifestStatus = client.get_json("/api/wallet/wallet/manifest").await?;
    Ok(status.accepted.context("no manifest accepted after approve")?.version)
}

/// Buys through the console, then pays as a person on the checkout page
/// would: loads it and presses Pay. Returns the order id.
async fn buy_and_pay(client: &Client, console: &Console, fake_stripe: SocketAddr) -> anyhow::Result<String> {
    let bought: Value = client.post_json("/api/buy", &json!({"product": PRODUCT})).await?;
    let order = bought["order_id"].as_str().context("buy returned no order id")?.to_owned();
    let url = bought["checkout_url"].as_str().context("buy returned no checkout URL")?;
    let path = url
        .strip_prefix(&format!("http://{fake_stripe}"))
        .with_context(|| format!("the checkout URL {url} is not fake-stripe's"))?;
    let page = send(fake_stripe, request(Method::GET, path, &fake_stripe.to_string()), Bytes::new()).await?;
    ensure!(
        page.status == StatusCode::OK && String::from_utf8_lossy(&page.body).contains("Pay"),
        "the checkout page: {}",
        page.status
    );
    let paid = send(fake_stripe, request(Method::POST, path, &fake_stripe.to_string()), Bytes::new()).await?;
    ensure!(paid.status == StatusCode::SEE_OTHER, "pressing Pay: {}", paid.status);
    let location = paid.header(LOCATION.as_str());
    ensure!(location == console.paid_url, "checkout sent the buyer to {location}, not {}", console.paid_url);
    let back = client.send(Method::GET, "/paid", None, None, &[]).await?;
    ensure!(back.status == StatusCode::OK, "GET /paid: {}", back.status);
    Ok(order)
}

/// Waits for the check the console starts once the manifest is approved:
/// walletd fetching the gate's manifest over the use plane.
async fn use_plane(client: &Client) -> anyhow::Result<()> {
    let deadline = Instant::now() + USE_PLANE_TIMEOUT;
    loop {
        let state: Value = client.get_json("/api/state").await?;
        let check = &state["use_plane_check"];
        if !check.is_null() {
            let text = check["text"].as_str().unwrap_or_default();
            ensure!(check["ok"] == true, "the use-plane check: {text}");
            ok(text);
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "the use-plane check did not finish within {USE_PLANE_TIMEOUT:?}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Waits until the console shows the order collected.
async fn collected(client: &Client, order: &str) -> anyhow::Result<()> {
    let deadline = Instant::now() + COLLECT_TIMEOUT;
    loop {
        let state: Value = client.get_json("/api/state").await?;
        let purchase =
            state["purchases"].as_array().into_iter().flatten().find(|purchase| purchase["order_id"] == order);
        let status = purchase.and_then(|purchase| purchase["status"].as_str()).unwrap_or_default();
        if status == "collected" {
            let chain = purchase.and_then(|purchase| purchase["chain"].as_i64()).unwrap_or_default();
            ok(format!("the collector turned the paid order into chain {chain}: a credential the gate signed blind"));
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "order {order} was not collected within {COLLECT_TIMEOUT:?}: {status}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// One streamed chat turn as the page reads it.
struct Turn {
    text: String,
    receipt: Receipt,
    /// When the first and last pieces of text arrived.
    first_text: Duration,
    last_text: Duration,
    pieces: usize,
}

impl Turn {
    /// Checks the receipt, the price and the streaming; with a real model,
    /// also that the reply says `expected`, the word the prompt asks it to
    /// end on.
    fn check(&self, turn: u32, model: &Model, expected: &str) -> anyhow::Result<()> {
        let receipt = &self.receipt;
        ensure!(receipt.turn == turn && receipt.model == model.id, "turn {turn}'s receipt: {receipt:?}");
        // The gate charges a faulted turn's delivered output only, so the
        // fault is the finding, not the price.
        ensure!(
            receipt.finish != FinishReason::Error,
            "the engine failed mid-reply after {} tokens, which the gate charged alone ({}): {receipt:?}",
            receipt.completion_tokens,
            receipt.charge
        );
        ensure!(receipt.completion_tokens > 0 && !self.text.trim().is_empty(), "an empty reply: {receipt:?}");
        let price = model.input_rate * u64::from(receipt.prompt_tokens)
            + model.output_rate * u64::from(receipt.completion_tokens);
        ensure!(
            receipt.charge == price,
            "charged {} for {receipt:?}, the release's prices make {price}",
            receipt.charge
        );
        // A stream that arrived in one piece was buffered on the way.
        ensure!(self.pieces > 1, "the reply arrived in one piece over {:?}: buffered", self.spread());
        if model.engine != "fake" {
            // Metering counts tokens, not sense: a GPU whose data path is
            // broken streams well-formed garbage that bills like an answer.
            ensure!(
                self.text.to_lowercase().contains(expected),
                "turn {turn}'s reply never says {expected:?}, so the model isn't answering: {:?}",
                self.text.chars().take(120).collect::<String>()
            );
            return Ok(());
        }
        // fake-engine streams one word per token and paces them, so a
        // stream that arrived much faster was buffered on the way too.
        let words = u32::try_from(self.text.split_whitespace().count())?;
        ensure!(words == receipt.completion_tokens, "{words} words streamed, {} receipted", receipt.completion_tokens);
        let paced = Duration::from_millis(TOKEN_DELAY_MS * u64::from(receipt.completion_tokens.saturating_sub(1)));
        ensure!(
            self.spread() >= paced / 4,
            "the reply arrived in {} pieces over {:?}; the engine took {paced:?}: buffered",
            self.pieces,
            self.spread()
        );
        Ok(())
    }

    /// How long the text took to arrive, from its first piece to its last.
    fn spread(&self) -> Duration {
        self.last_text.saturating_sub(self.first_text)
    }

    fn describe(&self) -> String {
        let receipt = &self.receipt;
        format!(
            "turn {}: {} prompt + {} output tokens, charged {} ({} left in the session); first text after {:.2} s, \
             streamed in {} pieces over {:.2} s",
            receipt.turn,
            receipt.prompt_tokens,
            receipt.completion_tokens,
            receipt.charge,
            receipt.remaining,
            self.first_text.as_secs_f64(),
            self.pieces,
            self.spread().as_secs_f64()
        )
    }
}

/// Sends one turn through the proxy and reads the stream as it arrives.
async fn chat(client: &Client, model: &str, conversation: &str, history: &[Value]) -> anyhow::Result<Turn> {
    let body = json!({"model": model, "messages": history, "stream": true});
    let request = client
        .request(Method::POST, "/api/wallet/v1/chat/completions", None, Some(client.token.as_str()))
        .header(CONTENT_TYPE, "application/json")
        .header(CONVERSATION_HEADER, conversation)
        .body(Full::new(Bytes::from(serde_json::to_vec(&body)?)))?;
    let started = Instant::now();
    let stream = TcpStream::connect(client.addr).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let response = sender.send_request(request).await?;
    ensure!(response.status() == StatusCode::OK, "chat through the proxy: {}", response.status());
    let mut body = response.into_body();
    let (mut buffer, mut text, mut receipt) = (String::new(), String::new(), None);
    let (mut first_text, mut last_text, mut pieces) = (None, Duration::ZERO, 0);
    while let Some(frame) = body.frame().await {
        let Ok(data) = frame?.into_data() else { continue };
        buffer.push_str(&String::from_utf8_lossy(&data));
        while let Some((event, rest)) = buffer.split_once("\n\n") {
            let (event, rest) = (event.to_owned(), rest.to_owned());
            buffer = rest;
            let Some(data) = event.lines().find_map(|line| line.strip_prefix("data:")).map(str::trim) else {
                continue;
            };
            if data == "[DONE]" {
                continue;
            }
            let chunk: Value = serde_json::from_str(data).with_context(|| format!("a chunk: {data}"))?;
            if let Some(piece) = chunk["choices"][0]["delta"]["content"].as_str().filter(|piece| !piece.is_empty()) {
                text.push_str(piece);
                first_text.get_or_insert(started.elapsed());
                last_text = started.elapsed();
            }
            if !chunk["receipt"].is_null() {
                receipt = Some(serde_json::from_value(chunk["receipt"].clone())?);
            }
        }
        if first_text.is_some() {
            pieces += 1;
        }
    }
    Ok(Turn {
        text,
        receipt: receipt.context("the turn ended without a receipt")?,
        first_text: first_text.context("no text arrived")?,
        last_text,
        pieces,
    })
}

/// The console, as the page reaches it.
struct Client {
    addr: SocketAddr,
    token: String,
}

impl Client {
    /// A request to the console naming `host` (its own by default) with `token`.
    fn request(&self, method: Method, path: &str, host: Option<&str>, token: Option<&str>) -> http::request::Builder {
        let mut builder = request(method, path, host.unwrap_or(&self.addr.to_string()));
        if let Some(token) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        builder
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        host: Option<&str>,
        token: Option<&str>,
        body: &[u8],
    ) -> anyhow::Result<Reply> {
        send(self.addr, self.request(method, path, host, token), Bytes::copy_from_slice(body)).await
    }

    async fn send_json(&self, method: Method, path: &str, body: &Value) -> anyhow::Result<Reply> {
        let builder = self.request(method, path, None, Some(&self.token)).header(CONTENT_TYPE, "application/json");
        send(self.addr, builder, Bytes::from(serde_json::to_vec(body)?)).await
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        self.send(Method::GET, path, None, Some(&self.token), &[]).await?.json(path)
    }

    async fn post_json<T: DeserializeOwned>(&self, path: &str, body: &Value) -> anyhow::Result<T> {
        self.send_json(Method::POST, path, body).await?.json(path)
    }
}

fn request(method: Method, path: &str, host: &str) -> http::request::Builder {
    http::Request::builder().method(method).uri(path).header(HOST, host)
}

/// One whole response.
struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Reply {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default()
    }

    fn json<T: DeserializeOwned>(&self, what: &str) -> anyhow::Result<T> {
        ensure!(self.status.is_success(), "{what}: {} {}", self.status, String::from_utf8_lossy(&self.body));
        serde_json::from_slice(&self.body).with_context(|| format!("decoding {what}"))
    }
}

async fn send(addr: SocketAddr, builder: http::request::Builder, body: Bytes) -> anyhow::Result<Reply> {
    let request = builder.body(Full::new(body))?;
    let stream = TcpStream::connect(addr).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let response = sender.send_request(request).await?;
    let (parts, body) = response.into_parts();
    Ok(Reply { status: parts.status, headers: parts.headers, body: body.collect().await?.to_bytes() })
}
