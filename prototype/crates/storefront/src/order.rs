//! The rules an order must pass, with no IO: which order ids are
//! acceptable, what a Checkout Session must say before the gate is asked to
//! issue, and the key the gate makes issuance idempotent by.

use manifest::{Product, ReleaseConfig};
use proto::ErrorCode;
use proto::SecretHex32;
use proto::encoding::hex_decode_array;
use proto::stripe::{CURRENCY_USD, CheckoutSession};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

/// A Checkout Session id, which is the order id.
///
/// Stripe's ids are ASCII letters, digits and underscores. Anything else is
/// refused before it reaches a Stripe URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderId(String);

impl OrderId {
    /// The longest id accepted. Stripe's are well under this.
    pub const MAX_LEN: usize = 255;

    /// The id, if it has Stripe's shape.
    pub fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text.len() <= Self::MAX_LEN
            && text.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        valid.then(|| Self(text.to_owned()))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `SHA-256(order_id)`: the gate keys issuance by this, so each paid
    /// order is credited once however often collect runs.
    pub fn order_ref(&self) -> [u8; 32] {
        Sha256::digest(self.0.as_bytes()).into()
    }
}

/// Decides whether a session lets the holder of `order_secret` collect, and
/// for which product. The checks run in SPEC section 6's order:
///
/// 1. `SHA-256(order_secret)` equals `client_reference_id`, in constant time: else `401`.
/// 2. `payment_status` is `paid`: else `402 unpaid`.
/// 3. `metadata.product` is a product of this release, and `amount_total`
///    and `currency` are its price in USD: else `409 amount_mismatch`.
///
/// Ownership comes first, so a stranger learns nothing about an order's
/// payment state.
pub fn authorise<'a>(
    release: &'a ReleaseConfig,
    session: &CheckoutSession,
    order_secret: &SecretHex32,
) -> Result<&'a Product, ErrorCode> {
    let secret_hash: [u8; 32] = Sha256::digest(order_secret.expose()).into();
    let reference = session.client_reference_id.as_deref().and_then(|text| hex_decode_array::<32>(text).ok());
    let owned = reference.is_some_and(|reference| bool::from(reference.ct_eq(&secret_hash)));
    if !owned {
        return Err(ErrorCode::Unauthorized);
    }
    if !session.is_paid() {
        return Err(ErrorCode::Unpaid);
    }
    let product = session.metadata.product.as_deref().and_then(|id| release.product(id));
    match product {
        Some(product)
            if session.amount_total == Some(product.usd_cents) && session.currency.as_deref() == Some(CURRENCY_USD) =>
        {
            Ok(product)
        }
        _ => Err(ErrorCode::AmountMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::stripe::CheckoutMetadata;
    use serde_json::json;

    const SECRET: [u8; 32] = [7; 32];

    fn release() -> ReleaseConfig {
        ReleaseConfig::from_value(&json!({
            "schema": 1, "image_version": "0.1.0", "service_id": "counteractive-poc", "key_epoch": 1,
            "balance_width": 24, "operator_public_key": "RWQ", "k_sha256": "00".repeat(32),
            "products": [{"id": "usd-5", "usd_cents": 500, "credits": 5_000_000},
                         {"id": "usd-1", "usd_cents": 100, "credits": 1_000_000}],
            "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
            "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64,
                               "max_age_seconds": 3600},
            "catalogue": [{"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                           "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096}]
        }))
        .unwrap()
    }

    /// A paid usd-5 session owned by [`SECRET`].
    fn paid_session() -> CheckoutSession {
        CheckoutSession {
            id: "cs_test_1".into(),
            status: Some("complete".into()),
            payment_status: Some("paid".into()),
            amount_total: Some(500),
            currency: Some("usd".into()),
            client_reference_id: Some(hex::encode(Sha256::digest(SECRET))),
            metadata: CheckoutMetadata { product: Some("usd-5".into()) },
            url: None,
        }
    }

    fn check(session: &CheckoutSession, secret: [u8; 32]) -> Result<u64, ErrorCode> {
        let release = release();
        authorise(&release, session, &SecretHex32::new(secret)).map(|product| product.credits)
    }

    #[test]
    fn a_paid_owned_session_names_its_product() {
        assert_eq!(check(&paid_session(), SECRET), Ok(5_000_000));
    }

    #[test]
    fn ownership_is_checked_first() {
        let unpaid_mismatched =
            CheckoutSession { payment_status: Some("unpaid".into()), amount_total: Some(1), ..paid_session() };
        assert_eq!(check(&unpaid_mismatched, [8; 32]), Err(ErrorCode::Unauthorized));
        let no_reference = CheckoutSession { client_reference_id: None, ..paid_session() };
        assert_eq!(check(&no_reference, SECRET), Err(ErrorCode::Unauthorized));
        let garbled = CheckoutSession { client_reference_id: Some("zz".into()), ..paid_session() };
        assert_eq!(check(&garbled, SECRET), Err(ErrorCode::Unauthorized));
    }

    #[test]
    fn payment_is_checked_before_the_amount() {
        let unpaid_mismatched =
            CheckoutSession { payment_status: Some("unpaid".into()), amount_total: Some(1), ..paid_session() };
        assert_eq!(check(&unpaid_mismatched, SECRET), Err(ErrorCode::Unpaid));
        let no_status = CheckoutSession { payment_status: None, ..paid_session() };
        assert_eq!(check(&no_status, SECRET), Err(ErrorCode::Unpaid));
    }

    #[test]
    fn product_amount_and_currency_must_match_the_release() {
        let cases = [
            CheckoutSession { amount_total: Some(499), ..paid_session() },
            CheckoutSession { amount_total: None, ..paid_session() },
            CheckoutSession { currency: Some("eur".into()), ..paid_session() },
            CheckoutSession { metadata: CheckoutMetadata { product: Some("usd-9".into()) }, ..paid_session() },
            CheckoutSession { metadata: CheckoutMetadata::default(), ..paid_session() },
            // usd-1's credits for usd-5's price.
            CheckoutSession { metadata: CheckoutMetadata { product: Some("usd-1".into()) }, ..paid_session() },
        ];
        for session in cases {
            assert_eq!(check(&session, SECRET), Err(ErrorCode::AmountMismatch), "{session:?}");
        }
    }

    #[test]
    fn order_ids_have_stripes_shape() {
        assert!(OrderId::parse("cs_test_a1B2").is_some());
        for bad in ["", "cs/../x", "cs?x=1", "cs%2Fx", "cs x", "cs-x", &"a".repeat(256)] {
            assert!(OrderId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn order_ref_is_the_hash_of_the_id() {
        let id = OrderId::parse("cs_test_1").unwrap();
        assert_eq!(id.order_ref(), <[u8; 32]>::from(Sha256::digest(b"cs_test_1")));
    }
}
