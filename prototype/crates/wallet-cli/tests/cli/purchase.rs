//! The purchase plane: `buy` journals before it sends anything, and
//! `collect`'s exit status tells a script what is left (0 collected, 2
//! still unpaid, 1 an error such as a frozen purchase).

use proto::walletd::PurchaseState;
use wallet_core::Journal;

use super::harness::{Output, Shop, USD_5, closed_port, wallet_cli, write_pins};

/// The rest of the first line of `text` starting with `prefix`.
fn line_value(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| line.strip_prefix(prefix)).map(str::to_owned)
}

fn assert_exit(output: &Output, code: i32, what: &str) {
    assert_eq!(output.code, Some(code), "{what}: stdout {:?}, stderr {:?}", output.stdout, output.stderr);
}

/// `init` and `manifest fetch --approve`, which every purchase needs.
async fn accepted(shop: &Shop) {
    assert_exit(&wallet_cli(shop.dir(), &["init"], None).await, 0, "init");
    let fetched = wallet_cli(shop.dir(), &["manifest", "fetch", "--approve"], None).await;
    assert_exit(&fetched, 0, "manifest fetch --approve");
    assert!(fetched.stdout.contains("accepted manifest 1"), "{}", fetched.stdout);
}

/// The purchase is journalled, secret and issue request included, before
/// anything goes over the network (ARCHITECTURE section 6.2): with the
/// storefront unreachable, `buy` fails but leaves the purchase `Ordered`,
/// and `collect` later places that very order, which is paid and
/// collected into a chain. A product the accepted manifest doesn't sell is
/// refused before anything is journalled.
#[tokio::test]
async fn buy_journals_the_purchase_before_any_network_call() {
    let shop = Shop::start().await;
    accepted(&shop).await;
    let mut unreachable = shop.pins.clone();
    unreachable.storefront_addr = closed_port().await.to_string();
    write_pins(shop.dir(), &unreachable);

    let unknown = wallet_cli(shop.dir(), &["buy", "--product", "usd-500", "--no-browser"], None).await;
    assert_exit(&unknown, 1, "buying an unknown product");
    assert!(unknown.stderr.contains("no product `usd-500`; the accepted manifest sells usd-5"), "{}", unknown.stderr);
    let all = [PurchaseState::Ordered, PurchaseState::AwaitingPayment];
    assert!(Journal::open(shop.dir()).unwrap().purchases_in(&all).unwrap().is_empty());

    let bought = wallet_cli(shop.dir(), &["buy", "--product", "usd-5", "--no-browser"], None).await;
    assert_exit(&bought, 1, "buying with the storefront down");
    assert!(bought.stderr.contains("the purchase is journalled"), "{}", bought.stderr);
    assert!(!bought.stdout.contains("order_id"), "no order can exist: {}", bought.stdout);
    let journal = Journal::open(shop.dir()).unwrap();
    let [row] = &journal.purchases_in(&all).unwrap()[..] else { panic!("one purchase journalled") };
    assert_eq!((row.state, row.product.as_str(), row.credits), (PurchaseState::Ordered, "usd-5", USD_5));
    assert!(row.order_id.is_none() && row.issue_request.is_some() && row.issue_state.is_some());
    let (purchase, issue_request) = (row.id, row.issue_request.clone());
    drop(journal);

    // The storefront is back: collect sends the journalled order.
    write_pins(shop.dir(), &shop.pins);
    let placed = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&placed, 2, "collect placing the order");
    let order_id = line_value(&placed.stdout, "order_id: ").expect("collect printed the order");
    assert!(line_value(&placed.stdout, "checkout_url: ").is_some(), "{}", placed.stdout);
    let journal = Journal::open(shop.dir()).unwrap();
    let [row] = &journal.purchases_in(&all).unwrap()[..] else { panic!("still one purchase") };
    assert_eq!(
        (row.id, row.state, row.order_id.as_deref()),
        (purchase, PurchaseState::AwaitingPayment, Some(&*order_id))
    );
    assert_eq!(row.issue_request, issue_request, "the journalled request, never a new one");
    drop(journal);

    shop.pay(&order_id).await;
    let collected = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&collected, 0, "collect after paying");
    assert!(
        collected.stdout.contains("collected usd-5: chain 1 holds 5000000 credits (Ready)"),
        "{}",
        collected.stdout
    );
    shop.stop().await;
}

/// `collect` exits 2 while an order is unpaid, saying `402 unpaid`, and 0
/// once it has collected it, as xtask's step 3 expects; with nothing left
/// to collect it says so and exits 0.
#[tokio::test]
async fn collect_exits_2_while_unpaid_and_0_once_collected() {
    let shop = Shop::start().await;
    accepted(&shop).await;
    let bought = wallet_cli(shop.dir(), &["buy", "--product", "usd-5", "--no-browser"], None).await;
    assert_exit(&bought, 0, "buy");
    let order_id = line_value(&bought.stdout, "order_id: ").expect("buy printed the order id");
    assert!(line_value(&bought.stdout, "checkout_url: ").is_some_and(|url| url.contains(&order_id)));

    let early = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&early, 2, "collect before paying");
    assert!(early.stdout.contains(&format!("order {order_id}: 402 unpaid")), "{}", early.stdout);

    shop.pay(&order_id).await;
    let collected = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&collected, 0, "collect after paying");
    assert!(
        collected.stdout.contains("collected usd-5: chain 1 holds 5000000 credits (Ready)"),
        "{}",
        collected.stdout
    );
    let nothing = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&nothing, 0, "collect with nothing waiting");
    assert!(nothing.stdout.contains("no orders waiting"), "{}", nothing.stdout);
    shop.stop().await;
}

/// An issue response that fails to finalise freezes the purchase, and
/// `collect` exits 1, not 0 or 2: the order is paid, so it isn't waiting,
/// and nothing was collected.
#[tokio::test]
async fn collect_exits_1_when_it_freezes_a_purchase() {
    let shop = Shop::start().await;
    accepted(&shop).await;
    let bought = wallet_cli(shop.dir(), &["buy", "--product", "usd-5", "--no-browser"], None).await;
    let order_id = line_value(&bought.stdout, "order_id: ").expect("buy printed the order id");
    shop.pay(&order_id).await;
    shop.corrupt_issuance();

    let frozen = wallet_cli(shop.dir(), &["collect"], None).await;
    assert_exit(&frozen, 1, "collect of a bad issue response");
    assert!(frozen.stderr.contains("the issue response failed to verify"), "{}", frozen.stderr);
    let journal = Journal::open(shop.dir()).unwrap();
    assert_eq!(journal.purchases_in(&[PurchaseState::Frozen]).unwrap().len(), 1);
    assert!(journal.chains().unwrap().is_empty(), "no chain from a response that didn't verify");
    shop.stop().await;
}
