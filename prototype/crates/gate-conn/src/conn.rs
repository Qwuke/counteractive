//! Connections: a dialed stream, inner TLS, HTTP/1.1, then attestation.
//!
//! The two connection types make the order impossible to get wrong. An
//! [`UnverifiedConn`] can read only public things (health and the
//! manifest) and ask for evidence. Only a successful attestation turns it
//! into an [`AttestedConn`], the one type that can carry a spend, a token
//! or a prompt.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use attest::{AttestationVerifier, Evidence, Rejection, ReportBinding, Role, Verdict};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{self, HeaderValue};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use manifest::Manifest;
use proto::gate::AttestationRequest;
use proto::sse::{SseError, SseEvent, SseSplitter};
use proto::{ErrorBody, ErrorCode, SignedManifest};
use rand::TryRng as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio_rustls::TlsConnector;

use crate::Target;
use crate::cache::VerdictCache;
use crate::dialer::{BoxedIo, DialError, DialScope, Dialer};
use crate::tls::{self, ALPN_HTTP1};

/// SPEC section 4: the TLS handshake must finish within this.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest JSON response body read. The biggest is a manifest.
const MAX_JSON_BODY: usize = 1024 * 1024;

/// The largest error body read; error bodies are a code and a number.
const MAX_ERROR_BODY: usize = 16 * 1024;

/// The largest single SSE event accepted from a server.
const MAX_SSE_EVENT: usize = 64 * 1024;

/// Why a connection or a request on it failed.
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    /// The dialer couldn't open a stream.
    #[error(transparent)]
    Dial(#[from] DialError),
    /// The TLS handshake took longer than [`HANDSHAKE_TIMEOUT`].
    #[error("TLS handshake timed out")]
    HandshakeTimeout,
    /// The TLS handshake failed, including a bad handshake signature.
    #[error("TLS handshake failed: {0}")]
    Tls(std::io::Error),
    /// The server broke the connection contract, such as by not selecting ALPN `http/1.1`.
    #[error("protocol violation: {0}")]
    Protocol(&'static str),
    /// HTTP failed on the connection.
    #[error("HTTP: {0}")]
    Http(#[from] hyper::Error),
    /// The server answered with a non-2xx status.
    #[error("server returned {status}{}", .error.as_ref().map(|e| format!(" ({:?})", e.error)).unwrap_or_default())]
    Api {
        /// The HTTP status.
        status: u16,
        /// The parsed error body, if it was one.
        error: Option<ErrorBody>,
    },
    /// A request body couldn't be serialised.
    #[error("could not encode request: {0}")]
    Encode(serde_json::Error),
    /// A response body wasn't the expected JSON.
    #[error("could not decode response: {0}")]
    Decode(serde_json::Error),
    /// A response body exceeded its size limit.
    #[error("response body too large")]
    BodyTooLarge,
    /// An event stream was malformed.
    #[error(transparent)]
    Sse(#[from] SseError),
    /// The operating system's random number generator failed.
    #[error("no randomness for the attestation nonce")]
    Entropy,
}

impl ConnError {
    /// The API error code, when the server sent one.
    pub fn error_code(&self) -> Option<ErrorCode> {
        match self {
            Self::Api { error: Some(body), .. } => Some(body.error),
            _ => None,
        }
    }
}

/// Why attestation didn't produce an [`AttestedConn`]. Either way the
/// connection is gone and nothing was sent over it.
#[derive(Debug, thiserror::Error)]
pub enum AttestFailure {
    /// Fetching the evidence failed.
    #[error(transparent)]
    Conn(#[from] ConnError),
    /// The evidence was refused.
    #[error("attestation rejected: {0}")]
    Rejected(#[from] Rejection),
}

/// One HTTP request to a service, built without touching the connection.
///
/// `Debug` shows the method and path only: bodies carry spends and prompts,
/// and the bearer token is a secret.
pub struct ApiRequest {
    method: Method,
    path: String,
    body: Option<Bytes>,
    bearer: Option<SecretString>,
}

impl ApiRequest {
    /// A `GET`.
    pub fn get(path: impl Into<String>) -> Self {
        Self { method: Method::GET, path: path.into(), body: None, bearer: None }
    }

    /// A `POST` with an empty body, until [`ApiRequest::json`] adds one.
    pub fn post(path: impl Into<String>) -> Self {
        Self { method: Method::POST, path: path.into(), body: None, bearer: None }
    }

    /// Sets a JSON body.
    pub fn json<T: Serialize>(mut self, body: &T) -> Result<Self, ConnError> {
        self.body = Some(serde_json::to_vec(body).map_err(ConnError::Encode)?.into());
        Ok(self)
    }

    /// Adds `Authorization: Bearer <token>`.
    #[must_use]
    pub fn bearer(mut self, token: SecretString) -> Self {
        self.bearer = Some(token);
        self
    }

    fn into_http(self, role: Role) -> Result<Request<Full<Bytes>>, ConnError> {
        let mut request = Request::builder()
            .method(self.method)
            .uri(self.path)
            .header(header::HOST, HeaderValue::from_static(tls::host_name(role)));
        if self.body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        if let Some(token) = self.bearer {
            let mut value = HeaderValue::try_from(format!("Bearer {}", token.expose_secret()))
                .map_err(|_| ConnError::Protocol("bearer token is not a valid header value"))?;
            value.set_sensitive(true);
            request = request.header(header::AUTHORIZATION, value);
        }
        request.body(Full::new(self.body.unwrap_or_default())).map_err(|_| ConnError::Protocol("invalid request path"))
    }
}

impl std::fmt::Debug for ApiRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiRequest").field("method", &self.method).field("path", &self.path).finish_non_exhaustive()
    }
}

/// The HTTP/1.1 client half shared by both connection types.
struct Http {
    role: Role,
    sender: SendRequest<Full<Bytes>>,
}

impl Http {
    async fn send(&mut self, request: ApiRequest) -> Result<Response<Incoming>, ConnError> {
        let request = request.into_http(self.role)?;
        self.sender.ready().await?;
        Ok(self.sender.send_request(request).await?)
    }

    async fn send_json<T: DeserializeOwned>(&mut self, request: ApiRequest) -> Result<T, ConnError> {
        let response = success(self.send(request).await?).await?;
        let body = read_body(response.into_body(), MAX_JSON_BODY).await?;
        serde_json::from_slice(&body).map_err(ConnError::Decode)
    }
}

/// Passes a 2xx response through, and turns anything else into [`ConnError::Api`].
async fn success(response: Response<Incoming>) -> Result<Response<Incoming>, ConnError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let error = read_body(response.into_body(), MAX_ERROR_BODY)
        .await
        .ok()
        .and_then(|body| serde_json::from_slice::<ErrorBody>(&body).ok());
    Err(ConnError::Api { status: status.as_u16(), error })
}

async fn read_body(body: Incoming, limit: usize) -> Result<Bytes, ConnError> {
    match Limited::new(body, limit).collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(error) => match error.downcast::<hyper::Error>() {
            Ok(error) => Err(ConnError::Http(*error)),
            Err(_) => Err(ConnError::BodyTooLarge),
        },
    }
}

/// A TLS connection whose server isn't attested yet.
pub struct UnverifiedConn {
    http: Http,
    spki_sha256: [u8; 32],
}

impl UnverifiedConn {
    /// Dials `target`, runs inner TLS expecting `role`, records the
    /// server's SPKI hash and starts HTTP/1.1.
    pub async fn connect(
        dialer: &dyn Dialer,
        target: &Target,
        role: Role,
        scope: &DialScope,
    ) -> Result<Self, ConnError> {
        let stream = dialer.dial(target, scope).await?;
        let (tls, spki_sha256) = handshake(stream, role).await?;
        let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(%error, "connection ended with an error");
            }
        });
        Ok(Self { http: Http { role, sender }, spki_sha256 })
    }

    /// The role this connection expects the server to have.
    pub fn role(&self) -> Role {
        self.http.role
    }

    /// SHA-256 of the SPKI the server proved it holds in the handshake.
    pub fn spki_sha256(&self) -> [u8; 32] {
        self.spki_sha256
    }

    /// `GET /v1/health`. The body type differs between gate and storefront.
    pub async fn health<T: DeserializeOwned>(&mut self) -> Result<T, ConnError> {
        self.http.send_json(ApiRequest::get("/v1/health")).await
    }

    /// `GET /v1/manifest`. The caller runs its accept rule on the result.
    pub async fn fetch_manifest(&mut self) -> Result<SignedManifest, ConnError> {
        self.http.send_json(ApiRequest::get("/v1/manifest")).await
    }

    /// `POST /v1/attestation` with `nonce`: the server's evidence, unchecked.
    ///
    /// Evidence is public, so fetching it sends nothing secret. Only
    /// [`UnverifiedConn::attest`] turns it into trust; this is exposed so
    /// tests can check that verification refuses a mismatched binding.
    pub async fn fetch_evidence(&mut self, nonce: [u8; 32]) -> Result<Evidence, ConnError> {
        self.http.send_json(ApiRequest::post("/v1/attestation").json(&AttestationRequest { nonce })?).await
    }

    /// Fetches evidence for a fresh nonce and verifies it against `manifest`.
    ///
    /// Consumes the connection: on any failure it is dropped unused.
    pub async fn attest(
        mut self,
        verifier: &dyn AttestationVerifier,
        manifest: &Manifest,
    ) -> Result<AttestedConn, AttestFailure> {
        let mut nonce = [0u8; 32];
        rand::rngs::SysRng.try_fill_bytes(&mut nonce).map_err(|_| ConnError::Entropy)?;
        let evidence = self.fetch_evidence(nonce).await?;
        let binding = ReportBinding::new(self.http.role, self.spki_sha256, nonce);
        let verdict = verifier.verify(&evidence, &binding, manifest)?;
        Ok(self.into_attested(Arc::new(verdict)))
    }

    /// Like [`UnverifiedConn::attest`], but reuses a verdict for this
    /// server's TLS key from `cache` when one is fresh.
    ///
    /// That is safe because the handshake on *this* connection proved the
    /// server holds the private key the cached report was bound to.
    pub async fn attest_cached(
        self,
        verifier: &dyn AttestationVerifier,
        manifest: &Manifest,
        cache: &VerdictCache,
    ) -> Result<AttestedConn, AttestFailure> {
        if let Some(verdict) = cache.get(self.http.role, &self.spki_sha256, manifest.version) {
            return Ok(self.into_attested(verdict));
        }
        let attested = self.attest(verifier, manifest).await?;
        cache.insert(Arc::clone(&attested.verdict));
        Ok(attested)
    }

    fn into_attested(self, verdict: Arc<Verdict>) -> AttestedConn {
        if verdict.is_insecure() {
            tracing::warn!(
                role = verdict.role.as_str(),
                "connected to a dev-insecure server: no hardware attestation protects this connection"
            );
        }
        AttestedConn { http: self.http, verdict }
    }
}

impl std::fmt::Debug for UnverifiedConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedConn").field("role", &self.http.role).finish_non_exhaustive()
    }
}

/// Inner TLS over a dialed stream. Returns the stream and the leaf's SPKI hash.
async fn handshake(
    stream: BoxedIo,
    role: Role,
) -> Result<(tokio_rustls::client::TlsStream<BoxedIo>, [u8; 32]), ConnError> {
    let connector = TlsConnector::from(tls::client_config());
    let tls = tokio::time::timeout(HANDSHAKE_TIMEOUT, connector.connect(tls::server_name(role), stream))
        .await
        .map_err(|_| ConnError::HandshakeTimeout)?
        .map_err(ConnError::Tls)?;
    let session = tls.get_ref().1;
    if session.alpn_protocol() != Some(ALPN_HTTP1) {
        return Err(ConnError::Protocol("server did not select ALPN http/1.1"));
    }
    let leaf = session
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or(ConnError::Protocol("server sent no certificate"))?;
    let spki_sha256 = tls::spki_sha256(leaf).ok_or(ConnError::Protocol("server certificate is unreadable"))?;
    Ok((tls, spki_sha256))
}

/// A connection to a server whose evidence verified. The only way to send
/// secrets.
pub struct AttestedConn {
    http: Http,
    verdict: Arc<Verdict>,
}

impl AttestedConn {
    /// What attestation established: role, platform, and the release config
    /// (prices, catalogue, policy) the server runs.
    pub fn verdict(&self) -> &Verdict {
        &self.verdict
    }

    /// Whether the server closed the connection, so the caller must reconnect.
    pub fn is_closed(&self) -> bool {
        self.http.sender.is_closed()
    }

    /// Sends a request and decodes a 2xx JSON response.
    pub async fn send_json<T: DeserializeOwned>(&mut self, request: ApiRequest) -> Result<T, ConnError> {
        self.http.send_json(request).await
    }

    /// Sends a request whose 2xx response is an event stream, such as a chat turn.
    ///
    /// The connection can't carry another request until the stream ends.
    pub async fn send_sse(&mut self, request: ApiRequest) -> Result<SseStream, ConnError> {
        let response = success(self.http.send(request).await?).await?;
        Ok(SseStream {
            body: response.into_body(),
            splitter: Some(SseSplitter::new(MAX_SSE_EVENT)),
            ready: VecDeque::new(),
        })
    }
}

impl std::fmt::Debug for AttestedConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttestedConn")
            .field("role", &self.verdict.role)
            .field("platform", &self.verdict.platform)
            .finish_non_exhaustive()
    }
}

/// Events from a streaming response, read as they arrive.
pub struct SseStream {
    body: Incoming,
    /// `None` once the body has ended cleanly.
    splitter: Option<SseSplitter>,
    ready: VecDeque<SseEvent>,
}

impl SseStream {
    /// The next event, or `None` when the stream ended cleanly.
    ///
    /// A stream cut mid-event is [`ConnError::Sse`]; one cut between events
    /// just ends, so callers decide what a missing final event means.
    pub async fn next_event(&mut self) -> Result<Option<SseEvent>, ConnError> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Ok(Some(event));
            }
            let Some(splitter) = self.splitter.as_mut() else {
                return Ok(None);
            };
            match self.body.frame().await {
                Some(frame) => {
                    if let Ok(data) = frame?.into_data() {
                        self.ready.extend(splitter.push(&data)?);
                    }
                }
                None => {
                    if let Some(splitter) = self.splitter.take() {
                        splitter.finish()?;
                    }
                }
            }
        }
    }
}

impl std::fmt::Debug for SseStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseStream").field("ended", &self.splitter.is_none()).finish_non_exhaustive()
    }
}
