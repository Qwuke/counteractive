//! The storefront end to end over inner TLS on an ephemeral TCP port, with
//! `fake-stripe` as Stripe and a stub gate on a temp `issue.sock`.
//!
//! The client side is what `wallet-cli` and `xtask` use: `TcpDialer`, then
//! dev-insecure attestation expecting role `storefront`.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a setup step that fails, or an unexpected answer, should fail the test at once"
)]

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use attest::{DevAttestationKey, DevInsecureSource, DevInsecureVerifier, Rejection, Role};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use axum::routing;
use fake_stripe::FakeStripe;
use gate_conn::server::{ServeOptions, serve_unix};
use gate_conn::{ApiRequest, AttestFailure, AttestedConn, ConnError, DialScope, Target, TcpDialer, UnverifiedConn};
use manifest::{AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, Product, Release, ReleaseConfig, Service};
use proto::gate::{LocalIssueRequest, LocalIssueResponse};
use proto::storefront::{
    CollectRequest, CollectResponse, ConfigureRequest, ConfigureResponse, CreateOrderRequest, CreateOrderResponse,
    StorefrontHealth, StorefrontState,
};
use proto::{Blob, ErrorBody, ErrorCode, SecretHex32};
use secrecy::SecretString;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use storefront::Storefront;
use storefront::config::Settings;
use storefront::stripe::{PaymentProvider as _, StripeClient, StripeEndpoint};
use tokio::net::{TcpListener, UnixListener};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const SERVICE_ID: &str = "counteractive-poc";
const OPERATOR_TOKEN: [u8; 32] = [0x70; 32];
const STRIPE_KEY: &str = "sk_test_fake";
const SUCCESS_URL: &str = "http://127.0.0.1:7777/paid";
const TIMEOUT: Duration = Duration::from_secs(20);

fn release_value() -> Value {
    json!({
        "schema": 1, "image_version": "0.1.0", "service_id": SERVICE_ID, "key_epoch": 1,
        "balance_width": 24, "operator_public_key": "RWQ", "k_sha256": "00".repeat(32),
        "products": [{"id": "usd-5", "usd_cents": 500, "credits": 5_000_000},
                     {"id": "usd-1", "usd_cents": 100, "credits": 1_000_000}],
        "storefront": {"port": 8443, "operator_token_sha256": hex::encode(Sha256::digest(OPERATOR_TOKEN))},
        "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64, "max_age_seconds": 3600},
        "catalogue": [{"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                       "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096}]
    })
}

/// A manifest with one dev release for `config`, attested by `dev_public_key`.
fn manifest_for(config: Value, dev_public_key: [u8; 32]) -> Manifest {
    Manifest {
        schema: 1,
        version: 1,
        service: Service {
            id: SERVICE_ID.into(),
            onion: None,
            credit_scheme: CREDIT_SCHEME.into(),
            issuer_public_key: Blob::new(vec![2; 33]),
            key_epoch: 1,
            balance_width: BALANCE_WIDTH,
        },
        releases: vec![Release {
            image_version: "0.1.0".into(),
            platform: "dev-insecure".into(),
            attestation_policy: AttestationPolicy {
                kind: "dev-insecure".into(),
                dev_public_key: Some(Blob::new(dev_public_key.to_vec())),
            },
            release_config_sha256: manifest::release_config_sha256(&config),
            release_config: config,
        }],
    }
}

/// A gate's `issue.sock` with canned behaviour: the first request for an
/// order is answered and remembered, the same request gets the same bytes,
/// another gets `409 order_issued`, and an empty one `400`. While `locked`,
/// everything gets `503 locked`.
#[derive(Default)]
struct StubGate {
    /// The first issue request answered for each order ref.
    issued: Mutex<HashMap<[u8; 32], Vec<u8>>>,
    calls: Mutex<Vec<LocalIssueRequest>>,
    locked: AtomicBool,
}

impl StubGate {
    fn calls(&self) -> Vec<LocalIssueRequest> {
        self.calls.lock().unwrap().clone()
    }
}

async fn stub_issue(State(gate): State<Arc<StubGate>>, Json(request): Json<LocalIssueRequest>) -> Response {
    gate.calls.lock().unwrap().push(request.clone());
    let refuse = |status: StatusCode, code| (status, Json(ErrorBody::new(code))).into_response();
    if gate.locked.load(Ordering::SeqCst) {
        return refuse(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Locked);
    }
    if request.issue_request.is_empty() {
        return refuse(StatusCode::BAD_REQUEST, ErrorCode::BadRequest);
    }
    let bytes = request.issue_request.into_vec();
    match gate.issued.lock().unwrap().entry(request.order_ref) {
        Entry::Occupied(first) if *first.get() != bytes => {
            return refuse(StatusCode::CONFLICT, ErrorCode::OrderIssued);
        }
        Entry::Occupied(_) => {}
        Entry::Vacant(entry) => {
            entry.insert(bytes.clone());
        }
    }
    Json(LocalIssueResponse { issue_response: Blob::new(canned_response(&bytes)) }).into_response()
}

/// Serves `gate` as plain HTTP on a Unix socket at `path`, as the gate serves `issue.sock`.
fn serve_stub_gate(gate: &Arc<StubGate>, path: &Path) -> (CancellationToken, JoinHandle<()>) {
    let shutdown = CancellationToken::new();
    let router = axum::Router::new().route("/local/issue", routing::post(stub_issue)).with_state(Arc::clone(gate));
    let listener = UnixListener::bind(path).unwrap();
    let task = tokio::spawn(serve_unix(listener, None, router, ServeOptions::default(), shutdown.clone()));
    (shutdown, task)
}

/// The stub's issue response: a function of the request, so a repeat gets identical bytes.
fn canned_response(issue_request: &[u8]) -> Vec<u8> {
    [b"issued:".as_slice(), issue_request].concat()
}

/// fake-stripe, a stub gate and a storefront, all on this machine.
struct World {
    dir: tempfile::TempDir,
    stripe: FakeStripe,
    gate: Arc<StubGate>,
    gate_shutdown: CancellationToken,
    gate_task: JoinHandle<()>,
    storefront_spki: [u8; 32],
    target: Target,
    manifest: Manifest,
    shutdown: CancellationToken,
    server: JoinHandle<()>,
}

impl World {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();

        let gate = Arc::new(StubGate::default());
        let issue_socket = dir.path().join("issue.sock");
        let (gate_shutdown, gate_task) = serve_stub_gate(&gate, &issue_socket);

        let config = release_value();
        let dev_key = DevAttestationKey::generate().unwrap();
        let manifest = manifest_for(config.clone(), dev_key.public_key());
        let manifest_dir = dir.path().join("manifest");
        std::fs::create_dir(&manifest_dir).unwrap();
        let settings = Settings {
            release: ReleaseConfig::from_value(&config).unwrap(),
            attestation: Box::new(DevInsecureSource::new(dev_key, manifest::release_config_sha256(&config))),
            issue_socket,
            stripe: endpoint(&stripe),
            manifest_dir,
        };
        let storefront = Storefront::new(settings).unwrap();
        let storefront_spki = storefront.spki_sha256();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(addr) = listener.local_addr().unwrap() else { unreachable!("bound to IPv4") };
        let target = Target::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let shutdown = CancellationToken::new();
        let server = tokio::spawn(storefront.serve(listener, shutdown.clone()));

        Self { dir, stripe, gate, gate_shutdown, gate_task, storefront_spki, target, manifest, shutdown, server }
    }

    async fn connect(&self, role: Role) -> UnverifiedConn {
        UnverifiedConn::connect(&TcpDialer, &self.target, role, &DialScope::new()).await.unwrap()
    }

    async fn attested(&self) -> AttestedConn {
        self.connect(Role::Storefront).await.attest(&DevInsecureVerifier, &self.manifest).await.unwrap()
    }

    /// A storefront configured with a Stripe key, and a connection to it.
    async fn ready() -> (Self, AttestedConn) {
        let world = Self::start().await;
        let mut conn = world.attested().await;
        let configured: ConfigureResponse = post("/v1/operator/configure", configure_body(STRIPE_KEY))
            .bearer(operator_bearer(&OPERATOR_TOKEN))
            .send(&mut conn)
            .await
            .unwrap();
        assert_eq!(configured.state, StorefrontState::Ready);
        (world, conn)
    }

    /// A Stripe client of the test's own, for sessions the storefront didn't make.
    fn stripe_client(&self) -> StripeClient {
        StripeClient::new(endpoint(&self.stripe), SecretString::from(STRIPE_KEY)).unwrap()
    }

    /// What a buyer pressing Pay does.
    async fn pay(&self, order_id: &str) {
        let url = format!("{}/test/pay/{order_id}", self.stripe.base_url());
        let response = reqwest::Client::new().post(url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn stop(self) {
        self.shutdown.cancel();
        self.gate_shutdown.cancel();
        tokio::time::timeout(TIMEOUT, self.server).await.unwrap().unwrap();
        tokio::time::timeout(TIMEOUT, self.gate_task).await.unwrap().unwrap();
        self.stripe.stop().await.unwrap();
    }
}

fn endpoint(stripe: &FakeStripe) -> StripeEndpoint {
    StripeEndpoint::new(stripe.base_url().parse().unwrap(), SUCCESS_URL.parse().unwrap()).unwrap()
}

fn operator_bearer(token: &[u8]) -> SecretString {
    SecretString::from(hex::encode(token))
}

fn configure_body(key: &str) -> ConfigureRequest {
    ConfigureRequest { stripe_key: SecretString::from(key) }
}

/// A request in the making, so a test reads as method, path, body, then send.
struct Call(ApiRequest);

fn post(path: impl Into<String>, body: impl Serialize) -> Call {
    Call(ApiRequest::post(path).json(&body).unwrap())
}

fn get(path: impl Into<String>) -> Call {
    Call(ApiRequest::get(path))
}

impl Call {
    fn bearer(self, token: SecretString) -> Self {
        Self(self.0.bearer(token))
    }

    async fn send<T: DeserializeOwned>(self, conn: &mut AttestedConn) -> Result<T, ConnError> {
        tokio::time::timeout(TIMEOUT, conn.send_json(self.0)).await.expect("request timed out")
    }
}

/// The status and code of a refused request.
fn refusal<T: std::fmt::Debug>(result: Result<T, ConnError>) -> (u16, ErrorCode) {
    match result {
        Err(ConnError::Api { status, error: Some(body) }) => (status, body.error),
        other => panic!("expected an API error, got {other:?}"),
    }
}

async fn create_order(conn: &mut AttestedConn, product: &str, secret: [u8; 32]) -> CreateOrderResponse {
    let request = CreateOrderRequest { product: product.into(), secret_hash: Sha256::digest(secret).into() };
    post("/v1/orders", request).send(conn).await.unwrap()
}

async fn collect(
    conn: &mut AttestedConn,
    order_id: &str,
    secret: [u8; 32],
    issue_request: &[u8],
) -> Result<CollectResponse, ConnError> {
    let request =
        CollectRequest { order_secret: SecretHex32::new(secret), issue_request: Blob::new(issue_request.to_vec()) };
    post(format!("/v1/orders/{order_id}/collect"), request).send(conn).await
}

#[tokio::test]
async fn attests_over_tcp_with_role_storefront() {
    let world = World::start().await;

    let mut conn = world.connect(Role::Storefront).await;
    assert_eq!(conn.spki_sha256(), world.storefront_spki, "SPKI recorded from the handshake");
    let health: StorefrontHealth = conn.health().await.unwrap();
    assert_eq!(health.state, StorefrontState::Unconfigured);
    let none = conn.fetch_manifest().await;
    assert_eq!(refusal(none), (503, ErrorCode::NoManifest));

    // The operator writes the signed manifest after the gate's first unlock;
    // it's served from then on, in canonical form.
    let manifest_dir = world.dir.path().join("manifest");
    let pretty = serde_json::to_vec_pretty(&world.manifest).unwrap();
    std::fs::write(manifest_dir.join("manifest.json"), pretty).unwrap();
    std::fs::write(manifest_dir.join("manifest.sig"), "untrusted comment: test\nRWQ\n").unwrap();
    let served = conn.fetch_manifest().await.unwrap();
    assert_eq!(served.manifest.as_bytes(), manifest::to_canonical_json(&world.manifest).unwrap());
    assert_eq!(served.signature, "untrusted comment: test\nRWQ\n");

    let attested = conn.attest(&DevInsecureVerifier, &world.manifest).await.unwrap();
    let verdict = attested.verdict();
    assert_eq!(verdict.role, Role::Storefront);
    assert_eq!(verdict.tls_spki_sha256, world.storefront_spki);
    assert_eq!(verdict.release_config.product("usd-5").unwrap().usd_cents, 500);
    drop(attested);

    // The storefront's evidence can't stand in for the gate's.
    let as_gate = world.connect(Role::Gate).await.attest(&DevInsecureVerifier, &world.manifest).await;
    assert!(
        matches!(
            as_gate,
            Err(AttestFailure::Rejected(Rejection::WrongRole { expected: Role::Gate, claimed: Role::Storefront }))
        ),
        "{as_gate:?}"
    );

    world.stop().await;
}

#[tokio::test]
async fn takes_orders_only_once_the_operator_configures_it() {
    let world = World::start().await;
    let mut conn = world.attested().await;

    let order = CreateOrderRequest { product: "usd-5".into(), secret_hash: [1; 32] };
    assert_eq!(refusal(post("/v1/orders", &order).send::<Value>(&mut conn).await), (503, ErrorCode::Unconfigured));
    let early_collect = collect(&mut conn, "cs_test_1", [1; 32], b"request").await;
    assert_eq!(refusal(early_collect), (503, ErrorCode::Unconfigured));

    let configure = |bearer: Option<SecretString>, key: &str| {
        let call = post("/v1/operator/configure", configure_body(key));
        match bearer {
            Some(token) => call.bearer(token),
            None => call,
        }
    };
    for bearer in
        [Some(operator_bearer(&[0x71; 32])), Some(operator_bearer(&[])), Some(SecretString::from("not-hex")), None]
    {
        let refused = configure(bearer, STRIPE_KEY).send::<Value>(&mut conn).await;
        assert_eq!(refusal(refused), (401, ErrorCode::Unauthorized));
    }
    // A wrong token is refused before its body is even read.
    let malformed = post("/v1/operator/configure", json!({"nope": 1}))
        .bearer(operator_bearer(&[0x71; 32]))
        .send::<Value>(&mut conn)
        .await;
    assert_eq!(refusal(malformed), (401, ErrorCode::Unauthorized));
    let health: StorefrontHealth = get("/v1/health").send(&mut conn).await.unwrap();
    assert_eq!(health.state, StorefrontState::Unconfigured);

    let empty_key = configure(Some(operator_bearer(&OPERATOR_TOKEN)), "").send::<Value>(&mut conn).await;
    assert_eq!(refusal(empty_key), (400, ErrorCode::BadRequest));

    let configured: ConfigureResponse =
        configure(Some(operator_bearer(&OPERATOR_TOKEN)), STRIPE_KEY).send(&mut conn).await.unwrap();
    assert_eq!(configured.state, StorefrontState::Ready);
    let health: StorefrontHealth = get("/v1/health").send(&mut conn).await.unwrap();
    assert_eq!(health.state, StorefrontState::Ready);

    world.stop().await;
}

#[tokio::test]
async fn collect_issues_once_stripe_says_paid() {
    let (world, mut conn) = World::ready().await;
    let secret = [0x42; 32];

    let unknown_product = CreateOrderRequest { product: "usd-9".into(), secret_hash: [1; 32] };
    let refused = post("/v1/orders", unknown_product).send::<Value>(&mut conn).await;
    assert_eq!(refusal(refused), (400, ErrorCode::BadRequest));

    let order = create_order(&mut conn, "usd-5", secret).await;
    assert!(order.order_id.starts_with("cs_test_"));
    assert_eq!(order.checkout_url, format!("{}/pay/{}", world.stripe.base_url(), order.order_id));

    let unpaid = collect(&mut conn, &order.order_id, secret, b"request-a").await;
    assert_eq!(refusal(unpaid), (402, ErrorCode::Unpaid));
    assert!(world.gate.calls().is_empty(), "nothing reaches the gate before payment");

    world.pay(&order.order_id).await;
    let collected = collect(&mut conn, &order.order_id, secret, b"request-a").await.unwrap();
    assert_eq!(collected.credits, 5_000_000);
    assert_eq!(collected.issue_response.as_bytes(), canned_response(b"request-a"));
    let calls = world.gate.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].order_ref, <[u8; 32]>::from(Sha256::digest(order.order_id.as_bytes())));
    assert_eq!(calls[0].credits, 5_000_000);
    assert_eq!(calls[0].issue_request.as_bytes(), b"request-a");

    // A retry gets the same bytes; another request for the order is refused.
    let again = collect(&mut conn, &order.order_id, secret, b"request-a").await.unwrap();
    assert_eq!(again, collected);
    let redirected = collect(&mut conn, &order.order_id, secret, b"request-b").await;
    assert_eq!(refusal(redirected), (409, ErrorCode::OrderIssued));

    let wrong_secret = collect(&mut conn, &order.order_id, [0x43; 32], b"request-a").await;
    assert_eq!(refusal(wrong_secret), (401, ErrorCode::Unauthorized));
    let unknown = collect(&mut conn, "cs_test_unknown", secret, b"request-a").await;
    assert_eq!(refusal(unknown), (404, ErrorCode::NotFound));
    let malformed_id = collect(&mut conn, "cs-not-an-id", secret, b"request-a").await;
    assert_eq!(refusal(malformed_id), (404, ErrorCode::NotFound));
    let bad_body = post(format!("/v1/orders/{}/collect", order.order_id), json!({"order_secret": "zz"}))
        .send::<Value>(&mut conn)
        .await;
    assert_eq!(refusal(bad_body), (400, ErrorCode::BadRequest));
    assert_eq!(world.gate.calls().len(), 3, "refused collects never reach the gate");

    drop(conn);
    world.stop().await;
}

#[tokio::test]
async fn collect_refuses_a_session_whose_amount_is_not_the_products() {
    let (world, mut conn) = World::ready().await;
    let secret = [0x42; 32];

    // A session made at Stripe directly, for usd-5 at a cent less.
    let underpriced = Product { id: "usd-5".into(), usd_cents: 499, credits: 5_000_000 };
    let reference = hex::encode(Sha256::digest(secret));
    let checkout = world.stripe_client().create_checkout(&underpriced, &reference).await.unwrap();
    world.pay(checkout.id.as_str()).await;

    let mismatched = collect(&mut conn, checkout.id.as_str(), secret, b"request-a").await;
    assert_eq!(refusal(mismatched), (409, ErrorCode::AmountMismatch));
    assert!(world.gate.calls().is_empty(), "nothing is issued for a mismatched amount");

    drop(conn);
    world.stop().await;
}

#[tokio::test]
async fn gate_refusals_reach_the_wallet() {
    let (world, mut conn) = World::ready().await;
    let secret = [0x42; 32];
    let order = create_order(&mut conn, "usd-1", secret).await;
    world.pay(&order.order_id).await;

    let rejected = collect(&mut conn, &order.order_id, secret, b"").await;
    assert_eq!(refusal(rejected), (400, ErrorCode::BadRequest));

    world.gate.locked.store(true, Ordering::SeqCst);
    let locked = collect(&mut conn, &order.order_id, secret, b"request-a").await;
    assert_eq!(refusal(locked), (503, ErrorCode::Locked));

    // A gate that's down reads the same to the wallet: retry later.
    world.gate_shutdown.cancel();
    std::fs::remove_file(world.dir.path().join("issue.sock")).unwrap();
    let down = collect(&mut conn, &order.order_id, secret, b"request-a").await;
    assert_eq!(refusal(down), (503, ErrorCode::Locked));

    // The paid order is still collectable once the gate is back.
    world.gate.locked.store(false, Ordering::SeqCst);
    let (restarted, task) = serve_stub_gate(&world.gate, &world.dir.path().join("issue.sock"));
    let collected = collect(&mut conn, &order.order_id, secret, b"request-a").await.unwrap();
    assert_eq!(collected.credits, 1_000_000);
    restarted.cancel();
    tokio::time::timeout(TIMEOUT, task).await.unwrap().unwrap();

    drop(conn);
    world.stop().await;
}

#[tokio::test]
async fn unknown_routes_and_oversized_bodies_get_json_errors() {
    let world = World::start().await;
    let mut conn = world.attested().await;

    let missing = get("/v1/nothing").send::<Value>(&mut conn).await;
    assert_eq!(refusal(missing), (404, ErrorCode::NotFound));
    let huge = json!({"product": "x".repeat(70 * 1024), "secret_hash": "00".repeat(32)});
    let oversized = post("/v1/orders", huge).send::<Value>(&mut conn).await;
    assert_eq!(refusal(oversized), (413, ErrorCode::PayloadTooLarge));

    drop(conn);
    world.stop().await;
}
