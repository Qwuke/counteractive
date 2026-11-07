//! What walletd's own tests run against: a gate in this process on temp
//! sockets, with a fake engine and a signing operator, and a wallet whose
//! credit was issued through the gate's `issue.sock`, served by a
//! [`Walletd`] built exactly as `main` builds one. It mirrors the gate's
//! tier 1 harness (`crates/gate/tests/tier1/harness.rs`) from the wallet's
//! side.

use std::sync::Arc;
use std::time::Duration;

use attest::{DevAttestationKey, PlatformVerifier, Role};
use axum::body::Body;
use bytes::Bytes;
use fake_engine::RunningEngine;
use gate::{GateOptions, RunningGate};
use gate_conn::{ApiRequest, AttestedConn, DialScope, Dialer, Target, UnixDialer, UnverifiedConn};
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use manifest::{
    AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, OperatorSecretKey, Product, Release, Service,
    SignedManifest,
};
use proto::gate::{GateState, LocalIssueRequest, LocalIssueResponse, StateResponse, UnlockResponse};
use proto::walletd::{BalanceView, PurchaseState, ReconcileReport};
use proto::{Blob, Receipt};
use rand::TryRng as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use sha2::Digest as _;
use tokio::net::{UnixListener, UnixStream};
use tower::ServiceExt as _;
use wallet_core::{ChainId, ChainRow, Journal, Pins, manifest_store, purchase};

use crate::api;
use crate::gate::GateClient;
use crate::state::Walletd;
use crate::token::{TOKEN_FILE, Token};

pub const SERVICE_ID: &str = "counteractive-test";
/// Credits in a `usd-5` purchase.
pub const USD_5: u64 = 5_000_000;
/// The `Host` the tests' requests name: the address walletd is told it
/// listens on.
pub const HOST: &str = "127.0.0.1:7777";
/// Long enough for any one step here, including a redial after a 10 s
/// handshake timeout; a hang fails instead of stalling the run.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Session limits; [`Policy::default`] matches a normal release.
#[derive(Clone, Copy)]
pub struct Policy {
    pub maximum_spend: u64,
    pub idle_seconds: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self { maximum_spend: 500_000, idle_seconds: 900 }
    }
}

/// A gate in this process: Ready, with manifest v1 naming its issuer key.
pub struct TestGate {
    k: [u8; 32],
    options: GateOptions,
    running: Option<RunningGate>,
    /// The manifest the gate runs under, and as the operator signed it.
    pub manifest: Manifest,
    pub signed: SignedManifest,
    /// The operator's public key, as the wallet pins it.
    pub operator_public_key: String,
    issuer_public_key: Vec<u8>,
    _engine: RunningEngine,
    // Dropped last: it holds the sockets and the store.
    _dir: tempfile::TempDir,
}

impl TestGate {
    /// Writes a release with `policy`, starts the fake engine and the
    /// gate, unlocks it and posts its first manifest.
    pub async fn start(policy: Policy) -> Self {
        let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("info").try_init();
        let dir = tempfile::tempdir().unwrap();
        let k = random();
        let operator = OperatorSecretKey::generate().unwrap();
        let dev_key = DevAttestationKey::generate().unwrap();
        let release_config = json!({
            "schema": 1, "image_version": "0.1.0-test", "service_id": SERVICE_ID, "key_epoch": 1,
            "balance_width": 24,
            "operator_public_key": operator.public_key().unwrap().as_base64(),
            "k_sha256": hex::encode(sha2::Sha256::digest(k)),
            "products": [{"id": "usd-5", "usd_cents": 500, "credits": USD_5}],
            "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
            "session_policy": {"maximum_spend": policy.maximum_spend, "idle_seconds": policy.idle_seconds,
                               "max_turns": 64, "max_age_seconds": 3600},
            "catalogue": [
                {"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                 "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096},
                {"id": "fake-large", "engine": "fake", "usage_trust": "inside_attested_boundary",
                 "input_rate": 4, "output_rate": 12, "max_output": 4096, "context": 4096}
            ]
        });
        let release = dir.path().join("release.json");
        std::fs::write(&release, serde_json::to_vec_pretty(&release_config).unwrap()).unwrap();
        let dev_attest_key = dir.path().join("dev-attest.key");
        std::fs::write(&dev_attest_key, *dev_key.to_bytes()).unwrap();
        let engine_socket = dir.path().join("engine.sock");
        let engine = fake_engine::spawn(UnixListener::bind(&engine_socket).unwrap());
        let options = GateOptions {
            release,
            data: dir.path().join("data"),
            run: dir.path().join("run"),
            engine: engine_socket,
            dev_attest_key,
        };
        let manifest = |issuer_public_key: &[u8]| Manifest {
            schema: 1,
            version: 1,
            service: Service {
                id: SERVICE_ID.into(),
                onion: None,
                credit_scheme: CREDIT_SCHEME.into(),
                issuer_public_key: Blob::new(issuer_public_key.to_vec()),
                key_epoch: 1,
                balance_width: BALANCE_WIDTH,
            },
            releases: vec![Release {
                image_version: "0.1.0-test".into(),
                platform: "dev-insecure".into(),
                attestation_policy: AttestationPolicy {
                    kind: "dev-insecure".into(),
                    dev_public_key: Some(Blob::new(dev_key.public_key().to_vec())),
                },
                release_config_sha256: manifest::release_config_sha256(&release_config),
                release_config: release_config.clone(),
            }],
        };

        let running = gate::start(options.clone()).await.unwrap();
        // Evidence doesn't depend on the issuer key, which the operator
        // learns only from the unlock, so any key does for this first check.
        let mut operator_conn = attested(&running, &manifest(&[2; 33])).await;
        let unlocked: UnlockResponse =
            operator_conn.send_json(ApiRequest::post("/v1/operator/unlock").bearer(k_token(&k))).await.unwrap();
        assert_eq!(unlocked.state, GateState::AwaitingManifest);
        let issuer_public_key = unlocked.issuer_public_key.as_bytes().to_vec();
        let signed_manifest = manifest(&issuer_public_key);
        let signed = manifest::sign_manifest(&signed_manifest, &operator).unwrap();
        let request = ApiRequest::post("/v1/operator/manifest").json(&signed).unwrap().bearer(k_token(&k));
        let posted: StateResponse = operator_conn.send_json(request).await.unwrap();
        assert_eq!(posted.state, GateState::Ready);
        Self {
            k,
            options,
            running: Some(running),
            manifest: signed_manifest,
            signed,
            operator_public_key: operator.public_key().unwrap().as_base64().to_owned(),
            issuer_public_key,
            _engine: engine,
            _dir: dir,
        }
    }

    /// `gate.sock`, where the wallet dials.
    pub fn target(&self) -> Target {
        Target::Unix(self.options.run.join("gate.sock"))
    }

    /// Stops the gate without settling its live sessions, which is what
    /// a crash does too, then starts it from its store and unlocks it.
    pub async fn restart(&mut self) {
        let running = self.running.take().expect("the gate is running");
        tokio::time::timeout(TIMEOUT, running.shutdown()).await.expect("the gate stops");
        let running = gate::start(self.options.clone()).await.unwrap();
        let mut operator_conn = attested(&running, &self.manifest).await;
        let unlocked: UnlockResponse =
            operator_conn.send_json(ApiRequest::post("/v1/operator/unlock").bearer(k_token(&self.k))).await.unwrap();
        assert_eq!(unlocked.state, GateState::Ready, "the store recorded the accepted manifest");
        assert_eq!(unlocked.issuer_public_key.as_bytes(), self.issuer_public_key.as_slice());
        self.running = Some(running);
    }

    /// An attested connection of the tests' own, for what the wallet
    /// deliberately never does, such as resending a spend by hand.
    pub async fn client(&self) -> AttestedConn {
        attested(self.running.as_ref().expect("the gate is running"), &self.manifest).await
    }

    /// `POST /local/issue`, as the storefront sends it after a payment.
    async fn issue(&self, credits: u64, issue_request: &[u8]) -> LocalIssueResponse {
        let request =
            LocalIssueRequest { order_ref: random(), credits, issue_request: Blob::new(issue_request.to_vec()) };
        let stream = UnixStream::connect(self.options.run.join("issue.sock")).await.unwrap();
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
        tokio::spawn(connection);
        let request = hyper::Request::post("/local/issue")
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(serde_json::to_vec(&request).unwrap())))
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        assert!(response.status().is_success(), "issuance failed: {}", response.status());
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
}

/// An attested connection to `gate` over its Unix socket.
async fn attested(gate: &RunningGate, manifest: &Manifest) -> AttestedConn {
    let target = Target::Unix(gate.gate_socket().to_path_buf());
    let conn = UnverifiedConn::connect(&UnixDialer, &target, Role::Gate, &DialScope::new()).await.unwrap();
    conn.attest(&PlatformVerifier::new(true), manifest).await.unwrap()
}

fn k_token(k: &[u8; 32]) -> SecretString {
    SecretString::from(hex::encode(k))
}

/// A wallet directory with the gate's manifest accepted and credit
/// collected, and the walletd that serves it.
pub struct TestWallet {
    /// walletd's state, as `main` assembles it.
    pub walletd: Arc<Walletd>,
    token: SecretString,
    // Dropped last: it holds the journal.
    _dir: tempfile::TempDir,
}

impl TestWallet {
    /// A wallet for `gate` holding one chain per entry of `chains`, each
    /// with that many credits, served by a walletd that dials with
    /// `dialer`.
    pub async fn new(gate: &TestGate, chains: &[u64], dialer: Box<dyn Dialer>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pins = Pins {
            service_id: SERVICE_ID.into(),
            key_epoch: 1,
            gate_target: gate.target().to_string(),
            storefront_addr: "127.0.0.1:9".into(),
            operator_public_key: gate.operator_public_key.clone(),
            tier_ladder: vec![500_000, 50_000, 5_000],
            allow_dev_attestation: true,
        };
        let mut journal = Journal::create(dir.path()).unwrap();
        let accept_pins = pins.accept_pins().unwrap();
        manifest_store::offer(&mut journal, &accept_pins, &gate.signed).unwrap();
        let manifest = manifest_store::approve(&mut journal, &accept_pins).unwrap();
        let holder = manifest_store::holder(&pins, &manifest).unwrap();
        for &credits in chains {
            // The journal steps `wallet-cli buy` and `collect` take, with
            // the storefront and Stripe left out.
            let product = Product { id: "usd-5".into(), usd_cents: 500, credits };
            let id = purchase::begin(&mut journal, &holder, &product).unwrap();
            journal.set_order_id(id, &format!("cs_test_{}", id.0)).unwrap();
            let row = journal.purchases_in(&[PurchaseState::AwaitingPayment]).unwrap().remove(0);
            let response = gate.issue(credits, row.issue_request.as_deref().unwrap()).await;
            purchase::complete(&mut journal, &holder, &pins.tier_ladder, &row, response.issue_response.as_bytes())
                .unwrap();
        }
        let token = Token::load_or_create(dir.path()).unwrap();
        let token_text = std::fs::read_to_string(dir.path().join(TOKEN_FILE)).unwrap();
        let client = GateClient::new(dialer, gate.target(), true);
        let allowed_hosts = vec![HOST.to_owned(), "localhost:7777".to_owned()];
        let walletd = Arc::new(Walletd::new(pins, journal, client, token, allowed_hosts).unwrap());
        Self { walletd, token: SecretString::from(token_text), _dir: dir }
    }

    /// A wallet on the gate's Unix socket, as tier 1 runs it.
    pub async fn unix(gate: &TestGate, chains: &[u64]) -> Self {
        Self::new(gate, chains, Box::new(UnixDialer)).await
    }

    /// The journal's row for a chain.
    pub fn chain(&self, chain: i64) -> ChainRow {
        self.walletd.journal().chain(ChainId(chain)).unwrap()
    }

    /// A local API request with the right `Host` and bearer token.
    pub fn request(&self, method: &str, path: &str) -> hyper::http::request::Builder {
        hyper::Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, HOST)
            .header(hyper::header::AUTHORIZATION, format!("Bearer {}", self.token.expose_secret()))
    }

    /// Sends a request through walletd's router, as the listener would.
    pub async fn send(&self, request: hyper::Request<Body>) -> (u16, hyper::HeaderMap, Value) {
        let router = api::router(Arc::clone(&self.walletd));
        let response = tokio::time::timeout(TIMEOUT, router.oneshot(request)).await.expect("walletd answers").unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
        (status, headers, value)
    }

    /// `POST path` with a JSON body.
    pub async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let request = self
            .request("POST", path)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let (status, _, value) = self.send(request).await;
        (status, value)
    }

    /// `GET path`.
    pub async fn get(&self, path: &str) -> (u16, Value) {
        let (status, _, value) = self.send(self.request("GET", path).body(Body::empty()).unwrap()).await;
        (status, value)
    }

    /// One non-streamed turn in `conversation` through
    /// `/v1/chat/completions`: its status, headers and body.
    pub async fn chat(&self, conversation: &str, model: &str, prompt: &str) -> (u16, hyper::HeaderMap, Value) {
        let body = json!({"model": model, "messages": [{"role": "user", "content": prompt}], "stream": false});
        let request = self
            .request("POST", "/v1/chat/completions")
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .header(proto::walletd::CONVERSATION_HEADER, conversation)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        self.send(request).await
    }

    /// A turn that must succeed: its receipt.
    pub async fn chat_ok(&self, conversation: &str, model: &str, prompt: &str) -> Receipt {
        let (status, _, body) = self.chat(conversation, model, prompt).await;
        assert_eq!(status, 200, "chat failed: {body}");
        serde_json::from_value(body["receipt"].clone()).expect("a finished turn carries its receipt")
    }

    /// `GET /wallet/balance`, which must succeed.
    pub async fn balance(&self) -> BalanceView {
        let (status, body) = self.get("/wallet/balance").await;
        assert_eq!(status, 200, "{body}");
        serde_json::from_value(body).unwrap()
    }

    /// `POST /wallet/reconcile`, which must succeed.
    pub async fn reconcile(&self) -> ReconcileReport {
        let (status, body) = self.post("/wallet/reconcile", json!({})).await;
        assert_eq!(status, 200, "{body}");
        serde_json::from_value(body).unwrap()
    }
}

/// Polls until `done` holds, failing after [`TIMEOUT`].
pub async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    while !done() {
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    rand::rngs::SysRng.try_fill_bytes(&mut bytes).unwrap();
    bytes
}
