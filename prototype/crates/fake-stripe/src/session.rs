//! Checkout Sessions in memory, and the JSON Stripe returns for one.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use proto::stripe::{CheckoutCreate, CheckoutMetadata, CheckoutSession, PAYMENT_STATUS_PAID};
use rand::Rng as _;
use serde::Serialize;

/// What the fake reports as the buyer once a session is paid. Real Stripe
/// returns the buyer's details here; the storefront must never read them.
const BUYER_EMAIL: &str = "buyer@example.invalid";

/// One Checkout Session.
#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) order: CheckoutCreate,
    pub(crate) paid: bool,
}

/// Every session created since start, by id.
#[derive(Debug, Default)]
pub(crate) struct Sessions(Mutex<HashMap<String, Session>>);

impl Sessions {
    /// Stores a new, unpaid session for `order`.
    pub(crate) fn create(&self, order: CheckoutCreate) -> Session {
        let session = Session { id: new_id(), order, paid: false };
        self.lock().insert(session.id.clone(), session.clone());
        session
    }

    /// The session with this id.
    pub(crate) fn get(&self, id: &str) -> Option<Session> {
        self.lock().get(id).cloned()
    }

    /// Marks a session paid, as completing Checkout does. Paying twice is harmless.
    pub(crate) fn pay(&self, id: &str) -> Option<Session> {
        let mut sessions = self.lock();
        let session = sessions.get_mut(id)?;
        session.paid = true;
        Some(session.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A Stripe-style id: `cs_test_` and random alphanumerics.
fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    format!("cs_test_{}", hex::encode(bytes))
}

/// A session as Stripe's API returns it.
///
/// Besides the fields the storefront reads ([`CheckoutSession`]), it
/// carries some it must ignore, notably `customer_details`, so tests show
/// the storefront's parser skips them.
#[derive(Debug, Serialize)]
pub(crate) struct SessionJson<'a> {
    #[serde(flatten)]
    session: CheckoutSession,
    object: &'static str,
    mode: &'static str,
    livemode: bool,
    success_url: &'a str,
    customer_details: Option<CustomerDetails>,
}

#[derive(Debug, Serialize)]
struct CustomerDetails {
    email: &'static str,
}

impl Session {
    /// The JSON for this session. `base_url` is where this fake serves its pay page.
    pub(crate) fn to_json<'a>(&'a self, base_url: &str) -> SessionJson<'a> {
        // Like Stripe, a completed session no longer offers its Checkout page.
        let (status, payment_status, url) = if self.paid {
            ("complete", PAYMENT_STATUS_PAID, None)
        } else {
            ("open", "unpaid", Some(format!("{base_url}/pay/{}", self.id)))
        };
        SessionJson {
            session: CheckoutSession {
                id: self.id.clone(),
                status: Some(status.into()),
                payment_status: Some(payment_status.into()),
                amount_total: Some(self.order.unit_amount),
                currency: Some(self.order.currency.clone()),
                client_reference_id: Some(self.order.client_reference_id.clone()),
                metadata: CheckoutMetadata { product: Some(self.order.product_id.clone()) },
                url,
            },
            object: "checkout.session",
            mode: "payment",
            livemode: false,
            success_url: &self.order.success_url,
            customer_details: self.paid.then_some(CustomerDetails { email: BUYER_EMAIL }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::stripe::CURRENCY_USD;
    use serde_json::{Value, json};

    fn order() -> CheckoutCreate {
        CheckoutCreate {
            unit_amount: 500,
            currency: CURRENCY_USD.into(),
            product_id: "usd-5".into(),
            client_reference_id: "ab".repeat(32),
            success_url: "http://127.0.0.1:7777/paid".into(),
        }
    }

    #[test]
    fn a_new_session_is_open_with_a_pay_page() {
        let sessions = Sessions::default();
        let session = sessions.create(order());
        assert!(session.id.starts_with("cs_test_"));
        assert!(session.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let value = serde_json::to_value(session.to_json("http://fake")).unwrap();
        assert_eq!(
            value,
            json!({
                "id": session.id, "object": "checkout.session", "mode": "payment", "livemode": false,
                "status": "open", "payment_status": "unpaid", "amount_total": 500, "currency": "usd",
                "client_reference_id": "ab".repeat(32), "metadata": {"product": "usd-5"},
                "success_url": "http://127.0.0.1:7777/paid",
                "url": format!("http://fake/pay/{}", session.id), "customer_details": null
            })
        );
    }

    #[test]
    fn paying_completes_the_session_and_adds_buyer_details() {
        let sessions = Sessions::default();
        let id = sessions.create(order()).id;
        assert!(sessions.pay("cs_test_unknown").is_none());
        let paid = sessions.pay(&id).unwrap();
        assert!(sessions.get(&id).unwrap().paid);
        let value = serde_json::to_value(paid.to_json("http://fake")).unwrap();
        assert_eq!(value["status"], "complete");
        assert_eq!(value["payment_status"], "paid");
        assert_eq!(value["url"], Value::Null);
        assert_eq!(value["customer_details"]["email"], BUYER_EMAIL);

        // The storefront's view of the same JSON drops the buyer.
        let read: CheckoutSession = serde_json::from_value(value).unwrap();
        assert!(read.is_paid());
        assert!(!serde_json::to_string(&read).unwrap().contains(BUYER_EMAIL));
    }

    #[test]
    fn ids_are_unique() {
        let sessions = Sessions::default();
        assert_ne!(sessions.create(order()).id, sessions.create(order()).id);
    }
}
