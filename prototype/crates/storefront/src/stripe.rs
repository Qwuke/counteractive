//! The payment axis: creating and reading Checkout Sessions (SPEC section 6).
//!
//! [`PaymentProvider`] has one implementation, [`StripeClient`], a
//! hand-written client over `reqwest` with a configurable base URL: Stripe's
//! API, or `fake-stripe` in tests, through the same code. It's two calls,
//! and a strict third-party Stripe crate would stop fulfilment on any
//! payload it didn't expect (docs 06 section 3).
//!
//! Responses are read into [`CheckoutSession`], which names only the fields
//! used; everything else, including `customer_details`, is skipped. Bodies
//! are never logged, and no error carries one.

use std::time::Duration;

use async_trait::async_trait;
use manifest::Product;
use proto::stripe::{CURRENCY_USD, CheckoutCreate, CheckoutSession, StripeErrorBody};
use reqwest::header::HeaderValue;
use reqwest::{Method, StatusCode, Url};
use secrecy::{ExposeSecret as _, SecretString};
use serde::de::DeserializeOwned;

use crate::order::OrderId;

/// The Stripe API version requests are pinned to, so response shapes don't
/// change under a running release (docs 06 section 3).
pub const STRIPE_VERSION: &str = "2026-10-28.endive";

/// The largest Stripe response read. Sessions are a few kilobytes.
const MAX_RESPONSE: usize = 256 * 1024;

/// Connecting to Stripe must finish within this.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A whole Stripe call must finish within this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A Checkout Session just created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// The session id, which is the order id.
    pub id: OrderId,
    /// The Checkout page the buyer pays on.
    pub url: String,
}

/// Why a payment provider call failed. No variant carries a response body.
#[derive(Debug, thiserror::Error)]
pub enum PayError {
    /// The provider has no session with this id.
    #[error("no such Checkout Session")]
    NotFound,
    /// The provider couldn't be reached, or the connection failed.
    #[error("payment provider unreachable: {0}")]
    Unreachable(reqwest::Error),
    /// The provider answered with an error status.
    #[error("payment provider refused the request with status {0}")]
    Refused(u16),
    /// The provider's answer was too large, unparseable, or about another session.
    #[error("payment provider sent an unusable response")]
    Malformed,
    /// The HTTP client couldn't be built.
    #[error("payment client setup failed: {0}")]
    Setup(reqwest::Error),
}

/// Creates and reads Checkout Sessions.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// Opens a Checkout Session for one `product`, tagged with `reference`
    /// (hex `SHA-256(order_secret)`).
    async fn create_checkout(&self, product: &Product, reference: &str) -> Result<Checkout, PayError>;

    /// Reads a session's payment state.
    async fn get_checkout(&self, id: &OrderId) -> Result<CheckoutSession, PayError>;
}

/// Where Stripe's API is, and where Checkout sends the buyer after paying.
/// Fixed at startup.
#[derive(Debug, Clone)]
pub struct StripeEndpoint {
    base_url: Url,
    success_url: Url,
}

impl StripeEndpoint {
    /// `base_url` is `https://api.stripe.com` or a `fake-stripe`;
    /// `success_url` is walletd's local `/paid` page.
    pub fn new(base_url: Url, success_url: Url) -> Result<Self, &'static str> {
        for url in [&base_url, &success_url] {
            if !matches!(url.scheme(), "http" | "https") || url.cannot_be_a_base() {
                return Err("Stripe base and success URLs must be http(s) URLs");
            }
        }
        if base_url.scheme() != "https" {
            tracing::warn!("the Stripe base URL is not HTTPS: acceptable only for fake-stripe");
        }
        Ok(Self { base_url, success_url })
    }

    /// The base URL with `segments` appended, each percent-encoded.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base_url.clone();
        url.path_segments_mut().expect("checked to be a base URL").pop_if_empty().extend(segments);
        url
    }
}

/// [`PaymentProvider`] over Stripe's HTTP API. Holds the only copy of the
/// Stripe key, in memory.
pub struct StripeClient {
    http: reqwest::Client,
    endpoint: StripeEndpoint,
    key: SecretString,
}

impl StripeClient {
    /// A client using `key` as its bearer. No request is made yet.
    pub fn new(endpoint: StripeEndpoint, key: SecretString) -> Result<Self, PayError> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // Stripe's API never redirects; refusing keeps the key on one host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(PayError::Setup)?;
        Ok(Self { http, endpoint, key })
    }

    /// `POST /v1/checkout/sessions` with the SPEC's form fields.
    fn create_request(&self, product: &Product, reference: &str) -> Result<reqwest::Request, PayError> {
        let form = CheckoutCreate {
            unit_amount: product.usd_cents,
            currency: CURRENCY_USD.into(),
            product_id: product.id.clone(),
            client_reference_id: reference.into(),
            success_url: self.endpoint.success_url.to_string(),
        };
        self.request(Method::POST, &["v1", "checkout", "sessions"])
            .form(&form.to_form())
            .build()
            .map_err(PayError::Setup)
    }

    /// `GET /v1/checkout/sessions/{id}`.
    fn get_request(&self, id: &OrderId) -> Result<reqwest::Request, PayError> {
        self.request(Method::GET, &["v1", "checkout", "sessions", id.as_str()]).build().map_err(PayError::Setup)
    }

    fn request(&self, method: Method, path: &[&str]) -> reqwest::RequestBuilder {
        self.http
            .request(method, self.endpoint.url(path))
            .bearer_auth(self.key.expose_secret())
            .header("Stripe-Version", HeaderValue::from_static(STRIPE_VERSION))
    }

    async fn send<T: DeserializeOwned>(&self, request: reqwest::Request) -> Result<T, PayError> {
        // Errors are logged, and their URL would name the order.
        let unreachable = |error: reqwest::Error| PayError::Unreachable(error.without_url());
        let mut response = self.http.execute(request).await.map_err(unreachable)?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
            if body.len() + chunk.len() > MAX_RESPONSE {
                return Err(PayError::Malformed);
            }
            body.extend_from_slice(&chunk);
        }
        interpret(status, &body)
    }
}

impl std::fmt::Debug for StripeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeClient").field("endpoint", &self.endpoint).finish_non_exhaustive()
    }
}

#[async_trait]
impl PaymentProvider for StripeClient {
    async fn create_checkout(&self, product: &Product, reference: &str) -> Result<Checkout, PayError> {
        let session: CheckoutSession = self.send(self.create_request(product, reference)?).await?;
        let id = OrderId::parse(&session.id).ok_or(PayError::Malformed)?;
        let url = session.url.ok_or(PayError::Malformed)?;
        Ok(Checkout { id, url })
    }

    async fn get_checkout(&self, id: &OrderId) -> Result<CheckoutSession, PayError> {
        let session: CheckoutSession = self.send(self.get_request(id)?).await?;
        // The order ref the gate keys issuance by is derived from the id asked
        // for, so the answer must be about that session.
        if session.id != id.as_str() {
            return Err(PayError::Malformed);
        }
        Ok(session)
    }
}

/// Turns a Stripe response into a value or an error, without keeping the body.
///
/// Only a 404 whose body says `resource_missing` means the session doesn't
/// exist; any other 404, such as from a wrong base URL, is a refusal, so a
/// misconfiguration never tells a wallet its paid order is gone.
fn interpret<T: DeserializeOwned>(status: StatusCode, body: &[u8]) -> Result<T, PayError> {
    if status.is_success() {
        return serde_json::from_slice(body).map_err(|_| PayError::Malformed);
    }
    let missing = serde_json::from_slice::<StripeErrorBody>(body).is_ok_and(|error| error.is_missing());
    if status == StatusCode::NOT_FOUND && missing {
        return Err(PayError::NotFound);
    }
    Err(PayError::Refused(status.as_u16()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use serde_json::json;

    fn client(base: &str) -> StripeClient {
        let endpoint =
            StripeEndpoint::new(base.parse().unwrap(), "http://127.0.0.1:7777/paid".parse().unwrap()).unwrap();
        StripeClient::new(endpoint, SecretString::from("sk_test_123")).unwrap()
    }

    fn product() -> Product {
        Product { id: "usd-5".into(), usd_cents: 500, credits: 5_000_000 }
    }

    #[test]
    fn create_is_form_encoded_exactly_as_the_spec_says() {
        let request = client("https://api.stripe.com").create_request(&product(), &"ab".repeat(32)).unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.url().as_str(), "https://api.stripe.com/v1/checkout/sessions");
        let headers = request.headers();
        assert_eq!(headers[AUTHORIZATION], "Bearer sk_test_123");
        assert!(headers[AUTHORIZATION].is_sensitive());
        assert_eq!(headers["stripe-version"], STRIPE_VERSION);
        assert_eq!(headers[CONTENT_TYPE], "application/x-www-form-urlencoded");
        let body = std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap();
        let expected = [
            "mode=payment",
            "line_items%5B0%5D%5Bquantity%5D=1",
            "line_items%5B0%5D%5Bprice_data%5D%5Bcurrency%5D=usd",
            "line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=500",
            "line_items%5B0%5D%5Bprice_data%5D%5Bproduct_data%5D%5Bname%5D=usd-5",
            &format!("client_reference_id={}", "ab".repeat(32)),
            "metadata%5Bproduct%5D=usd-5",
            "success_url=http%3A%2F%2F127.0.0.1%3A7777%2Fpaid",
        ]
        .join("&");
        assert_eq!(body, expected);
    }

    #[test]
    fn get_appends_the_id_under_any_base_path() {
        let id = OrderId::parse("cs_test_a1").unwrap();
        for base in ["http://127.0.0.1:9/", "http://127.0.0.1:9"] {
            let request = client(base).get_request(&id).unwrap();
            assert_eq!(request.method(), Method::GET);
            assert_eq!(request.url().as_str(), "http://127.0.0.1:9/v1/checkout/sessions/cs_test_a1");
            assert!(request.body().is_none());
        }
        let request = client("http://127.0.0.1:9/stripe").get_request(&id).unwrap();
        assert_eq!(request.url().path(), "/stripe/v1/checkout/sessions/cs_test_a1");
    }

    #[test]
    fn a_session_is_read_without_its_buyer_details() {
        let body = json!({
            "id": "cs_test_1", "object": "checkout.session", "status": "complete", "payment_status": "paid",
            "amount_total": 500, "currency": "usd", "client_reference_id": "ab",
            "metadata": {"product": "usd-5"}, "url": null,
            "customer_details": {"email": "someone@example.com", "name": "Some One"}
        });
        let session: CheckoutSession = interpret(StatusCode::OK, body.to_string().as_bytes()).unwrap();
        assert!(session.is_paid());
        assert_eq!(session.amount_total, Some(500));
        assert_eq!(session.metadata.product.as_deref(), Some("usd-5"));
        assert!(!format!("{session:?}").contains("example.com"));
    }

    #[test]
    fn errors_map_without_their_bodies() {
        let missing = json!({"error": {"type": "invalid_request_error", "code": "resource_missing",
                                       "message": "No such checkout.session: cs_x"}});
        let result = interpret::<CheckoutSession>(StatusCode::NOT_FOUND, missing.to_string().as_bytes());
        assert!(matches!(result, Err(PayError::NotFound)), "{result:?}");

        let bare_404 = interpret::<CheckoutSession>(StatusCode::NOT_FOUND, b"<html>not here</html>");
        assert!(matches!(bare_404, Err(PayError::Refused(404))), "{bare_404:?}");

        let auth = json!({"error": {"type": "invalid_request_error", "message": "Invalid API Key provided"}});
        let result = interpret::<CheckoutSession>(StatusCode::UNAUTHORIZED, auth.to_string().as_bytes());
        let error = result.unwrap_err();
        assert!(matches!(error, PayError::Refused(401)));
        assert!(!error.to_string().contains("Invalid API Key"));

        let garbled = interpret::<CheckoutSession>(StatusCode::OK, b"{\"id\":");
        assert!(matches!(garbled, Err(PayError::Malformed)));
    }

    #[test]
    fn endpoints_must_be_http_urls() {
        let paid: Url = "http://127.0.0.1:7777/paid".parse().unwrap();
        StripeEndpoint::new("mailto:x@example.com".parse().unwrap(), paid.clone()).unwrap_err();
        StripeEndpoint::new("ftp://example.com".parse().unwrap(), paid).unwrap_err();
    }

    #[test]
    fn debug_never_shows_the_key() {
        let debug = format!("{:?}", client("https://api.stripe.com"));
        assert!(debug.contains("api.stripe.com"));
        assert!(!debug.contains("sk_test"));
    }
}
