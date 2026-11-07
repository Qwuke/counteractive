//! The storefront's Stripe client against `fake-stripe` over real HTTP.
//!
//! These live here rather than in `fake-stripe` because the storefront owns
//! the HTTP client: the point is that the form it writes and the JSON it
//! reads round-trip through a server with Stripe's shapes.

#![expect(clippy::unwrap_used, reason = "test helpers: a setup step that fails should fail the test at once")]

use std::time::Duration;

use fake_stripe::FakeStripe;
use manifest::Product;
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use secrecy::SecretString;
use serde_json::Value;
use storefront::order::OrderId;
use storefront::stripe::{PayError, PaymentProvider as _, StripeClient, StripeEndpoint};

const SUCCESS_URL: &str = "http://127.0.0.1:7777/paid";
const TIMEOUT: Duration = Duration::from_secs(20);

fn client(stripe: &FakeStripe, key: &str) -> StripeClient {
    let endpoint = StripeEndpoint::new(stripe.base_url().parse().unwrap(), SUCCESS_URL.parse().unwrap()).unwrap();
    StripeClient::new(endpoint, SecretString::from(key)).unwrap()
}

fn usd_5() -> Product {
    Product { id: "usd-5".into(), usd_cents: 500, credits: 5_000_000 }
}

/// A plain HTTP client that doesn't follow redirects, standing in for a browser or `curl`.
fn http() -> reqwest::Client {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(TIMEOUT).build().unwrap()
}

#[tokio::test]
async fn a_created_session_reads_back_with_every_field_the_form_set() {
    let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let client = client(&stripe, "sk_test_1");

    let reference = "ab".repeat(32);
    let checkout = client.create_checkout(&usd_5(), &reference).await.unwrap();
    assert_eq!(checkout.url, format!("{}/pay/{}", stripe.base_url(), checkout.id.as_str()));

    let session = client.get_checkout(&checkout.id).await.unwrap();
    assert_eq!(session.id, checkout.id.as_str());
    assert_eq!(session.status.as_deref(), Some("open"));
    assert_eq!(session.payment_status.as_deref(), Some("unpaid"));
    assert_eq!(session.amount_total, Some(500));
    assert_eq!(session.currency.as_deref(), Some("usd"));
    assert_eq!(session.client_reference_id, Some(reference));
    assert_eq!(session.metadata.product.as_deref(), Some("usd-5"));

    stripe.stop().await.unwrap();
}

#[tokio::test]
async fn an_unknown_session_is_not_found() {
    let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let id = OrderId::parse("cs_test_nothing").unwrap();
    let result = client(&stripe, "sk_test_1").get_checkout(&id).await;
    assert!(matches!(result, Err(PayError::NotFound)), "{result:?}");

    let response = http().post(format!("{}/test/pay/cs_test_nothing", stripe.base_url())).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    stripe.stop().await.unwrap();
}

#[tokio::test]
async fn api_calls_need_a_bearer_key_and_a_complete_form() {
    let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let sessions = format!("{}/v1/checkout/sessions", stripe.base_url());

    let anonymous = http().post(&sessions).form(&[("mode", "payment")]).send().await.unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let body: Value = anonymous.json().await.unwrap();
    assert_eq!(body["error"]["type"], "invalid_request_error");

    let incomplete = http().post(&sessions).bearer_auth("sk_test_1").form(&[("mode", "payment")]).send().await.unwrap();
    assert_eq!(incomplete.status(), StatusCode::BAD_REQUEST);

    let anonymous_get = http().get(format!("{sessions}/cs_test_1")).send().await.unwrap();
    assert_eq!(anonymous_get.status(), StatusCode::UNAUTHORIZED);

    stripe.stop().await.unwrap();
}

#[tokio::test]
async fn the_pay_page_pays_and_returns_the_browser_to_the_wallet() {
    let stripe = FakeStripe::start("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let client = client(&stripe, "sk_test_1");
    let checkout = client.create_checkout(&usd_5(), &"ab".repeat(32)).await.unwrap();

    let page = http().get(&checkout.url).send().await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = page.text().await.unwrap();
    assert!(html.contains("usd-5: 5.00 USD"));
    assert!(html.contains(&format!("action=\"/pay/{}\"", checkout.id.as_str())));

    let pressed = http().post(&checkout.url).send().await.unwrap();
    assert_eq!(pressed.status(), StatusCode::SEE_OTHER);
    assert_eq!(pressed.headers()[LOCATION], SUCCESS_URL);

    let session = client.get_checkout(&checkout.id).await.unwrap();
    assert!(session.is_paid());
    assert_eq!(session.status.as_deref(), Some("complete"));
    assert!(session.url.is_none());

    stripe.stop().await.unwrap();
}
