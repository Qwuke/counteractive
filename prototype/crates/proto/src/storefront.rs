//! The storefront's API over inner TLS (SPEC section 6).
//!
//! `GET /v1/manifest` and `POST /v1/attestation` share the gate's types.

use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, Serializer};

use crate::encoding::{Blob, SecretHex32, hex_array};

/// Whether the storefront can take orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorefrontState {
    /// No Stripe key yet.
    Unconfigured,
    /// Taking orders.
    Ready,
}

/// `GET /v1/health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorefrontHealth {
    /// Whether a Stripe key is set.
    pub state: StorefrontState,
}

/// `POST /v1/orders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    /// A product id from the release.
    pub product: String,
    /// `SHA-256(order_secret)`, stored at Stripe as `client_reference_id`.
    #[serde(with = "hex_array")]
    pub secret_hash: [u8; 32],
}

/// The answer to `POST /v1/orders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOrderResponse {
    /// The Checkout Session id.
    pub order_id: String,
    /// Where the user pays.
    pub checkout_url: String,
}

/// `POST /v1/orders/{id}/collect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectRequest {
    /// The secret whose hash the order was created with.
    pub order_secret: SecretHex32,
    /// The wallet's issue request, sent unchanged on every retry.
    pub issue_request: Blob,
}

/// The answer to `POST /v1/orders/{id}/collect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectResponse {
    /// The gate's issue response.
    pub issue_response: Blob,
    /// The credits it carries.
    pub credits: u64,
}

/// `POST /v1/operator/configure`, with the storefront token as bearer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureRequest {
    /// The Stripe secret key, held in memory only.
    #[serde(serialize_with = "expose_string")]
    pub stripe_key: SecretString,
}

/// Exposes a secret string deliberately: serialising is how it goes on the wire.
fn expose_string<S: Serializer>(secret: &SecretString, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// The answer to `POST /v1/operator/configure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigureResponse {
    /// State after configuring.
    pub state: StorefrontState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn order_shapes() {
        let request = CreateOrderRequest { product: "usd-5".into(), secret_hash: [0xcd; 32] };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"product": "usd-5", "secret_hash": "cd".repeat(32)})
        );
    }

    #[test]
    fn collect_request_round_trips_and_hides_its_secret() {
        let request =
            CollectRequest { order_secret: SecretHex32::new([0x42; 32]), issue_request: Blob::new(vec![1, 2]) };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value, json!({"order_secret": "42".repeat(32), "issue_request": "AQI"}));
        let back: CollectRequest = serde_json::from_value(value).unwrap();
        assert_eq!(back.order_secret.expose(), &[0x42; 32]);
        assert!(!format!("{back:?}").contains("4242"));
    }

    #[test]
    fn stripe_key_serialises_but_never_debugs() {
        let request = ConfigureRequest { stripe_key: SecretString::from("sk_test_abc") };
        assert_eq!(serde_json::to_string(&request).unwrap(), r#"{"stripe_key":"sk_test_abc"}"#);
        assert!(!format!("{request:?}").contains("sk_test"));
        let back: ConfigureRequest = serde_json::from_str(r#"{"stripe_key":"sk_test_x"}"#).unwrap();
        assert_eq!(back.stripe_key.expose_secret(), "sk_test_x");
    }
}
