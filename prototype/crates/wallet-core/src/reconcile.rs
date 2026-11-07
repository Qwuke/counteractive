//! The balance view (docs 04 section 7) and the settled-session report
//! (docs 04 section 10). The credential is authoritative: balances come
//! only from `finalize_issue` and `finalize_refund` results.

use std::collections::BTreeMap;

use credit::Holder;
use manifest::{Manifest, Platform};
use metering::wallet::{format_usd, in_session_value, output_tokens_at_most};
use proto::walletd::{
    BalanceView, ChainState, ChainView, PendingOrderView, PurchaseState, SettledSessionView, TokenEstimate,
};
use secrecy::ExposeSecret as _;

use crate::journal::{ChainRow, Journal};
use crate::manifest_store::usable_configs;
use crate::{Pins, WalletError};

/// The balance view, following the contribution table of docs 04 section 7.
pub fn balance_view(
    journal: &Journal,
    holder: &Holder,
    pins: &Pins,
    manifest: &Manifest,
) -> Result<BalanceView, WalletError> {
    let mut view = BalanceView {
        total_credits: 0,
        total_usd: String::new(),
        estimated_tokens_remaining: BTreeMap::new(),
        chains: Vec::new(),
        orders_awaiting_payment: Vec::new(),
        frozen_orders: 0,
        frozen_chains: 0,
        dust_credits: 0,
        experimental_platform: false,
        manifest_version: Some(manifest.version),
    };

    for row in journal.chains()? {
        let credits = contribution(journal, holder, &row)?;
        match row.state {
            ChainState::Dust => view.dust_credits += credits,
            ChainState::Frozen => view.frozen_chains += 1,
            ChainState::Ready
            | ChainState::Spending
            | ChainState::Open
            | ChainState::Closing
            | ChainState::Recovering => view.total_credits += credits,
        }
        view.chains.push(ChainView { id: row.id.0, state: row.state, credits, resolved: row.state.is_resolved() });
    }

    for purchase in journal.purchases_in(&[PurchaseState::Ordered, PurchaseState::AwaitingPayment])? {
        view.orders_awaiting_payment.push(PendingOrderView { product: purchase.product, credits: purchase.credits });
    }
    view.frozen_orders = journal.purchases_in(&[PurchaseState::Frozen])?.len() as u64;

    let usable = usable_configs(manifest, pins.allow_dev_attestation);
    view.experimental_platform = usable.iter().any(|(platform, _)| is_experimental(*platform));
    // The cheapest output rate any usable release offers gives the ceiling.
    let mut lowest_rates: BTreeMap<String, u64> = BTreeMap::new();
    for entry in usable.iter().flat_map(|(_, config)| &config.catalogue) {
        let rate = lowest_rates.entry(entry.id.clone()).or_insert(entry.output_rate);
        *rate = (*rate).min(entry.output_rate);
    }
    view.estimated_tokens_remaining = lowest_rates
        .into_iter()
        .map(|(model, rate)| {
            (model, TokenEstimate { output_tokens_at_most: output_tokens_at_most(view.total_credits, rate) })
        })
        .collect();
    view.total_usd = format_usd(view.total_credits);
    Ok(view)
}

/// What a chain holds: the credential's balance, or in a session what the
/// credential kept plus what the session has left by the last receipt.
/// A frozen chain shows nothing, since its state is under investigation.
fn contribution(journal: &Journal, holder: &Holder, row: &ChainRow) -> Result<u64, WalletError> {
    match row.state {
        ChainState::Ready | ChainState::Dust => {
            let stored = row.credential.as_ref().ok_or(WalletError::Corrupt("chain without a credential"))?;
            Ok(holder.credential_from_bytes(stored.expose_secret())?.balance())
        }
        ChainState::Spending | ChainState::Open | ChainState::Closing | ChainState::Recovering => {
            let stored = row.spend_state.as_ref().ok_or(WalletError::Corrupt("chain without a spend state"))?;
            let remainder = holder.spend_state_from_bytes(stored.expose_secret())?.remainder();
            let budget = row.budget.ok_or(WalletError::Corrupt("chain without a budget"))?;
            let last = journal.receipts(row.id, row.spend_seq)?.last().map(|receipt| receipt.remaining);
            Ok(in_session_value(remainder, budget, last))
        }
        ChainState::Frozen => Ok(0),
    }
}

/// Whether the wallet labels a platform experimental. Both are for now:
/// `dev-insecure` has no hardware root at all, and SNP has no supported
/// TCB floor since AMD withdrew its published values (docs 04 section 6).
fn is_experimental(platform: Platform) -> bool {
    match platform {
        Platform::DevInsecure | Platform::SnpVerda => true,
    }
}

/// Every settled session with its classification, and how many chains are frozen.
pub fn settled_report(journal: &Journal) -> Result<(Vec<SettledSessionView>, u64), WalletError> {
    let frozen = journal.chains()?.iter().filter(|row| row.state == ChainState::Frozen).count() as u64;
    Ok((journal.settled_sessions()?, frozen))
}
