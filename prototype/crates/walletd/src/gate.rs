//! The gate over attested TLS (docs 04 section 6): dial, attest, then the
//! four use-plane calls. Nothing secret is sent before attestation, because
//! only an [`AttestedConn`] can send a request with a body.

use attest::{PlatformVerifier, Role};
use gate_conn::{
    ApiRequest, AttestFailure, AttestedConn, ConnError, DialScope, Dialer, SseStream, Target, UnverifiedConn,
    VerdictCache,
};
use manifest::{Manifest, SignedManifest};
use proto::gate::{
    CloseResponse, OpenSessionRequest, OpenSessionResponse, RecoverRequest, RecoverResponse, SessionChatRequest,
};
use proto::{SecretBlob, SessionId};

/// Dials and attests the gate.
pub struct GateClient {
    dialer: Box<dyn Dialer>,
    target: Target,
    verifier: PlatformVerifier,
    cache: VerdictCache,
}

impl GateClient {
    /// A client for `target`. `allow_dev_attestation` comes from the pins.
    pub fn new(dialer: Box<dyn Dialer>, target: Target, allow_dev_attestation: bool) -> Self {
        Self { dialer, target, verifier: PlatformVerifier::new(allow_dev_attestation), cache: VerdictCache::new() }
    }

    /// A new attested connection in `scope`, checked against `manifest`.
    /// A verdict for the same TLS key is reused for up to an hour.
    ///
    /// If the dial, the TLS handshake or the evidence fetch fails in
    /// transit, it redials once in a fresh scope, which over Tor means a
    /// fresh circuit: a stalled circuit stalls every stream on it, so the
    /// same scope would likely stall again. The failed connection carried
    /// no request but the public evidence fetch, so the gate saw no spend
    /// and no turn on it, and whatever the caller sends next goes once, on
    /// the new connection. The link keeps the fresh scope from then on.
    /// An attestation that is rejected is never retried.
    pub async fn connect(&self, manifest: &Manifest, scope: DialScope) -> Result<Link, AttestFailure> {
        match self.attested(manifest, &scope).await {
            Ok(conn) => Ok(Link { conn: Some(conn), scope }),
            Err(AttestFailure::Conn(error)) if in_transit(&error) => {
                tracing::info!(%error, "connecting to the gate failed; redialling once on a fresh circuit");
                let scope = DialScope::new();
                let conn = self.attested(manifest, &scope).await?;
                Ok(Link { conn: Some(conn), scope })
            }
            Err(failure) => Err(failure),
        }
    }

    /// Dials in `scope`, runs inner TLS and attests the gate.
    async fn attested(&self, manifest: &Manifest, scope: &DialScope) -> Result<AttestedConn, AttestFailure> {
        let conn = UnverifiedConn::connect(self.dialer.as_ref(), &self.target, Role::Gate, scope).await?;
        conn.attest_cached(&self.verifier, manifest, &self.cache).await
    }

    /// Reconnects `link` in its own scope if the gate closed it (in a
    /// fresh scope if that dial fails in transit, as [`GateClient::connect`]
    /// explains).
    pub async fn ensure_connected(&self, link: &mut Link, manifest: &Manifest) -> Result<(), AttestFailure> {
        if link.conn.as_ref().is_none_or(AttestedConn::is_closed) {
            *link = self.connect(manifest, link.scope.clone()).await?;
        }
        Ok(())
    }

    /// The gate's signed manifest, fetched over a fresh, unattested
    /// connection. The caller runs the accept rule; the user approves.
    /// Like [`GateClient::connect`], a connection that fails in transit
    /// before the request is redialled once on a fresh circuit.
    pub async fn fetch_manifest(&self) -> Result<SignedManifest, ConnError> {
        let mut conn = match self.unverified().await {
            Err(error) if in_transit(&error) => {
                tracing::info!(%error, "connecting to the gate failed; redialling once on a fresh circuit");
                self.unverified().await?
            }
            conn => conn?,
        };
        conn.fetch_manifest().await
    }

    /// Dials in a fresh scope and runs inner TLS.
    async fn unverified(&self) -> Result<UnverifiedConn, ConnError> {
        UnverifiedConn::connect(self.dialer.as_ref(), &self.target, Role::Gate, &DialScope::new()).await
    }
}

/// Whether a connection failed on the way, as a stalled or broken circuit
/// fails, rather than because the gate answered or refused something.
fn in_transit(error: &ConnError) -> bool {
    matches!(error, ConnError::Dial(_) | ConnError::HandshakeTimeout | ConnError::Tls(_) | ConnError::Http(_))
}

/// One attested connection and the dial scope it belongs to. The
/// connection may be gone, after which the link must be reconnected.
pub struct Link {
    conn: Option<AttestedConn>,
    scope: DialScope,
}

impl Link {
    /// The session limits and catalogue of the release the gate proved it runs.
    pub fn release(&self) -> Option<&manifest::ReleaseConfig> {
        self.conn.as_ref().map(|conn| &conn.verdict().release_config)
    }

    /// Drops the connection after a transport failure, so the next use reconnects.
    pub fn drop_connection(&mut self) {
        self.conn = None;
    }

    /// `POST /v1/sessions`, with the journalled spend bytes.
    pub async fn open(&mut self, spend: &SecretBlob) -> Result<OpenSessionResponse, ConnError> {
        let request = ApiRequest::post("/v1/sessions").json(&OpenSessionRequest { spend: spend.clone() })?;
        self.conn()?.send_json(request).await
    }

    /// `POST /v1/sessions/{id}/chat`: the turn's event stream.
    pub async fn chat(&mut self, id: &SessionId, request: &SessionChatRequest) -> Result<SseStream, ConnError> {
        let request = ApiRequest::post(format!("/v1/sessions/{}/chat", id.to_hex())).json(request)?;
        self.conn()?.send_sse(request).await
    }

    /// `POST /v1/sessions/{id}/close`.
    pub async fn close(&mut self, id: &SessionId) -> Result<CloseResponse, ConnError> {
        self.conn()?.send_json(ApiRequest::post(format!("/v1/sessions/{}/close", id.to_hex()))).await
    }

    /// `POST /v1/sessions/recover`, with the journalled spend bytes.
    pub async fn recover(&mut self, spend: &SecretBlob) -> Result<RecoverResponse, ConnError> {
        let request = ApiRequest::post("/v1/sessions/recover").json(&RecoverRequest { spend: spend.clone() })?;
        self.conn()?.send_json(request).await
    }

    fn conn(&mut self) -> Result<&mut AttestedConn, ConnError> {
        self.conn.as_mut().ok_or(ConnError::Protocol("the connection to the gate was dropped"))
    }
}
