//! Buying and collecting, up to the first `Ready` (docs 04 section 5).
//!
//! The order secret, issue state and issue request are generated once and
//! journalled before the order exists, so every retry sends identical
//! bytes. A paid order becomes credit only in the transaction that stores
//! `finalize_issue`'s credential.

use crash_point::crash_point;
use credit::Holder;
use manifest::Product;
use proto::walletd::ChainState;
use rand::TryRng as _;
use secrecy::{ExposeSecret as _, ExposeSecretMut as _, SecretBox};
use sha2::{Digest as _, Sha256};

use crate::WalletError;
use crate::journal::{ChainId, Journal, NewPurchase, PurchaseId, PurchaseRow};

/// Generates the order secret and issue request for `product` and journals
/// them as `Ordered`. Nothing has been sent yet.
pub fn begin(journal: &mut Journal, holder: &Holder, product: &Product) -> Result<PurchaseId, WalletError> {
    let mut order_secret = SecretBox::new(Box::new([0u8; 32]));
    rand::rngs::SysRng.try_fill_bytes(order_secret.expose_secret_mut()).map_err(|_| WalletError::Entropy)?;
    let (issue_state, issue_request) = holder.issue_request()?;
    journal.insert_purchase(&NewPurchase {
        order_secret,
        product: product.id.clone(),
        credits: product.credits,
        issue_state: issue_state.to_bytes(),
        issue_request,
    })
}

/// `SHA-256(order_secret)`: what the storefront names the order by.
pub fn secret_hash(purchase: &PurchaseRow) -> [u8; 32] {
    Sha256::digest(purchase.order_secret.expose_secret()).into()
}

/// A collected purchase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collected {
    /// The new chain.
    pub chain: ChainId,
    /// Its balance, from the credential itself.
    pub credits: u64,
    /// `Ready`, or `Dust` if the credential is below every tier.
    pub state: ChainState,
}

/// Finalises issuance with the storefront's response and commits the new
/// chain. A response that fails `finalize_issue` freezes the purchase: a
/// bad response never silently counts as credit.
pub fn complete(
    journal: &mut Journal,
    holder: &Holder,
    tiers: &[u64],
    purchase: &PurchaseRow,
    issue_response: &[u8],
) -> Result<Collected, WalletError> {
    let stored = purchase.issue_state.as_ref().ok_or(WalletError::Corrupt("purchase has no issue state"))?;
    let finalized = holder
        .issue_state_from_bytes(stored.expose_secret())
        .and_then(|state| holder.finalize_issue(state, issue_response));
    let credential = match finalized {
        Ok(credential) => credential,
        Err(error) => {
            journal.freeze_purchase(purchase.id)?;
            return Err(error.into());
        }
    };
    // `finalize_issue` consumed the state in memory only: the journal still
    // holds it and the request, so a retry resends the same request.
    crash_point!("after_finalize_issue");
    let credits = credential.balance();
    let state = if metering::wallet::is_dust(tiers, credits) { ChainState::Dust } else { ChainState::Ready };
    let chain = journal.collect_purchase(purchase.id, &credential.to_bytes(), state)?;
    Ok(Collected { chain, credits, state })
}
