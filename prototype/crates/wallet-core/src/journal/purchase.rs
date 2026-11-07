//! `purchase` rows, and the collect transaction that turns one into a chain.

use proto::walletd::{ChainState, PurchaseState};
use rusqlite::Row;
use secrecy::{ExposeSecret as _, SecretBox, SecretSlice};

use super::states::StateName as _;
use super::{ChainId, Journal, PurchaseId, from_sql, to_sql};
use crate::WalletError;

/// What an order needs journalled before it is created (docs 04 section 5, Buying).
pub struct NewPurchase {
    /// The secret whose hash names the order; it authorises collecting.
    pub order_secret: SecretBox<[u8; 32]>,
    /// Product id from the attested release.
    pub product: String,
    /// That product's credits.
    pub credits: u64,
    /// The client issuance state, consumed by `finalize_issue`.
    pub issue_state: SecretSlice<u8>,
    /// The issue request, resent unchanged on every collect.
    pub issue_request: Vec<u8>,
}

/// One purchase.
pub struct PurchaseRow {
    /// Local id.
    pub id: PurchaseId,
    /// The Checkout Session id, once the order exists.
    pub order_id: Option<String>,
    /// The order secret.
    pub order_secret: SecretBox<[u8; 32]>,
    /// Product id.
    pub product: String,
    /// Credits the product carries.
    pub credits: u64,
    /// The issuance state, until collect commits.
    pub issue_state: Option<SecretSlice<u8>>,
    /// The issue request, until collect commits.
    pub issue_request: Option<Vec<u8>>,
    /// Where it is in the state machine.
    pub state: PurchaseState,
    /// The chain collect created.
    pub chain_id: Option<ChainId>,
}

impl std::fmt::Debug for PurchaseRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PurchaseRow")
            .field("id", &self.id)
            .field("product", &self.product)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

const COLUMNS: &str =
    "purchase_id, order_id, order_secret, product, credits, issue_state, issue_request, state, chain_id";

impl Journal {
    /// Journals a new order as `Ordered`, before anything is sent.
    pub fn insert_purchase(&mut self, purchase: &NewPurchase) -> Result<PurchaseId, WalletError> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT INTO purchase (order_secret, product, credits, issue_state, issue_request, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    purchase.order_secret.expose_secret().as_slice(),
                    &purchase.product,
                    to_sql(purchase.credits)?,
                    purchase.issue_state.expose_secret(),
                    &purchase.issue_request,
                    PurchaseState::Ordered.name(),
                ),
            )?;
            Ok(PurchaseId(transaction.last_insert_rowid()))
        })
    }

    /// `Ordered` to `AwaitingPayment`, recording the storefront's order id.
    pub fn set_order_id(&mut self, id: PurchaseId, order_id: &str) -> Result<(), WalletError> {
        self.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE purchase SET order_id = ?2, state = ?3 WHERE purchase_id = ?1 AND state = ?4",
                (id.0, order_id, PurchaseState::AwaitingPayment.name(), PurchaseState::Ordered.name()),
            )?;
            expect_one(changed, id.0, "not Ordered")
        })
    }

    /// The purchases in any of `states`, oldest first.
    pub fn purchases_in(&self, states: &[PurchaseState]) -> Result<Vec<PurchaseRow>, WalletError> {
        let mut statement = self.connection.prepare(&format!("SELECT {COLUMNS} FROM purchase ORDER BY purchase_id"))?;
        let rows = statement.query_and_then([], read_row)?;
        let mut selected = Vec::new();
        for row in rows {
            let row = row?;
            if states.contains(&row.state) {
                selected.push(row);
            }
        }
        Ok(selected)
    }

    /// Moves an unfinished purchase to `Frozen`, keeping every column.
    pub fn freeze_purchase(&mut self, id: PurchaseId) -> Result<(), WalletError> {
        self.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE purchase SET state = ?2 WHERE purchase_id = ?1 AND state IN (?3, ?4)",
                (
                    id.0,
                    PurchaseState::Frozen.name(),
                    PurchaseState::Ordered.name(),
                    PurchaseState::AwaitingPayment.name(),
                ),
            )?;
            expect_one(changed, id.0, "already finished")
        })
    }

    /// The collect transaction: stores the credential as a new chain, marks
    /// the purchase `Collected` with that chain, and clears the issuance
    /// state and request, all at once (docs 04 section 5, Collecting).
    pub fn collect_purchase(
        &mut self,
        id: PurchaseId,
        credential: &SecretSlice<u8>,
        chain_state: ChainState,
    ) -> Result<ChainId, WalletError> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT INTO chain (state, credential) VALUES (?1, ?2)",
                (chain_state.name(), credential.expose_secret()),
            )?;
            let chain_id = transaction.last_insert_rowid();
            let changed = transaction.execute(
                "UPDATE purchase SET state = ?2, chain_id = ?3, issue_state = NULL, issue_request = NULL
                 WHERE purchase_id = ?1 AND state = ?4",
                (id.0, PurchaseState::Collected.name(), chain_id, PurchaseState::AwaitingPayment.name()),
            )?;
            expect_one(changed, id.0, "not AwaitingPayment")?;
            Ok(ChainId(chain_id))
        })
    }
}

fn read_row(row: &Row<'_>) -> Result<PurchaseRow, WalletError> {
    let secret: Vec<u8> = row.get(2)?;
    let order_secret: [u8; 32] = secret.as_slice().try_into().map_err(|_| WalletError::Corrupt("order secret"))?;
    Ok(PurchaseRow {
        id: PurchaseId(row.get(0)?),
        order_id: row.get(1)?,
        order_secret: SecretBox::new(Box::new(order_secret)),
        product: row.get(3)?,
        credits: from_sql(row.get(4)?, "purchase credits")?,
        issue_state: row.get::<_, Option<Vec<u8>>>(5)?.map(SecretSlice::from),
        issue_request: row.get(6)?,
        state: PurchaseState::from_name(&row.get::<_, String>(7)?)?,
        chain_id: row.get::<_, Option<i64>>(8)?.map(ChainId),
    })
}

/// A guarded update touched exactly one row, or the row was in another state.
pub(super) fn expect_one(changed: usize, id: i64, found: &str) -> Result<(), WalletError> {
    if changed == 1 { Ok(()) } else { Err(WalletError::WrongState { id, found: found.to_owned() }) }
}
