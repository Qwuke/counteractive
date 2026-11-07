//! Spending a chain and settling it (docs 04 sections 5 and 7).
//!
//! `prove_spend` consumes the credential, so the spend state and message
//! are journalled before the message is sent, and every retry resends the
//! stored bytes: a second proof from the same credential would reveal the
//! same nullifier. `finalize_refund` consumes the spend state, so its
//! result and the classified session commit in one transaction.

use crash_point::crash_point;
use credit::Holder;
use metering::wallet::{classify_settlement, is_dust, session_budget};
use proto::SecretBlob;
use proto::walletd::{ChainState, SettledSessionView, SettlementOutcome};
use secrecy::ExposeSecret as _;

use crate::WalletError;
use crate::journal::{ChainId, ChainRow, Journal};

/// A spend that is journalled and may now be sent.
#[derive(Debug, Clone)]
pub struct PreparedSpend {
    /// The chain it spends from.
    pub chain: ChainId,
    /// Which spend of that chain.
    pub spend_seq: u64,
    /// `s`, the session's budget.
    pub budget: u64,
    /// The spend bytes for `POST /v1/sessions`.
    pub message: SecretBlob,
}

/// Picks the first `Ready` chain whose session size is at least `at_least`,
/// proves a spend of that size and journals it as `Spending`.
///
/// The size is the largest tier not above `min(balance, maximum_spend)`.
/// `None` means no chain can open such a session.
pub fn prepare_spend(
    journal: &mut Journal,
    holder: &Holder,
    tiers: &[u64],
    maximum_spend: u64,
    at_least: u64,
) -> Result<Option<PreparedSpend>, WalletError> {
    for row in journal.chains()? {
        if row.state != ChainState::Ready {
            continue;
        }
        let stored = row.credential.as_ref().ok_or(WalletError::Corrupt("Ready chain without a credential"))?;
        let credential = holder.credential_from_bytes(stored.expose_secret())?;
        let Some(budget) = session_budget(tiers, credential.balance(), maximum_spend) else { continue };
        if budget < at_least {
            continue;
        }
        let (spend_state, message) = holder.prove_spend(credential, budget)?;
        let spend_seq = journal.begin_spend(row.id, &spend_state.to_bytes(), &message, budget)?;
        // Journalled, not yet sent: a retry must send these bytes, never a new proof.
        crash_point!("after_journal_spend");
        return Ok(Some(PreparedSpend { chain: row.id, spend_seq, budget, message: SecretBlob::new(message) }));
    }
    Ok(None)
}

/// The journalled spend of an unsettled chain, to resend unchanged.
pub fn stored_spend(row: &ChainRow) -> Result<PreparedSpend, WalletError> {
    let message = row.spend_message.as_ref().ok_or(WalletError::Corrupt("unsettled chain without a spend"))?;
    Ok(PreparedSpend {
        chain: row.id,
        spend_seq: row.spend_seq,
        budget: row.budget.ok_or(WalletError::Corrupt("unsettled chain without a budget"))?,
        message: SecretBlob::new(message.expose_secret().to_vec()),
    })
}

/// Finalises a refund for spend `spend_seq` of the chain and commits the
/// new credential with the classified session (docs 04 section 10).
///
/// A chain that has moved past that spend is refused untouched: the
/// refund belongs to an earlier spend, and finalising it against the
/// current one would fail and freeze a healthy chain. A refund for this
/// spend that fails to verify freezes the chain, keeping its state: it is
/// reported, never discarded.
pub fn settle(
    journal: &mut Journal,
    holder: &Holder,
    tiers: &[u64],
    chain: ChainId,
    spend_seq: u64,
    refund: &[u8],
) -> Result<SettledSessionView, WalletError> {
    let row = journal.chain(chain)?;
    if row.spend_seq != spend_seq || !row.is_unsettled() {
        return Err(WalletError::WrongState { id: chain.0, found: "not in this session".into() });
    }
    let stored = row.spend_state.as_ref().ok_or(WalletError::Corrupt("unsettled chain without a spend state"))?;
    let s = row.budget.ok_or(WalletError::Corrupt("unsettled chain without a budget"))?;
    // The refund is in memory only: recover must return it again.
    crash_point!("before_finalize_refund");
    let finalized = holder.refund_amount(refund).and_then(|t| {
        let state = holder.spend_state_from_bytes(stored.expose_secret())?;
        Ok((t, holder.finalize_refund(state, refund)?))
    });
    let (t, credential) = match finalized {
        Ok(finalized) => finalized,
        Err(error) => {
            freeze(journal, chain, spend_seq, "the refund did not verify")?;
            return Err(error.into());
        }
    };

    let receipts_sum = journal.receipts(chain, row.spend_seq)?.iter().map(|receipt| receipt.charge).sum();
    let outcome = classify_settlement(s, t, receipts_sum, row.unknown_turns);
    if outcome == SettlementOutcome::Mismatch {
        tracing::warn!(chain = chain.0, s, t, receipts_sum, "receipts do not match the refund");
    }
    let settled = SettledSessionView { chain_id: chain.0, spend_seq: row.spend_seq, s, t, receipts_sum, outcome };
    let state = if is_dust(tiers, credential.balance()) { ChainState::Dust } else { ChainState::Ready };
    journal.settle_chain(chain, &credential.to_bytes(), state, &settled)?;
    Ok(settled)
}

/// Freezes a chain in spend `spend_seq` for investigation by hand, saying
/// why in the log.
pub fn freeze(journal: &mut Journal, chain: ChainId, spend_seq: u64, reason: &str) -> Result<(), WalletError> {
    journal.freeze_chain(chain, spend_seq)?;
    tracing::error!(chain = chain.0, reason, "chain frozen; its state is kept for investigation");
    Ok(())
}
