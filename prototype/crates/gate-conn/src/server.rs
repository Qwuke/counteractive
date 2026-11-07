//! The server half shared by the gate and the storefront (SPEC section 4).
//!
//! Each boot generates a fresh key and self-signed certificate in memory.
//! Nothing persists, so the only thing that vouches for the key is the
//! attestation report binding its SPKI hash.

use std::fmt::Write as _;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use attest::{ReportBinding, Role};
use axum::Router;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use rustls::server::NoServerSessionStorage;
use rustls_pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::tls::{self, ALPN_HTTP1};

/// How long the accept loop waits after a failed accept, such as when out
/// of file descriptors, so it doesn't spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Why a server identity couldn't be created.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// Key or certificate generation failed.
    #[error("certificate generation failed: {0}")]
    Certificate(#[from] rcgen::Error),
    /// rustls refused the key or certificate.
    #[error("TLS configuration failed: {0}")]
    Tls(#[from] rustls::Error),
    /// The generated certificate couldn't be parsed back.
    #[error("generated certificate is unreadable")]
    Spki,
}

/// This boot's TLS key and certificate, and the role they serve.
pub struct ServerIdentity {
    role: Role,
    spki_sha256: [u8; 32],
    config: Arc<ServerConfig>,
}

impl ServerIdentity {
    /// A fresh P-256 key and self-signed certificate, held only in memory.
    pub fn generate(role: Role) -> Result<Self, IdentityError> {
        let certified = rcgen::generate_simple_self_signed(vec![tls::host_name(role).to_owned()])?;
        let certificate = certified.cert.der().clone();
        let spki_sha256 = tls::spki_sha256(&certificate).ok_or(IdentityError::Spki)?;
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der()));

        let mut config = ServerConfig::builder_with_provider(tls::provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)?;
        // No tickets and no session cache: nothing links two connections.
        config.session_storage = Arc::new(NoServerSessionStorage {});
        config.send_tls13_tickets = 0;
        config.alpn_protocols = vec![ALPN_HTTP1.to_vec()];
        Ok(Self { role, spki_sha256, config: Arc::new(config) })
    }

    /// The role this server attests to.
    pub fn role(&self) -> Role {
        self.role
    }

    /// SHA-256 of the certificate's SPKI, as clients will compute it.
    pub fn spki_sha256(&self) -> [u8; 32] {
        self.spki_sha256
    }

    /// What a report for a client's `nonce` must bind to.
    pub fn binding(&self, nonce: [u8; 32]) -> ReportBinding {
        ReportBinding::new(self.role, self.spki_sha256, nonce)
    }

    /// An acceptor for this identity, for [`serve_unix`] or [`serve_tcp`].
    pub fn tls_acceptor(&self) -> TlsAcceptor {
        TlsAcceptor::from(Arc::clone(&self.config))
    }
}

impl std::fmt::Debug for ServerIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerIdentity")
            .field("role", &self.role)
            .field("spki_sha256", &hex_short(&self.spki_sha256))
            .finish_non_exhaustive()
    }
}

fn hex_short(bytes: &[u8; 32]) -> String {
    bytes[..8].iter().fold(String::with_capacity(16), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Limits for a serve loop.
#[derive(Debug, Clone, Copy)]
pub struct ServeOptions {
    /// SPEC section 4: a TLS handshake must finish within this.
    pub handshake_timeout: Duration,
    /// A request's headers must arrive within this.
    pub header_read_timeout: Duration,
    /// Connections served at once; further accepts wait.
    pub max_connections: usize,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            header_read_timeout: Duration::from_secs(30),
            max_connections: 256,
        }
    }
}

/// Serves `router` on a Unix socket until `shutdown` fires, then lets open
/// requests finish. With `tls` set it is inner TLS (`gate.sock`); without,
/// plain HTTP (`issue.sock`).
pub async fn serve_unix(
    listener: UnixListener,
    tls: Option<TlsAcceptor>,
    router: Router,
    options: ServeOptions,
    shutdown: CancellationToken,
) {
    serve(listener, tls, router, options, shutdown).await;
}

/// Serves `router` on TCP until `shutdown` fires, then lets open requests
/// finish. The storefront always passes `tls`.
pub async fn serve_tcp(
    listener: TcpListener,
    tls: Option<TlsAcceptor>,
    router: Router,
    options: ServeOptions,
    shutdown: CancellationToken,
) {
    serve(listener, tls, router, options, shutdown).await;
}

/// The two listener kinds, so one loop serves both.
trait Listener: Send + Sync {
    type Io: AsyncRead + AsyncWrite + Send + Unpin + 'static;
    fn next_conn(&self) -> impl Future<Output = std::io::Result<Self::Io>> + Send;
}

impl Listener for UnixListener {
    type Io = UnixStream;
    async fn next_conn(&self) -> std::io::Result<UnixStream> {
        Ok(self.accept().await?.0)
    }
}

impl Listener for TcpListener {
    type Io = TcpStream;
    async fn next_conn(&self) -> std::io::Result<TcpStream> {
        let (stream, _) = self.accept().await?;
        stream.set_nodelay(true)?;
        Ok(stream)
    }
}

async fn serve<L: Listener>(
    listener: L,
    tls: Option<TlsAcceptor>,
    router: Router,
    options: ServeOptions,
    shutdown: CancellationToken,
) {
    let tasks = TaskTracker::new();
    let slots = Arc::new(Semaphore::new(options.max_connections));
    loop {
        let permit = tokio::select! {
            () = shutdown.cancelled() => break,
            permit = Arc::clone(&slots).acquire_owned() => permit.expect("the semaphore is never closed"),
        };
        let io = tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.next_conn() => match accepted {
                Ok(io) => io,
                Err(error) => {
                    tracing::warn!(%error, "accept failed");
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
            },
        };
        let (tls, router, shutdown) = (tls.clone(), router.clone(), shutdown.clone());
        tasks.spawn(async move {
            serve_connection(io, tls, router, options, shutdown).await;
            drop(permit);
        });
    }
    tasks.close();
    tasks.wait().await;
}

async fn serve_connection<I>(
    io: I,
    tls: Option<TlsAcceptor>,
    router: Router,
    options: ServeOptions,
    shutdown: CancellationToken,
) where
    I: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let Some(acceptor) = tls else {
        return serve_http(io, router, options, shutdown).await;
    };
    match tokio::time::timeout(options.handshake_timeout, acceptor.accept(io)).await {
        Ok(Ok(stream)) => serve_http(stream, router, options, shutdown).await,
        Ok(Err(error)) => tracing::debug!(%error, "TLS handshake failed"),
        Err(_) => tracing::debug!("TLS handshake timed out"),
    }
}

/// One HTTP/1.1 connection. On shutdown, an idle connection closes at once
/// and a busy one after its current response.
async fn serve_http<I>(io: I, router: Router, options: ServeOptions, shutdown: CancellationToken)
where
    I: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder.timer(TokioTimer::new()).header_read_timeout(options.header_read_timeout);
    let connection = builder.serve_connection(TokioIo::new(io), TowerToHyperService::new(router));
    let mut connection = std::pin::pin!(connection);
    let result = tokio::select! {
        result = connection.as_mut() => result,
        () = shutdown.cancelled() => {
            connection.as_mut().graceful_shutdown();
            connection.await
        }
    };
    if let Err(error) = result {
        tracing::debug!(%error, "connection ended with an error");
    }
}
