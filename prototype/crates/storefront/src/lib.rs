//! The storefront: the CVM's purchase service (ARCHITECTURE section 10,
//! SPEC section 6).
//!
//! It creates Stripe Checkout Sessions, asks Stripe whether one is paid,
//! and then asks the gate, over `issue.sock`, to issue an EC-ACT credential
//! against the wallet's blinded issue request. Wallets reach it over TCP
//! with inner TLS, attested with role `storefront`.
//!
//! It stores nothing. Stripe holds the order, and the gate holds the
//! issuance record that makes a retry exact. The Stripe key arrives from
//! the operator after each start and lives only in memory.
//!
//! - [`config`]: `release.json`, the dev attestation key, the operator token check
//! - [`tls`]: this boot's TLS key and its evidence
//! - [`stripe`]: [`stripe::PaymentProvider`] and its Stripe client
//! - [`order`]: the pure rules an order must pass
//! - [`gate`]: the `issue.sock` client
//! - [`api`]: the endpoints

pub mod api;
pub mod config;
pub mod gate;
pub mod order;
pub mod stripe;
pub mod tls;

use std::sync::Arc;

use gate_conn::server::{IdentityError, ServeOptions, serve_tcp};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::api::AppState;
use crate::config::Settings;
use crate::gate::IssueClient;
use crate::tls::Attestor;

/// A storefront ready to serve: settings loaded, this boot's TLS key made.
pub struct Storefront {
    state: Arc<AppState>,
}

impl Storefront {
    /// Generates this boot's TLS identity. The storefront starts
    /// `unconfigured`, with no Stripe key.
    pub fn new(settings: Settings) -> Result<Self, IdentityError> {
        let Settings { release, attestation, issue_socket, stripe, manifest_dir } = settings;
        let attestor = Attestor::generate(attestation)?;
        let state = AppState::new(release, attestor, stripe, manifest_dir, IssueClient::new(issue_socket));
        Ok(Self { state: Arc::new(state) })
    }

    /// SHA-256 of this boot's TLS SPKI, the value its reports bind.
    pub fn spki_sha256(&self) -> [u8; 32] {
        self.state.attestor().identity().spki_sha256()
    }

    /// Serves inner TLS on `listener` until `shutdown` fires, then lets open
    /// requests finish.
    pub async fn serve(self, listener: TcpListener, shutdown: CancellationToken) {
        let acceptor = self.state.attestor().identity().tls_acceptor();
        serve_tcp(listener, Some(acceptor), api::router(self.state), ServeOptions::default(), shutdown).await;
    }
}
