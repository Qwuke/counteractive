//! The storefront over inner TLS on the TCP dialer: the manifest before
//! attestation, then orders and collects on an attested connection only,
//! because a collect carries the order secret.

use attest::{PlatformVerifier, Role};
use gate_conn::{ApiRequest, AttestFailure, AttestedConn, ConnError, DialScope, Target, TcpDialer, UnverifiedConn};
use manifest::{Manifest, SignedManifest};
use proto::storefront::{CollectRequest, CollectResponse, CreateOrderRequest, CreateOrderResponse};
use proto::{Blob, SecretHex32};

/// An attested connection to the storefront.
pub struct Storefront {
    conn: AttestedConn,
}

impl Storefront {
    /// The storefront's signed manifest, over an unattested connection.
    /// The accept rule and the user's approval decide what it is worth.
    pub async fn fetch_manifest(target: &Target) -> Result<SignedManifest, ConnError> {
        let mut conn = UnverifiedConn::connect(&TcpDialer, target, Role::Storefront, &DialScope::new()).await?;
        conn.fetch_manifest().await
    }

    /// Dials and attests the storefront against the accepted manifest.
    pub async fn connect(target: &Target, manifest: &Manifest, allow_dev: bool) -> Result<Self, AttestFailure> {
        let conn = UnverifiedConn::connect(&TcpDialer, target, Role::Storefront, &DialScope::new()).await?;
        let conn = conn.attest(&PlatformVerifier::new(allow_dev), manifest).await?;
        Ok(Self { conn })
    }

    /// `POST /v1/orders`.
    pub async fn create_order(
        &mut self,
        product: &str,
        secret_hash: [u8; 32],
    ) -> Result<CreateOrderResponse, ConnError> {
        let body = CreateOrderRequest { product: product.to_owned(), secret_hash };
        self.conn.send_json(ApiRequest::post("/v1/orders").json(&body)?).await
    }

    /// `POST /v1/orders/{id}/collect`. `order_id` must already be checked
    /// with [`is_order_id`], since it goes into the path.
    pub async fn collect(
        &mut self,
        order_id: &str,
        order_secret: [u8; 32],
        issue_request: &[u8],
    ) -> Result<CollectResponse, ConnError> {
        let body = CollectRequest {
            order_secret: SecretHex32::new(order_secret),
            issue_request: Blob::new(issue_request.to_vec()),
        };
        self.conn.send_json(ApiRequest::post(format!("/v1/orders/{order_id}/collect")).json(&body)?).await
    }
}

/// Whether an order id from the storefront is safe to put in a path: a
/// Checkout Session id is ASCII letters, digits and underscores.
pub fn is_order_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 255 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
