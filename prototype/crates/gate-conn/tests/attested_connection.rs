//! A real server over a temp Unix socket: inner TLS, manifest, dev
//! attestation, then authenticated and streaming requests. Plus the
//! negatives: a verifier expecting another role, and pins that refuse
//! dev attestation.

#![expect(clippy::unwrap_used, reason = "test helpers: a setup step that fails should fail the test at once")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use attest::{
    AttestationSource as _, DevAttestationKey, DevInsecureSource, Evidence, PlatformVerifier, Rejection, Role,
};
use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use bytes::Bytes;
use gate_conn::server::{ServeOptions, ServerIdentity, serve_unix};
use gate_conn::{
    ApiRequest, AttestFailure, AttestedConn, ConnError, DialError, DialScope, Target, UnixDialer, UnverifiedConn,
    VerdictCache,
};
use http_body_util::channel::Channel;
use manifest::{
    AcceptPins, AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, OperatorSecretKey, Release, Service,
    SignedManifest,
};
use proto::gate::{AttestationRequest, GateHealth, GateState};
use proto::sse::{EVENT_DONE, EVENT_RECEIPT};
use proto::{Blob, ErrorBody, ErrorCode};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::net::UnixListener;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const SERVICE_ID: &str = "counteractive-poc";
const TOKEN: &str = "test-bearer-token";
const TIMEOUT: Duration = Duration::from_secs(10);

/// What the test server serves, and what it observed.
struct ServerState {
    identity: ServerIdentity,
    source: DevInsecureSource,
    signed: SignedManifest,
    evidence_fetches: AtomicUsize,
    /// Released by the client once it has read the stream's first event.
    first_event_seen: Notify,
}

struct Fixture {
    _dir: tempfile::TempDir,
    target: Target,
    pins: AcceptPins,
    state: Arc<ServerState>,
    shutdown: CancellationToken,
    server: JoinHandle<()>,
}

fn release_config() -> Value {
    json!({
        "schema": 1, "image_version": "0.1.0", "service_id": SERVICE_ID, "key_epoch": 1,
        "balance_width": 24, "operator_public_key": "RWQ", "k_sha256": "00".repeat(32),
        "products": [{"id": "usd-5", "usd_cents": 500, "credits": 5_000_000}],
        "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
        "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64,
                           "max_age_seconds": 3600},
        "catalogue": [{"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                       "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096}]
    })
}

/// A signed manifest with one dev release, and the source that attests to it.
fn signed_world(operator: &OperatorSecretKey) -> (SignedManifest, DevInsecureSource) {
    let dev_key = DevAttestationKey::generate().unwrap();
    let config = release_config();
    let config_sha256 = manifest::release_config_sha256(&config);
    let manifest = Manifest {
        schema: 1,
        version: 3,
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
                dev_public_key: Some(Blob::new(dev_key.public_key().to_vec())),
            },
            release_config_sha256: config_sha256,
            release_config: config,
        }],
    };
    let signed = manifest::sign_manifest(&manifest, operator).unwrap();
    (signed, DevInsecureSource::new(dev_key, config_sha256))
}

fn start() -> Fixture {
    let operator = OperatorSecretKey::generate().unwrap();
    let (signed, source) = signed_world(&operator);
    let state = Arc::new(ServerState {
        identity: ServerIdentity::generate(Role::Gate).unwrap(),
        source,
        signed,
        evidence_fetches: AtomicUsize::new(0),
        first_event_seen: Notify::new(),
    });
    let router = axum::Router::new()
        .route("/v1/health", get(health))
        .route("/v1/manifest", get(manifest_route))
        .route("/v1/attestation", post(attestation))
        .route("/v1/echo", post(echo))
        .route("/v1/stream", post(stream))
        .with_state(Arc::clone(&state));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gate.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let shutdown = CancellationToken::new();
    let server = tokio::spawn(serve_unix(
        listener,
        Some(state.identity.tls_acceptor()),
        router,
        ServeOptions::default(),
        shutdown.clone(),
    ));
    let pins = AcceptPins {
        operator_public_key: operator.public_key().unwrap(),
        service_id: SERVICE_ID.into(),
        key_epoch: 1,
        onion: None,
        allow_dev_attestation: true,
    };
    Fixture { _dir: dir, target: Target::Unix(path), pins, state, shutdown, server }
}

impl Fixture {
    async fn connect(&self, role: Role) -> UnverifiedConn {
        UnverifiedConn::connect(&UnixDialer, &self.target, role, &DialScope::new()).await.unwrap()
    }

    /// Connects, fetches and accepts the manifest, and attests.
    async fn attested(&self) -> (AttestedConn, Manifest) {
        let mut conn = self.connect(Role::Gate).await;
        let manifest = manifest::accept(&conn.fetch_manifest().await.unwrap(), &self.pins, None).unwrap();
        let attested = conn.attest(&PlatformVerifier::new(true), &manifest).await.unwrap();
        (attested, manifest)
    }

    fn evidence_fetches(&self) -> usize {
        self.state.evidence_fetches.load(Ordering::SeqCst)
    }

    async fn stop(self) {
        self.shutdown.cancel();
        tokio::time::timeout(TIMEOUT, self.server).await.unwrap().unwrap();
    }
}

async fn health() -> Json<GateHealth> {
    Json(GateHealth { state: GateState::Ready, version: "0.1.0".into() })
}

async fn manifest_route(State(state): State<Arc<ServerState>>) -> Json<SignedManifest> {
    Json(state.signed.clone())
}

async fn attestation(State(state): State<Arc<ServerState>>, Json(request): Json<AttestationRequest>) -> Json<Evidence> {
    state.evidence_fetches.fetch_add(1, Ordering::SeqCst);
    Json(state.source.evidence(&state.identity.binding(request.nonce)).unwrap())
}

async fn echo(headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let expected = format!("Bearer {TOKEN}");
    if headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) != Some(expected.as_str()) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorBody::new(ErrorCode::Unauthorized))).into_response();
    }
    Json(body).into_response()
}

/// Sends one event split across two frames, waits until the client has
/// read it, then sends the rest. A client that buffered whole bodies
/// would never see the first event, and the test would time out.
async fn stream(State(state): State<Arc<ServerState>>) -> Response {
    let (mut sender, body) = Channel::<Bytes>::new(4);
    tokio::spawn(async move {
        sender.send_data(Bytes::from_static(b"data: {\"n\":")).await.unwrap();
        sender.send_data(Bytes::from_static(b"1}\n\n")).await.unwrap();
        state.first_event_seen.notified().await;
        sender
            .send_data(Bytes::from_static(b"event: receipt\ndata: {\"turn\":1}\n\nevent: done\ndata: {}\n\n"))
            .await
            .unwrap();
    });
    ([(header::CONTENT_TYPE, "text/event-stream")], Body::new(body)).into_response()
}

#[tokio::test]
async fn attests_then_sends_authenticated_and_streaming_requests() {
    let fixture = start();

    let mut conn = fixture.connect(Role::Gate).await;
    assert_eq!(conn.spki_sha256(), fixture.state.identity.spki_sha256(), "SPKI recorded from the handshake");
    let health: GateHealth = conn.health().await.unwrap();
    assert_eq!(health.state, GateState::Ready);
    let manifest = manifest::accept(&conn.fetch_manifest().await.unwrap(), &fixture.pins, None).unwrap();
    let mut conn = conn.attest(&PlatformVerifier::new(true), &manifest).await.unwrap();
    assert_eq!(conn.verdict().role, Role::Gate);
    assert_eq!(conn.verdict().manifest_version, 3);
    assert!(conn.verdict().is_insecure());
    assert_eq!(conn.verdict().release_config.model("fake-small").unwrap().output_rate, 3);

    let request = || ApiRequest::post("/v1/echo").json(&json!({"hello": "gate"})).unwrap();
    let echoed: Value = conn.send_json(request().bearer(SecretString::from(TOKEN))).await.unwrap();
    assert_eq!(echoed, json!({"hello": "gate"}));

    let refused = conn.send_json::<Value>(request()).await.unwrap_err();
    assert!(matches!(refused, ConnError::Api { status: 401, .. }), "{refused:?}");
    assert_eq!(refused.error_code(), Some(ErrorCode::Unauthorized));

    let mut events = conn.send_sse(ApiRequest::post("/v1/stream")).await.unwrap();
    let first = tokio::time::timeout(TIMEOUT, events.next_event()).await.unwrap().unwrap().unwrap();
    assert_eq!((first.event.as_deref(), first.data.as_str()), (None, r#"{"n":1}"#));
    fixture.state.first_event_seen.notify_one();
    let receipt = tokio::time::timeout(TIMEOUT, events.next_event()).await.unwrap().unwrap().unwrap();
    assert!(receipt.is(EVENT_RECEIPT));
    let done = events.next_event().await.unwrap().unwrap();
    assert!(done.is(EVENT_DONE));
    assert!(events.next_event().await.unwrap().is_none());

    // The connection is still usable after a stream ends.
    let again: Value = conn.send_json(request().bearer(SecretString::from(TOKEN))).await.unwrap();
    assert_eq!(again, json!({"hello": "gate"}));

    drop(conn);
    fixture.stop().await;
}

#[tokio::test]
async fn a_verifier_expecting_another_role_rejects() {
    let fixture = start();
    let mut conn = fixture.connect(Role::Storefront).await;
    let manifest = manifest::accept(&conn.fetch_manifest().await.unwrap(), &fixture.pins, None).unwrap();
    let failure = conn.attest(&PlatformVerifier::new(true), &manifest).await.unwrap_err();
    assert!(
        matches!(
            failure,
            AttestFailure::Rejected(Rejection::WrongRole { expected: Role::Storefront, claimed: Role::Gate })
        ),
        "{failure:?}"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn pins_without_dev_attestation_reject_a_dev_server() {
    let fixture = start();
    let mut conn = fixture.connect(Role::Gate).await;
    let manifest = manifest::accept(&conn.fetch_manifest().await.unwrap(), &fixture.pins, None).unwrap();
    let failure = conn.attest(&PlatformVerifier::new(false), &manifest).await.unwrap_err();
    assert!(matches!(failure, AttestFailure::Rejected(Rejection::DevAttestationNotAllowed)), "{failure:?}");
    fixture.stop().await;
}

#[tokio::test]
async fn a_cached_verdict_skips_the_evidence_fetch() {
    let fixture = start();
    let (_, manifest) = fixture.attested().await;
    assert_eq!(fixture.evidence_fetches(), 1);

    let verifier = PlatformVerifier::new(true);
    let cache = VerdictCache::new();
    let first = fixture.connect(Role::Gate).await.attest_cached(&verifier, &manifest, &cache).await.unwrap();
    let second = fixture.connect(Role::Gate).await.attest_cached(&verifier, &manifest, &cache).await.unwrap();
    assert_eq!(fixture.evidence_fetches(), 2, "the second connection reused the verdict");
    assert_eq!(first.verdict(), second.verdict());

    // Another role never shares a verdict, even with the same TLS key.
    let other_role = fixture.connect(Role::Storefront).await.attest_cached(&verifier, &manifest, &cache).await;
    assert!(matches!(other_role, Err(AttestFailure::Rejected(Rejection::WrongRole { .. }))));

    let expired = VerdictCache::with_ttl(Duration::ZERO);
    fixture.connect(Role::Gate).await.attest_cached(&verifier, &manifest, &expired).await.unwrap();
    fixture.connect(Role::Gate).await.attest_cached(&verifier, &manifest, &expired).await.unwrap();
    assert_eq!(fixture.evidence_fetches(), 5);

    drop((first, second));
    fixture.stop().await;
}

#[tokio::test]
async fn a_unix_dialer_refuses_other_targets() {
    let target: Target = "tcp:127.0.0.1:1".parse().unwrap();
    let error = UnverifiedConn::connect(&UnixDialer, &target, Role::Gate, &DialScope::new()).await.unwrap_err();
    assert!(matches!(error, ConnError::Dial(DialError::UnsupportedTarget("tcp"))), "{error:?}");
}
