//! Running the binary, and what it runs against: [`Shop`], a storefront
//! on fake-stripe whose issuance a stub gate answers with a real credit
//! issuer, and [`StubWalletd`], canned walletd answers on loopback.

use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use attest::{DevAttestationKey, DevInsecureSource, DevInsecureVerifier, Role};
use axum::Json;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use bytes::Bytes;
use credit::{Moderator, SecretSeed};
use fake_stripe::FakeStripe;
use gate_conn::server::{ServeOptions, serve_unix};
use gate_conn::{ApiRequest, DialScope, Target, TcpDialer, UnverifiedConn};
use http_body_util::Empty;
use hyper_util::rt::TokioIo;
use manifest::{
    AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, OperatorSecretKey, Release, ReleaseConfig, Service,
};
use proto::gate::{LocalIssueRequest, LocalIssueResponse};
use proto::openai::{ChatCompletionChunk, ChatCompletionRequest, ModelList};
use proto::sse::{OPENAI_DONE, SseEvent};
use proto::storefront::{ConfigureRequest, ConfigureResponse, StorefrontState};
use proto::walletd::{
    BalanceView, ChainState, ChainView, LocalErrorBody, PendingOrderView, RECOVERED_HEADER, ROLLOVER_HEADER,
    ReconcileReport, SettledSessionView, SettlementOutcome, TokenEstimate, WalletCloseResponse, WithReceipt,
};
use proto::{Blob, ErrorBody, ErrorCode, FinishReason, Receipt};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use storefront::Storefront;
use storefront::config::Settings;
use storefront::stripe::StripeEndpoint;
use tokio::io::AsyncWriteExt as _;
use tokio::net::{TcpListener, TcpStream, UnixListener};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wallet_core::Pins;
use wallet_core::pins::PINS_FILE;

pub const SERVICE_ID: &str = "counteractive-cli-test";
/// Credits in the one product, `usd-5`.
pub const USD_5: u64 = 5_000_000;
const OPERATOR_TOKEN: [u8; 32] = [0x70; 32];
const STRIPE_KEY: &str = "sk_test_fake";
const SUCCESS_URL: &str = "http://127.0.0.1:7777/paid";
/// Long enough for any one run of the binary; a hang fails instead.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What one run of `wallet-cli` did.
#[derive(Debug)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// `wallet-cli`, with the wallet directory from neither the environment
/// nor the default: each caller says where.
pub fn command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_wallet-cli"));
    command.env_remove("COUNTERACTIVE_WALLET_DIR").env_remove("RUST_LOG").kill_on_drop(true);
    command
}

/// Runs `command`, feeding it `stdin`.
pub async fn run(mut command: tokio::process::Command, stdin: Option<&str>) -> Output {
    command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("wallet-cli starts");
    if let Some(text) = stdin {
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(text.as_bytes()).await.unwrap();
    }
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output()).await.expect("wallet-cli finishes").unwrap();
    Output {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `wallet-cli --wallet-dir <dir> <args>`.
pub async fn wallet_cli(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = command();
    command.arg("--wallet-dir").arg(dir).args(args);
    run(command, stdin).await
}

/// Pins for a wallet of this service, with a fresh operator key unless
/// one is given.
pub fn pins(storefront: SocketAddr, operator: Option<&OperatorSecretKey>) -> Pins {
    let generated;
    let operator = if let Some(operator) = operator {
        operator
    } else {
        generated = OperatorSecretKey::generate().unwrap();
        &generated
    };
    Pins {
        service_id: SERVICE_ID.into(),
        key_epoch: 1,
        gate_target: "unix:/nonexistent/gate.sock".into(),
        storefront_addr: storefront.to_string(),
        operator_public_key: operator.public_key().unwrap().as_base64().to_owned(),
        tier_ladder: vec![500_000, 50_000, 5_000],
        allow_dev_attestation: true,
    }
}

/// Writes `wallet.toml` into `dir`.
pub fn write_pins(dir: &Path, pins: &Pins) {
    std::fs::write(dir.join(PINS_FILE), pins.to_toml().unwrap()).unwrap();
}

/// A loopback address nothing listens on.
pub async fn closed_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

/// A storefront on fake-stripe, configured and serving a signed manifest,
/// whose `issue.sock` is a stub gate issuing real credentials; and a
/// wallet directory whose pins point at it.
pub struct Shop {
    /// The wallet directory, with `wallet.toml` written.
    pub wallet: tempfile::TempDir,
    /// The pins in it.
    pub pins: Pins,
    stripe: FakeStripe,
    corrupt: Arc<AtomicBool>,
    shutdown: CancellationToken,
    server: JoinHandle<()>,
    issue_task: JoinHandle<()>,
    // Dropped last: it holds issue.sock and the manifest files.
    _dir: tempfile::TempDir,
}

impl Shop {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let operator = OperatorSecretKey::generate().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let pins = pins(addr, Some(&operator));

        let moderator =
            Moderator::from_seed(&SecretSeed::generate().unwrap(), &pins.service_context().unwrap()).unwrap();
        let issuer_public_key = moderator.public_key().to_bytes().to_vec();
        let corrupt = Arc::new(AtomicBool::new(false));
        let shutdown = CancellationToken::new();
        let issue_socket = dir.path().join("issue.sock");
        let issuer = Issuer { moderator: Arc::new(moderator), corrupt: Arc::clone(&corrupt) };
        let router = axum::Router::new().route("/local/issue", axum::routing::post(issue)).with_state(issuer);
        let issue_listener = UnixListener::bind(&issue_socket).unwrap();
        let issue_task =
            tokio::spawn(serve_unix(issue_listener, None, router, ServeOptions::default(), shutdown.clone()));

        let config = json!({
            "schema": 1, "image_version": "0.1.0-test", "service_id": SERVICE_ID, "key_epoch": 1,
            "balance_width": 24, "operator_public_key": pins.operator_public_key, "k_sha256": "00".repeat(32),
            "products": [{"id": "usd-5", "usd_cents": 500, "credits": USD_5}],
            "storefront": {"port": 8443, "operator_token_sha256": hex::encode(Sha256::digest(OPERATOR_TOKEN))},
            "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64, "max_age_seconds": 3600},
            "catalogue": [{"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                           "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096}]
        });
        let dev_key = DevAttestationKey::generate().unwrap();
        let manifest = manifest(&config, dev_key.public_key(), issuer_public_key);
        let signed = manifest::sign_manifest(&manifest, &operator).unwrap();
        let manifest_dir = dir.path().join("manifest");
        std::fs::create_dir(&manifest_dir).unwrap();
        std::fs::write(manifest_dir.join("manifest.json"), signed.manifest.as_bytes()).unwrap();
        std::fs::write(manifest_dir.join("manifest.sig"), &signed.signature).unwrap();

        let settings = Settings {
            release: ReleaseConfig::from_value(&config).unwrap(),
            attestation: Box::new(DevInsecureSource::new(dev_key, manifest::release_config_sha256(&config))),
            issue_socket,
            stripe: StripeEndpoint::new(stripe.base_url().parse().unwrap(), SUCCESS_URL.parse().unwrap()).unwrap(),
            manifest_dir,
        };
        let server = tokio::spawn(Storefront::new(settings).unwrap().serve(listener, shutdown.clone()));
        configure(addr, &manifest).await;

        let wallet = tempfile::tempdir().unwrap();
        write_pins(wallet.path(), &pins);
        Self { wallet, pins, stripe, corrupt, shutdown, server, issue_task, _dir: dir }
    }

    /// The wallet directory.
    pub fn dir(&self) -> &Path {
        self.wallet.path()
    }

    /// What a buyer pressing Pay does: fake-stripe's `POST /test/pay/{id}`.
    pub async fn pay(&self, order_id: &str) {
        let base: hyper::Uri = self.stripe.base_url().parse().unwrap();
        let authority = base.authority().unwrap().as_str().to_owned();
        let stream = TcpStream::connect(&authority).await.unwrap();
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
        tokio::spawn(connection);
        let request = hyper::Request::post(format!("/test/pay/{order_id}"))
            .header(hyper::header::HOST, authority)
            .body(Empty::<Bytes>::new())
            .unwrap();
        assert_eq!(sender.send_request(request).await.unwrap().status(), 200, "fake-stripe took the payment");
    }

    /// From now on the stub gate sends issue responses that don't
    /// finalise, as a faulty or hostile issuer would.
    pub fn corrupt_issuance(&self) {
        self.corrupt.store(true, Ordering::SeqCst);
    }

    pub async fn stop(self) {
        self.shutdown.cancel();
        tokio::time::timeout(TIMEOUT, self.server).await.unwrap().unwrap();
        tokio::time::timeout(TIMEOUT, self.issue_task).await.unwrap().unwrap();
        self.stripe.stop().await.unwrap();
    }
}

/// The manifest: one dev-insecure release with `config`.
fn manifest(config: &Value, dev_public_key: [u8; 32], issuer_public_key: Vec<u8>) -> Manifest {
    Manifest {
        schema: 1,
        version: 1,
        service: Service {
            id: SERVICE_ID.into(),
            onion: None,
            credit_scheme: CREDIT_SCHEME.into(),
            issuer_public_key: Blob::new(issuer_public_key),
            key_epoch: 1,
            balance_width: BALANCE_WIDTH,
        },
        releases: vec![Release {
            image_version: "0.1.0-test".into(),
            platform: "dev-insecure".into(),
            attestation_policy: AttestationPolicy {
                kind: "dev-insecure".into(),
                dev_public_key: Some(Blob::new(dev_public_key.to_vec())),
            },
            release_config_sha256: manifest::release_config_sha256(config),
            release_config: config.clone(),
        }],
    }
}

/// Gives the storefront its Stripe key, as the operator does.
async fn configure(addr: SocketAddr, manifest: &Manifest) {
    let target = Target::Tcp { host: addr.ip().to_string(), port: addr.port() };
    let conn = UnverifiedConn::connect(&TcpDialer, &target, Role::Storefront, &DialScope::new()).await.unwrap();
    let mut conn = conn.attest(&DevInsecureVerifier, manifest).await.unwrap();
    let request = ApiRequest::post("/v1/operator/configure")
        .json(&ConfigureRequest { stripe_key: SecretString::from(STRIPE_KEY) })
        .unwrap()
        .bearer(SecretString::from(hex::encode(OPERATOR_TOKEN)));
    let configured: ConfigureResponse = conn.send_json(request).await.unwrap();
    assert_eq!(configured.state, StorefrontState::Ready);
}

/// The stub gate's issuer.
#[derive(Clone)]
struct Issuer {
    moderator: Arc<Moderator>,
    corrupt: Arc<AtomicBool>,
}

/// `POST /local/issue`: a real issue response, or a corrupted one.
async fn issue(State(issuer): State<Issuer>, Json(request): Json<LocalIssueRequest>) -> Response {
    let Ok(mut response) = issuer.moderator.issue(request.credits, request.issue_request.as_bytes()) else {
        return (StatusCode::BAD_REQUEST, Json(ErrorBody::new(ErrorCode::BadRequest))).into_response();
    };
    if issuer.corrupt.load(Ordering::SeqCst) {
        let last = response.len() - 1;
        response[last] ^= 0x01;
    }
    Json(LocalIssueResponse { issue_response: Blob::new(response) }).into_response()
}

/// One request the stub walletd received.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// walletd's local API with canned answers, recording every request. A
/// chat whose prompt mentions "notices" carries the rollover and recovered
/// headers; one that mentions "too large" gets `402 prompt_exceeds_budget`.
pub struct StubWalletd {
    /// Where it listens, as written to `walletd.addr`.
    pub addr: SocketAddr,
    /// The bearer token, as written to `walletd.token`.
    pub token: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    shutdown: CancellationToken,
    task: JoinHandle<()>,
}

impl StubWalletd {
    /// Serves on a free loopback port and writes `walletd.addr` and
    /// `walletd.token` into `wallet_dir`, as walletd does.
    pub async fn start(wallet_dir: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let token = hex::encode([0x5a; 32]);
        std::fs::write(wallet_dir.join("walletd.addr"), addr.to_string()).unwrap();
        std::fs::write(wallet_dir.join("walletd.token"), format!("{token}\n")).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = axum::Router::new().fallback(stub).with_state(Arc::clone(&seen));
        let shutdown = CancellationToken::new();
        let stop = shutdown.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).with_graceful_shutdown(stop.cancelled_owned()).await.unwrap();
        });
        Self { addr, token, seen, shutdown, task }
    }

    /// Every request so far.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    pub async fn stop(self) {
        self.shutdown.cancel();
        tokio::time::timeout(TIMEOUT, self.task).await.unwrap().unwrap();
    }
}

async fn stub(State(seen): State<Arc<Mutex<Vec<Seen>>>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, 1 << 20).await.unwrap();
    let path = parts.uri.path().to_owned();
    seen.lock().unwrap().push(Seen {
        method: parts.method.to_string(),
        path: path.clone(),
        headers: parts.headers,
        body: body.clone(),
    });
    match (parts.method.as_str(), path.as_str()) {
        ("GET", "/wallet/balance") => Json(balance_view()).into_response(),
        ("GET", "/v1/models") => Json(model_list()).into_response(),
        ("POST", "/wallet/close") => Json(close_response()).into_response(),
        ("POST", "/wallet/reconcile") => Json(reconcile_report()).into_response(),
        ("POST", "/v1/chat/completions") => chat(&body),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A streamed turn as walletd sends it: content chunks, a last chunk
/// carrying the receipt, then `[DONE]`.
fn chat(body: &[u8]) -> Response {
    let request: ChatCompletionRequest = serde_json::from_slice(body).unwrap();
    let prompt = &request.messages[0].content;
    if prompt.contains("too large") {
        let error = LocalErrorBody { error: "prompt_exceeds_budget".into(), message: None, required: Some(520_028) };
        return (StatusCode::PAYMENT_REQUIRED, Json(error)).into_response();
    }
    let chunk = |text: &str, receipt: Option<Receipt>| {
        let chunk = WithReceipt { body: ChatCompletionChunk::content(text), receipt };
        SseEvent::data(serde_json::to_string(&chunk).unwrap()).encode()
    };
    let frames =
        [chunk("hi", None), chunk(" there", None), chunk("", Some(receipt())), SseEvent::data(OPENAI_DONE).encode()];
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    if prompt.contains("notices") {
        headers.insert(ROLLOVER_HEADER, HeaderValue::from_static("session_exhausted"));
        headers.insert(RECOVERED_HEADER, HeaderValue::from_static("full_refund"));
    }
    (headers, frames.concat()).into_response()
}

/// The stub's receipt for every turn.
pub fn receipt() -> Receipt {
    Receipt {
        turn: 2,
        model: "fake-small".into(),
        prompt_tokens: 9,
        completion_tokens: 21,
        charge: 72,
        remaining: 499_928,
        finish: FinishReason::Stop,
    }
}

/// The stub's balance, with every field set.
pub fn balance_view() -> BalanceView {
    BalanceView {
        total_credits: 4_999_928,
        total_usd: "4.999928".into(),
        estimated_tokens_remaining: [("fake-small".to_owned(), TokenEstimate { output_tokens_at_most: 1_666_642 })]
            .into(),
        chains: vec![ChainView { id: 1, state: ChainState::Ready, credits: 4_999_928, resolved: true }],
        orders_awaiting_payment: vec![PendingOrderView { product: "usd-5".into(), credits: USD_5 }],
        frozen_orders: 1,
        frozen_chains: 0,
        dust_credits: 3,
        experimental_platform: false,
        manifest_version: Some(1),
    }
}

/// The stub's model list.
pub fn model_list() -> ModelList {
    ModelList::new(["fake-large", "fake-small"], "counteractive")
}

/// The stub's close answer.
pub fn close_response() -> WalletCloseResponse {
    WalletCloseResponse { closed: 2, recovered: 1 }
}

/// The stub's reconcile report.
pub fn reconcile_report() -> ReconcileReport {
    let session = |spend_seq, t, outcome| SettledSessionView {
        chain_id: 1,
        spend_seq,
        s: 500_000,
        t,
        receipts_sum: 500_000 - t,
        outcome,
    };
    ReconcileReport {
        recovered: 1,
        sessions: vec![session(1, 499_928, SettlementOutcome::Exact), session(2, 500_000, SettlementOutcome::Exact)],
        frozen_chains: 0,
    }
}
