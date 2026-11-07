//! The purchase plane (docs 04 section 5): `init`, `manifest`, `buy` and
//! `collect`. Everything here runs over the TCP dialer to the storefront;
//! nothing reaches the gate.
//!
//! Every step is journalled before the bytes it produced leave the
//! process, so a crash at any point is repaired by running `collect`.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow, bail};
use crash_point::crash_point;
use credit::Holder;
use gate_conn::{ConnError, Target};
use manifest::{AcceptPins, Manifest, Product};
use proto::ErrorCode;
use proto::walletd::{ManifestSummary, PurchaseState};
use secrecy::ExposeSecret as _;
use wallet_core::manifest_store::{self, Offered};
use wallet_core::purchase::{self, Collected};
use wallet_core::{Journal, Pins, PurchaseRow};

use crate::storefront::{Storefront, is_order_id};

/// How often `collect --wait` asks again.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// An opened wallet directory.
pub struct Wallet {
    pins: Pins,
    accept_pins: AcceptPins,
    journal: Journal,
}

impl Wallet {
    /// Opens the pins and journal in `dir`.
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        let pins = Pins::read(dir)?;
        let accept_pins = pins.accept_pins()?;
        let journal = Journal::open(dir).context("opening the journal: run `wallet-cli init` first")?;
        Ok(Self { pins, accept_pins, journal })
    }

    fn storefront_target(&self) -> anyhow::Result<Target> {
        Ok(format!("tcp:{}", self.pins.storefront_addr).parse()?)
    }

    /// Attests the storefront against the accepted manifest.
    async fn storefront(&self) -> anyhow::Result<(Manifest, Storefront)> {
        let manifest = manifest_store::require_accepted(&self.journal, &self.accept_pins)?;
        let storefront = Storefront::connect(&self.storefront_target()?, &manifest, self.pins.allow_dev_attestation)
            .await
            .context("attesting the storefront")?;
        Ok((manifest, storefront))
    }
}

/// `init`: creates the journal and its key next to an existing `wallet.toml`.
pub fn init(dir: &Path) -> anyhow::Result<()> {
    let pins = Pins::read(dir).context("init needs wallet.toml, the pins a wallet release compiles in")?;
    Journal::create(dir)?;
    println!("initialised wallet in {} for service {} epoch {}", dir.display(), pins.service_id, pins.key_epoch);
    Ok(())
}

/// `manifest fetch [--approve]`: fetches the storefront's manifest and runs
/// the accept rule. A manifest that passes waits for approval.
pub async fn fetch_manifest(wallet: &mut Wallet, approve: bool) -> anyhow::Result<()> {
    let signed = Storefront::fetch_manifest(&wallet.storefront_target()?).await.context("fetching the manifest")?;
    match manifest_store::offer(&mut wallet.journal, &wallet.accept_pins, &signed)? {
        Offered::AlreadyAccepted(manifest) => {
            println!("manifest {} is already accepted", manifest.version);
            Ok(())
        }
        Offered::Pending(manifest) => {
            print_summary("fetched", &manifest.summary());
            if approve {
                approve_manifest(wallet)
            } else {
                println!("not accepted yet: review it, then run `wallet-cli manifest approve`");
                Ok(())
            }
        }
    }
}

/// `manifest approve`: accepts the fetched manifest waiting for approval.
pub fn approve_manifest(wallet: &mut Wallet) -> anyhow::Result<()> {
    let manifest = manifest_store::approve(&mut wallet.journal, &wallet.accept_pins)?;
    println!("accepted manifest {}", manifest.version);
    Ok(())
}

fn print_summary(what: &str, summary: &ManifestSummary) {
    println!(
        "{what} manifest {}: images {}; platforms {}",
        summary.version,
        summary.image_versions.join(", "),
        summary.platforms.join(", ")
    );
}

/// `buy --product <id>`: journals the purchase, then creates the order and
/// shows where to pay. The product comes from the accepted manifest, so
/// the purchase is journalled before anything goes over the network
/// (ARCHITECTURE section 6.2). If the storefront can't be reached, the
/// purchase waits as `Ordered`, and `collect` sends its order.
pub async fn buy(wallet: &mut Wallet, product_id: &str, open_browser: bool) -> anyhow::Result<()> {
    let manifest = manifest_store::require_accepted(&wallet.journal, &wallet.accept_pins)?;
    let mut products: Vec<Product> = manifest_store::usable_configs(&manifest, wallet.pins.allow_dev_attestation)
        .into_iter()
        .flat_map(|(_, config)| config.products)
        .collect();
    let Some(product) = products.iter().find(|product| product.id == product_id).cloned() else {
        products.sort_by(|a, b| a.id.cmp(&b.id));
        products.dedup_by(|a, b| a.id == b.id);
        let ids: Vec<_> = products.iter().map(|product| product.id.as_str()).collect();
        bail!("no product `{product_id}`; the accepted manifest sells {}", ids.join(", "));
    };
    let holder = manifest_store::holder(&wallet.pins, &manifest)?;
    let id = purchase::begin(&mut wallet.journal, &holder, &product)?;
    // Journalled as Ordered, nothing sent: `collect` sends this order.
    crash_point!("after_journal_purchase");
    let row = wallet
        .journal
        .purchases_in(&[PurchaseState::Ordered])?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| anyhow!("the purchase just journalled is missing"))?;
    let (_, mut storefront) = wallet
        .storefront()
        .await
        .context("the purchase is journalled; `wallet-cli collect` places its order once the storefront answers")?;
    let checkout_url = place_order(wallet, &mut storefront, &row).await?;
    if open_browser {
        open_in_browser(&checkout_url);
    }
    println!("pay in the browser, then run `wallet-cli collect`");
    Ok(())
}

/// Sends an `Ordered` purchase's order and journals its id, then prints
/// the order and where to pay. Only an order id that is safe in a path
/// is accepted; anything else freezes the purchase as a storefront fault.
async fn place_order(wallet: &mut Wallet, storefront: &mut Storefront, row: &PurchaseRow) -> anyhow::Result<String> {
    let order =
        storefront.create_order(&row.product, purchase::secret_hash(row)).await.context("creating the order")?;
    // The order exists but its id isn't journalled, as if the response were
    // lost: `collect` sends the same order again.
    crash_point!("after_create_order");
    if !is_order_id(&order.order_id) {
        wallet.journal.freeze_purchase(row.id)?;
        bail!("the storefront sent a malformed order id; purchase {} is frozen", row.id.0);
    }
    wallet.journal.set_order_id(row.id, &order.order_id)?;
    println!("order_id: {}", order.order_id);
    println!("checkout_url: {}", order.checkout_url);
    Ok(order.checkout_url)
}

/// Opens an `http` or `https` URL in the default browser. Failing to open
/// one is not an error: the URL is already printed.
fn open_in_browser(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        eprintln!("not opening a non-web checkout URL");
        return;
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    if let Err(error) = std::process::Command::new(opener).arg(url).status() {
        eprintln!("could not open the browser ({error}); open the checkout URL yourself");
    }
}

/// What a `collect` run left.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CollectOutcome {
    /// Purchases still waiting for payment or for a retry.
    pub waiting: usize,
    /// Purchases frozen by this run.
    pub frozen: usize,
}

/// `collect [--wait <secs>]`: collects every paid order, asking again
/// until nothing waits or the wait is over.
pub async fn collect(wallet: &mut Wallet, wait: Duration) -> anyhow::Result<CollectOutcome> {
    let deadline = Instant::now() + wait;
    let mut total = CollectOutcome::default();
    loop {
        let round = collect_round(wallet).await?;
        total.frozen += round.frozen;
        total.waiting = round.waiting;
        if round.waiting == 0 || Instant::now() + POLL_INTERVAL > deadline {
            return Ok(total);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn collect_round(wallet: &mut Wallet) -> anyhow::Result<CollectOutcome> {
    let rows = wallet.journal.purchases_in(&[PurchaseState::Ordered, PurchaseState::AwaitingPayment])?;
    let mut outcome = CollectOutcome::default();
    if rows.is_empty() {
        println!("no orders waiting");
        return Ok(outcome);
    }
    let (manifest, mut storefront) = wallet.storefront().await?;
    let holder = manifest_store::holder(&wallet.pins, &manifest)?;
    for row in rows {
        if row.state == PurchaseState::Ordered {
            // A crash before the order's id was journalled: the URL never
            // reached the user, so send the same order again.
            place_order(wallet, &mut storefront, &row).await?;
            outcome.waiting += 1;
            continue;
        }
        match collect_one(wallet, &mut storefront, &holder, &row).await? {
            Step::Collected(collected) => println!(
                "collected {}: chain {} holds {} credits ({:?})",
                row.product, collected.chain, collected.credits, collected.state
            ),
            Step::Waiting(why) => {
                println!("order {}: {why}", order_label(&row));
                outcome.waiting += 1;
            }
            Step::Frozen(why) => {
                eprintln!("order {}: {why}; purchase {} is frozen for investigation", order_label(&row), row.id.0);
                outcome.frozen += 1;
            }
        }
    }
    Ok(outcome)
}

enum Step {
    Collected(Collected),
    Waiting(String),
    Frozen(String),
}

/// One collect, with the reaction table of docs 04 section 5: `402` and
/// transient failures wait; `400`, `401`, `404` and `409` freeze, as does
/// an issue response that fails to finalise.
async fn collect_one(
    wallet: &mut Wallet,
    storefront: &mut Storefront,
    holder: &Holder,
    row: &PurchaseRow,
) -> anyhow::Result<Step> {
    let order_id = row.order_id.as_deref().ok_or_else(|| anyhow!("an awaiting purchase has no order id"))?;
    let issue_request = row.issue_request.as_deref().ok_or_else(|| anyhow!("an awaiting purchase has no request"))?;
    let answer = storefront.collect(order_id, *row.order_secret.expose_secret(), issue_request).await;
    let response = match answer {
        Ok(response) => response,
        Err(ConnError::Api { status: 402, .. }) => return Ok(Step::Waiting("402 unpaid".into())),
        Err(ConnError::Api { status: status @ (400 | 401 | 404 | 409), error }) => {
            wallet.journal.freeze_purchase(row.id)?;
            let code = error.map_or(ErrorCode::Unknown, |body| body.error);
            return Ok(Step::Frozen(format!("{status} {code}")));
        }
        Err(error) => return Ok(Step::Waiting(format!("{error}; will retry with the same request"))),
    };
    let tiers = &wallet.pins.tier_ladder;
    match purchase::complete(&mut wallet.journal, holder, tiers, row, response.issue_response.as_bytes()) {
        Ok(collected) => {
            if collected.credits != row.credits {
                eprintln!("warning: ordered {} credits but the credential holds {}", row.credits, collected.credits);
            }
            Ok(Step::Collected(collected))
        }
        Err(wallet_core::WalletError::Credit(_)) => Ok(Step::Frozen("the issue response failed to verify".into())),
        Err(error) => Err(error.into()),
    }
}

fn order_label(row: &PurchaseRow) -> &str {
    row.order_id.as_deref().unwrap_or("(not sent)")
}
