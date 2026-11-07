//! Dialers: the only part of a connection that differs between Unix, TCP and Tor.
//!
//! Everything above a dialer (TLS, attestation, HTTP) is shared, so tier 1
//! tests over a Unix socket exercise the same code `walletd` runs over Tor.

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::Target;

/// A byte stream a dialer returns.
pub trait Io: AsyncRead + AsyncWrite + Send + Unpin + 'static {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Io for T {}

/// A boxed stream, so connections don't depend on which dialer made them.
pub type BoxedIo = Box<dyn Io>;

/// Opens byte streams to targets.
#[async_trait]
pub trait Dialer: Send + Sync {
    /// Opens a stream. `scope` groups streams that may share a Tor circuit.
    async fn dial(&self, target: &Target, scope: &DialScope) -> Result<BoxedIo, DialError>;
}

/// Which streams may share a circuit: one scope per conversation.
///
/// Clones share the scope; [`DialScope::new`] starts a fresh one. Dialers
/// other than Arti ignore it.
#[derive(Debug, Clone)]
pub struct DialScope {
    #[cfg(feature = "arti")]
    pub(crate) isolation: arti_client::IsolationToken,
}

impl DialScope {
    /// A scope that shares nothing with any other.
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "arti")]
            isolation: arti_client::IsolationToken::new(),
        }
    }
}

impl Default for DialScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a dial failed.
#[derive(Debug, thiserror::Error)]
pub enum DialError {
    /// This dialer can't reach this kind of target.
    #[error("this dialer cannot reach {0} targets")]
    UnsupportedTarget(&'static str),
    /// The operating system refused the connection.
    #[error("connect failed: {0}")]
    Io(#[from] std::io::Error),
    /// Tor couldn't bootstrap or open the stream.
    #[cfg(feature = "arti")]
    #[error("tor: {0}")]
    Tor(#[from] arti_client::Error),
}

/// Dials Unix sockets: the gate's `gate.sock` in tier 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnixDialer;

#[async_trait]
impl Dialer for UnixDialer {
    async fn dial(&self, target: &Target, _: &DialScope) -> Result<BoxedIo, DialError> {
        match target {
            Target::Unix(path) => Ok(Box::new(tokio::net::UnixStream::connect(path).await?)),
            other => Err(DialError::UnsupportedTarget(other.kind())),
        }
    }
}

/// Dials TCP: the storefront, from `wallet-cli` and `xtask`. Never in `walletd`.
#[cfg(feature = "tcp")]
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpDialer;

#[cfg(feature = "tcp")]
#[async_trait]
impl Dialer for TcpDialer {
    async fn dial(&self, target: &Target, _: &DialScope) -> Result<BoxedIo, DialError> {
        match target {
            Target::Tcp { host, port } => {
                let stream = tokio::net::TcpStream::connect((host.as_str(), *port)).await?;
                stream.set_nodelay(true)?;
                Ok(Box::new(stream))
            }
            other => Err(DialError::UnsupportedTarget(other.kind())),
        }
    }
}
