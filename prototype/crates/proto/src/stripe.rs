//! The two Stripe Checkout calls the storefront makes (SPEC section 6).
//!
//! The storefront writes them and `fake-stripe` reads them, so both sides
//! share one definition of the form fields. Responses are read through
//! structs naming only the fields used: everything else Stripe sends,
//! including `customer_details`, is skipped without being stored.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The `payment_status` of a paid Checkout Session.
pub const PAYMENT_STATUS_PAID: &str = "paid";

/// The only currency products are priced in.
pub const CURRENCY_USD: &str = "usd";

/// `POST /v1/checkout/sessions`: what to put in the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutCreate {
    /// Price in cents.
    pub unit_amount: u64,
    /// Lowercase ISO currency, `usd`.
    pub currency: String,
    /// The product id, shown on the Checkout page and kept in metadata.
    pub product_id: String,
    /// Hex `SHA-256(order_secret)`.
    pub client_reference_id: String,
    /// Where Checkout returns to: walletd's static `/paid` page.
    pub success_url: String,
}

/// A form field that was missing or malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("missing or invalid form field `{0}`")]
pub struct FormError(pub &'static str);

impl CheckoutCreate {
    const MODE: &'static str = "mode";
    const QUANTITY: &'static str = "line_items[0][quantity]";
    const CURRENCY: &'static str = "line_items[0][price_data][currency]";
    const UNIT_AMOUNT: &'static str = "line_items[0][price_data][unit_amount]";
    const NAME: &'static str = "line_items[0][price_data][product_data][name]";
    const REFERENCE: &'static str = "client_reference_id";
    const PRODUCT: &'static str = "metadata[product]";
    const SUCCESS_URL: &'static str = "success_url";

    /// The form fields, in Stripe's bracket notation.
    pub fn to_form(&self) -> Vec<(&'static str, String)> {
        vec![
            (Self::MODE, "payment".into()),
            (Self::QUANTITY, "1".into()),
            (Self::CURRENCY, self.currency.clone()),
            (Self::UNIT_AMOUNT, self.unit_amount.to_string()),
            (Self::NAME, self.product_id.clone()),
            (Self::REFERENCE, self.client_reference_id.clone()),
            (Self::PRODUCT, self.product_id.clone()),
            (Self::SUCCESS_URL, self.success_url.clone()),
        ]
    }

    /// Reads the fields back, as `fake-stripe` does. Only one-item payment-mode forms parse.
    pub fn from_form(form: &HashMap<String, String>) -> Result<Self, FormError> {
        let field = |name: &'static str| form.get(name).filter(|v| !v.is_empty()).ok_or(FormError(name));
        if field(Self::MODE)? != "payment" {
            return Err(FormError(Self::MODE));
        }
        if field(Self::QUANTITY)? != "1" {
            return Err(FormError(Self::QUANTITY));
        }
        let product_id = field(Self::PRODUCT)?.clone();
        if field(Self::NAME)? != &product_id {
            return Err(FormError(Self::NAME));
        }
        Ok(Self {
            unit_amount: field(Self::UNIT_AMOUNT)?.parse().map_err(|_| FormError(Self::UNIT_AMOUNT))?,
            currency: field(Self::CURRENCY)?.clone(),
            product_id,
            client_reference_id: field(Self::REFERENCE)?.clone(),
            success_url: field(Self::SUCCESS_URL)?.clone(),
        })
    }
}

/// The metadata the storefront sets.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CheckoutMetadata {
    /// The product id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
}

/// A Checkout Session, as created or retrieved. Only these fields are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutSession {
    /// `cs_...`, used as the order id.
    pub id: String,
    /// `open`, `complete` or `expired`.
    #[serde(default)]
    pub status: Option<String>,
    /// `paid` once the payment succeeded.
    #[serde(default)]
    pub payment_status: Option<String>,
    /// Total in the smallest currency unit.
    #[serde(default)]
    pub amount_total: Option<u64>,
    /// Lowercase ISO currency.
    #[serde(default)]
    pub currency: Option<String>,
    /// Hex `SHA-256(order_secret)`.
    #[serde(default)]
    pub client_reference_id: Option<String>,
    /// Holds the product id.
    #[serde(default)]
    pub metadata: CheckoutMetadata,
    /// The Checkout page; set on create.
    #[serde(default)]
    pub url: Option<String>,
}

impl CheckoutSession {
    /// Whether Stripe reports the payment complete.
    pub fn is_paid(&self) -> bool {
        self.payment_status.as_deref() == Some(PAYMENT_STATUS_PAID)
    }
}

/// Stripe's error envelope. Its message is never logged or forwarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StripeErrorBody {
    /// The error.
    pub error: StripeError,
}

/// One Stripe error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StripeError {
    /// `invalid_request_error`, `authentication_error`, ...
    #[serde(rename = "type")]
    pub kind: String,
    /// `resource_missing` for an unknown session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl StripeErrorBody {
    /// The code Stripe uses for an unknown id.
    pub const RESOURCE_MISSING: &'static str = "resource_missing";

    /// An error of `kind` with an optional `code`.
    pub fn new(kind: &str, code: Option<&str>) -> Self {
        Self { error: StripeError { kind: kind.into(), code: code.map(Into::into) } }
    }

    /// Whether this says the resource doesn't exist.
    pub fn is_missing(&self) -> bool {
        self.error.code.as_deref() == Some(Self::RESOURCE_MISSING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> CheckoutCreate {
        CheckoutCreate {
            unit_amount: 500,
            currency: CURRENCY_USD.into(),
            product_id: "usd-5".into(),
            client_reference_id: "ab".repeat(32),
            success_url: "http://127.0.0.1:7777/paid".into(),
        }
    }

    #[test]
    fn form_has_the_spec_fields() {
        let form: HashMap<String, String> = sample().to_form().into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        assert_eq!(form["mode"], "payment");
        assert_eq!(form["line_items[0][quantity]"], "1");
        assert_eq!(form["line_items[0][price_data][currency]"], "usd");
        assert_eq!(form["line_items[0][price_data][unit_amount]"], "500");
        assert_eq!(form["line_items[0][price_data][product_data][name]"], "usd-5");
        assert_eq!(form["metadata[product]"], "usd-5");
        assert_eq!(CheckoutCreate::from_form(&form).unwrap(), sample());
    }

    #[test]
    fn from_form_rejects_missing_or_odd_fields() {
        let mut form: HashMap<String, String> =
            sample().to_form().into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        form.insert("mode".into(), "subscription".into());
        assert_eq!(CheckoutCreate::from_form(&form), Err(FormError("mode")));
        form.insert("mode".into(), "payment".into());
        form.remove("client_reference_id");
        assert_eq!(CheckoutCreate::from_form(&form), Err(FormError("client_reference_id")));
    }

    #[test]
    fn session_reads_only_known_fields() {
        let session: CheckoutSession = serde_json::from_value(json!({
            "id": "cs_test_1", "object": "checkout.session", "status": "complete",
            "payment_status": "paid", "amount_total": 500, "currency": "usd",
            "client_reference_id": "ab", "metadata": {"product": "usd-5", "other": "x"},
            "customer_details": {"email": "someone@example.com"}, "url": null
        }))
        .unwrap();
        assert!(session.is_paid());
        assert_eq!(session.metadata.product.as_deref(), Some("usd-5"));
        let echoed = serde_json::to_string(&session).unwrap();
        assert!(!echoed.contains("example.com"));
    }

    #[test]
    fn error_body_shape() {
        let body: StripeErrorBody = serde_json::from_value(json!({
            "error": {"type": "invalid_request_error", "code": "resource_missing", "message": "No such"}
        }))
        .unwrap();
        assert!(body.is_missing());
    }
}
