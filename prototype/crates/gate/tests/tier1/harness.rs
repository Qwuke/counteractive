//! A gate in this process on temp sockets, with a fake engine, a signing
//! operator and a wallet-side holder: everything a tier 1 test drives.

use std::path::Path;
use std::time::Duration;

use attest::{DevAttestationKey, PlatformVerifier, Role};
use bytes::Bytes;
use credit::{Credential, Holder, IssuerPublicKey, ServiceContext};
use fake_engine::RunningEngine;
use gate::{GateOptions, RunningGate};
use gate_conn::{ApiRequest, AttestedConn, ConnError, DialScope, SseStream, Target, UnixDialer, UnverifiedConn};
use http_body_util::{BodyExt as _, Full};
use hyper::StatusCode;
use hyper_util::rt::TokioIo;
use manifest::{
    AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, OperatorSecretKey, Release, Service, SignedManifest,
};
use proto::gate::{
    CloseResponse, GateState, LocalIssueRequest, LocalIssueResponse, OpenSessionRequest, OpenSessionResponse,
    RecoverRequest, RecoverResponse, SessionChatRequest, StateResponse, UnlockResponse,
};
use proto::openai::{ChatCompletionChunk, ChatMessage, ChatRole};
use proto::sse::{EVENT_DONE, EVENT_RECEIPT};
use proto::{Blob, ErrorBody, ErrorCode, Receipt, SecretBlob, SessionId};
use rand::TryRng as _;
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::Digest as _;
use tokio::net::{UnixListener, UnixStream};

pub const SERVICE_ID: &str = "counteractive-test";
/// Credits in the `usd-5` and `usd-1` products.
pub const USD_5: u64 = 5_000_000;
pub const USD_1: u64 = 1_000_000;
/// The default policy's maximum spend.
pub const MAXIMUM_SPEND: u64 = 500_000;
/// Long enough for any one step here; a hang fails instead of stalling the run.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// Session limits for a test; [`Policy::default`] matches a normal release.
#[derive(Clone, Copy)]
pub struct Policy {
    pub idle_seconds: u64,
    pub max_turns: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self { idle_seconds: 900, max_turns: 64 }
    }
}

/// The files and keys a gate starts from, and the fake engine it talks to.
pub struct World {
    _dir: tempfile::TempDir,
    k: [u8; 32],
    operator: OperatorSecretKey,
    release_config: Value,
    dev_public_key: [u8; 32],
    options: GateOptions,
    _engine: RunningEngine,
}

impl World {
    pub fn new(policy: Policy) -> Self {
        // The gate's logs, shown with a failing test's output.
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
            "products": [
                {"id": "usd-5", "usd_cents": 500, "credits": USD_5},
                {"id": "usd-1", "usd_cents": 100, "credits": USD_1}
            ],
            "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
            "session_policy": {"maximum_spend": MAXIMUM_SPEND, "idle_seconds": policy.idle_seconds,
                               "max_turns": policy.max_turns, "max_age_seconds": 3600},
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
        Self { _dir: dir, k, operator, release_config, dev_public_key: dev_key.public_key(), options, _engine: engine }
    }

    /// Starts a gate on this world's files: Locked, as after any boot.
    pub async fn start(&self) -> RunningGate {
        gate::start(self.options.clone()).await.unwrap()
    }

    /// The operator's bearer token: K in hex.
    pub fn k_token(&self) -> SecretString {
        SecretString::from(hex::encode(self.k))
    }

    /// A manifest for this release naming `issuer_public_key`.
    pub fn manifest(&self, issuer_public_key: &[u8], version: u64) -> Manifest {
        Manifest {
            schema: 1,
            version,
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
                    dev_public_key: Some(Blob::new(self.dev_public_key.to_vec())),
                },
                release_config_sha256: manifest::release_config_sha256(&self.release_config),
                release_config: self.release_config.clone(),
            }],
        }
    }

    /// The manifest signed by this world's operator.
    pub fn signed(&self, issuer_public_key: &[u8], version: u64) -> SignedManifest {
        manifest::sign_manifest(&self.manifest(issuer_public_key, version), &self.operator).unwrap()
    }

    /// The same manifest signed by a different operator key.
    pub fn signed_by_stranger(&self, issuer_public_key: &[u8], version: u64) -> SignedManifest {
        let stranger = OperatorSecretKey::generate().unwrap();
        manifest::sign_manifest(&self.manifest(issuer_public_key, version), &stranger).unwrap()
    }

    /// A connection that attested the gate against this release. Before
    /// unlock the operator doesn't know the issuer key yet, and evidence
    /// doesn't depend on it, so any key will do.
    pub async fn attested(&self, gate: &RunningGate) -> Client {
        let manifest = self.manifest(&[2; 33], 1);
        let conn = connect(gate).await;
        Client { conn: conn.attest(&PlatformVerifier::new(true), &manifest).await.unwrap() }
    }

    /// Starts a gate, unlocks it and gives it its first manifest.
    pub async fn ready(&self) -> (RunningGate, Client, Wallet) {
        let gate = self.start().await;
        let mut operator = self.attested(&gate).await;
        let unlocked = operator.unlock(self.k_token()).await.unwrap();
        assert_eq!(unlocked.state, GateState::AwaitingManifest);
        let state =
            operator.post_manifest(self.k_token(), &self.signed(unlocked.issuer_public_key.as_bytes(), 1)).await;
        assert_eq!(state.unwrap().state, GateState::Ready);
        (gate, operator, Wallet::new(unlocked.issuer_public_key.as_bytes()))
    }
}

/// An unattested inner-TLS connection to `gate.sock`.
pub async fn connect(gate: &RunningGate) -> UnverifiedConn {
    let target = Target::Unix(gate.gate_socket().to_path_buf());
    UnverifiedConn::connect(&UnixDialer, &target, Role::Gate, &DialScope::new()).await.unwrap()
}

/// An attested connection, with the gate's routes as methods.
pub struct Client {
    pub conn: AttestedConn,
}

impl Client {
    pub async fn unlock(&mut self, token: SecretString) -> Result<UnlockResponse, ConnError> {
        self.conn.send_json(ApiRequest::post("/v1/operator/unlock").bearer(token)).await
    }

    pub async fn post_manifest(
        &mut self,
        token: SecretString,
        signed: &SignedManifest,
    ) -> Result<StateResponse, ConnError> {
        self.conn.send_json(ApiRequest::post("/v1/operator/manifest").json(signed)?.bearer(token)).await
    }

    pub async fn open(&mut self, spend: &[u8]) -> Result<OpenSessionResponse, ConnError> {
        let body = OpenSessionRequest { spend: SecretBlob::new(spend.to_vec()) };
        timed(self.conn.send_json(ApiRequest::post("/v1/sessions").json(&body)?)).await
    }

    /// Opens a session that must open, returning its id and budget.
    pub async fn open_live(&mut self, spend: &[u8]) -> (SessionId, u64) {
        match self.open(spend).await.unwrap() {
            OpenSessionResponse::Open { session_id, budget, .. } => (session_id, budget),
            other @ OpenSessionResponse::Settled { .. } => panic!("expected an open session, got {other:?}"),
        }
    }

    pub async fn recover(&mut self, spend: &[u8]) -> Result<RecoverResponse, ConnError> {
        let body = RecoverRequest { spend: SecretBlob::new(spend.to_vec()) };
        timed(self.conn.send_json(ApiRequest::post("/v1/sessions/recover").json(&body)?)).await
    }

    pub async fn close(&mut self, id: SessionId) -> Result<CloseResponse, ConnError> {
        timed(self.conn.send_json(ApiRequest::post(format!("/v1/sessions/{}/close", id.to_hex())))).await
    }

    /// Runs one turn and reads its stream to the end.
    pub async fn chat(&mut self, id: SessionId, request: &SessionChatRequest) -> Result<Turn, ConnError> {
        let path = format!("/v1/sessions/{}/chat", id.to_hex());
        let stream = timed(self.conn.send_sse(ApiRequest::post(path).json(request)?)).await?;
        Ok(timed(Turn::read(stream)).await)
    }
}

/// What a turn's stream carried.
#[derive(Debug)]
pub struct Turn {
    pub text: String,
    pub receipt: Receipt,
}

impl Turn {
    async fn read(mut stream: SseStream) -> Self {
        let mut text = String::new();
        let mut receipt = None;
        while let Some(event) = stream.next_event().await.unwrap() {
            if event.is(EVENT_RECEIPT) {
                receipt = Some(serde_json::from_str(&event.data).unwrap());
            } else if event.is(EVENT_DONE) {
                assert!(receipt.is_some(), "done before the receipt");
            } else {
                assert!(receipt.is_none(), "content after the receipt");
                let chunk: ChatCompletionChunk = serde_json::from_str(&event.data).unwrap();
                text.push_str(chunk.text().unwrap());
            }
        }
        Self { text, receipt: receipt.expect("a turn always ends with a receipt") }
    }
}

/// A chat request for one model.
pub fn chat_request(model: &str, messages: Vec<ChatMessage>, max_completion_tokens: Option<u32>) -> SessionChatRequest {
    SessionChatRequest { model: model.into(), messages, max_completion_tokens }
}

pub fn user(text: &str) -> ChatMessage {
    ChatMessage::new(ChatRole::User, text)
}

pub fn assistant(text: &str) -> ChatMessage {
    ChatMessage::new(ChatRole::Assistant, text)
}

/// The wallet side: credentials, spends and refunds, bought over `issue.sock`.
pub struct Wallet {
    pub holder: Holder,
}

impl Wallet {
    pub fn new(issuer_public_key: &[u8]) -> Self {
        let context = ServiceContext::new(SERVICE_ID, 1).unwrap();
        Self { holder: Holder::new(&context, &IssuerPublicKey::from_bytes(issuer_public_key).unwrap()).unwrap() }
    }

    /// Buys `credits` under a fresh order and finalizes the credential.
    pub async fn buy(&self, gate: &RunningGate, credits: u64) -> Credential {
        let (state, request) = self.holder.issue_request().unwrap();
        let response = issue(gate, &issue_body(random(), credits, request)).await.unwrap();
        self.holder.finalize_issue(state, response.issue_response.as_bytes()).unwrap()
    }
}

pub fn issue_body(order_ref: [u8; 32], credits: u64, issue_request: Vec<u8>) -> LocalIssueRequest {
    LocalIssueRequest { order_ref, credits, issue_request: Blob::new(issue_request) }
}

/// `POST /local/issue` on `issue.sock`, as the storefront sends it.
pub async fn issue(gate: &RunningGate, request: &LocalIssueRequest) -> Result<LocalIssueResponse, (u16, ErrorCode)> {
    let (status, body) = post_plain(gate.issue_socket(), "/local/issue", serde_json::to_vec(request).unwrap()).await;
    if status.is_success() {
        Ok(serde_json::from_slice(&body).unwrap())
    } else {
        Err((status.as_u16(), serde_json::from_slice::<ErrorBody>(&body).unwrap().error))
    }
}

/// One plain HTTP/1.1 POST over a Unix socket.
async fn post_plain(socket: &Path, path: &str, body: Vec<u8>) -> (StatusCode, Bytes) {
    let stream = UnixStream::connect(socket).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(connection);
    let request = hyper::Request::post(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap();
    let response = timed(sender.send_request(request)).await.unwrap();
    let status = response.status();
    (status, response.into_body().collect().await.unwrap().to_bytes())
}

/// The API error code a failed call carried.
pub fn code<T: std::fmt::Debug>(result: Result<T, ConnError>) -> (u16, ErrorCode) {
    match result {
        Err(ConnError::Api { status, error: Some(body) }) => (status, body.error),
        other => panic!("expected an API error, got {other:?}"),
    }
}

/// The error body itself, for codes that carry more than the code.
pub fn error_body<T: std::fmt::Debug>(result: Result<T, ConnError>) -> ErrorBody {
    match result {
        Err(ConnError::Api { error: Some(body), .. }) => body,
        other => panic!("expected an API error, got {other:?}"),
    }
}

/// The refund bytes of a settled recover.
pub fn settled(response: RecoverResponse) -> Vec<u8> {
    match response {
        RecoverResponse::Settled { refund } => refund.into_vec(),
        other @ RecoverResponse::Live { .. } => panic!("expected a settled session, got {other:?}"),
    }
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    rand::rngs::SysRng.try_fill_bytes(&mut bytes).unwrap();
    bytes
}

async fn timed<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(TIMEOUT, future).await.expect("timed out")
}

/// Lets `RunningGate::shutdown` finish, failing rather than hanging.
pub async fn stop(gate: RunningGate) {
    timed(gate.shutdown()).await;
}
