//! Connections that can only carry secrets after attestation (SPEC section 4).
//!
//! A [`Dialer`] opens a byte stream to a [`Target`]. [`UnverifiedConn`]
//! runs inner TLS over it, recording the server's SPKI hash, and can fetch
//! health and the manifest. [`UnverifiedConn::attest`] checks the server's
//! evidence against that SPKI, the expected role and a fresh nonce, and is
//! the only way to get an [`AttestedConn`], which can send anything.
//!
//! Features pick dialers: `tcp` for `wallet-cli` and `xtask`, `arti` for
//! `walletd`. `server` adds the server half for the gate and storefront.

#[cfg(feature = "arti")]
mod arti;
mod cache;
mod conn;
mod dialer;
#[cfg(feature = "server")]
pub mod server;
mod target;
mod tls;

#[cfg(feature = "arti")]
pub use arti::ArtiDialer;
pub use cache::{DEFAULT_VERDICT_TTL, VerdictCache};
pub use conn::{ApiRequest, AttestFailure, AttestedConn, ConnError, HANDSHAKE_TIMEOUT, SseStream, UnverifiedConn};
#[cfg(feature = "tcp")]
pub use dialer::TcpDialer;
pub use dialer::{BoxedIo, DialError, DialScope, Dialer, Io, UnixDialer};
pub use target::{Target, TargetError};
pub use tls::{server_name, spki_sha256};
