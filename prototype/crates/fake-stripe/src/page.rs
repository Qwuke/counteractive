//! The pay page a buyer's browser opens in place of Stripe Checkout.

use crate::session::Session;

/// A page naming the product and price, with a Pay button that POSTs back.
/// Once paid, it just says so.
pub(crate) fn pay_page(session: &Session) -> String {
    let order = &session.order;
    let body = if session.paid {
        "<p>Paid. You can return to your wallet.</p>".to_owned()
    } else {
        format!(
            "<form method=\"post\" action=\"/pay/{id}\"><button type=\"submit\">Pay</button></form>",
            id = escape(&session.id)
        )
    };
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>fake-stripe checkout</title></head>\n\
         <body><h1>Test checkout</h1>\n<p>{product}: {price}</p>\n{body}\n\
         <p>This is fake-stripe. No card is charged.</p></body></html>\n",
        product = escape(&order.product_id),
        price = price(order.unit_amount, &order.currency),
    )
}

/// `500, "usd"` as `5.00 USD`.
fn price(cents: u64, currency: &str) -> String {
    format!("{}.{:02} {}", cents / 100, cents % 100, escape(&currency.to_ascii_uppercase()))
}

/// Escapes text for HTML element content and quoted attributes.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::stripe::CheckoutCreate;

    fn session(product_id: &str, paid: bool) -> Session {
        Session {
            id: "cs_test_1".into(),
            order: CheckoutCreate {
                unit_amount: 105,
                currency: "usd".into(),
                product_id: product_id.into(),
                client_reference_id: "ab".into(),
                success_url: "http://127.0.0.1:7777/paid".into(),
            },
            paid,
        }
    }

    #[test]
    fn unpaid_page_posts_back_to_itself() {
        let page = pay_page(&session("usd-1", false));
        assert!(page.contains("usd-1: 1.05 USD"));
        assert!(page.contains("<form method=\"post\" action=\"/pay/cs_test_1\">"));
        assert!(page.contains(">Pay</button>"));
    }

    #[test]
    fn paid_page_has_no_button() {
        let page = pay_page(&session("usd-1", true));
        assert!(page.contains("Paid."));
        assert!(!page.contains("<form"));
    }

    #[test]
    fn product_names_are_escaped() {
        let page = pay_page(&session("<script>\"&'", false));
        assert!(page.contains("&lt;script&gt;&quot;&amp;&#39;"));
        assert!(!page.contains("<script>"));
    }
}
