//! The money store: the issuer seed, issuance records and spent
//! nullifiers (docs 05 section 4, docs 03 section 5.6).
//!
//! Methods are blocking and take `&self`: an implementation serialises
//! its own writes, and callers run writes on the blocking pool. Reads are
//! cheap enough to run under the slot lock, where opening and recovering
//! need them (docs 05 section 5).

mod files;
mod sqlcipher;

use credit::{Nullifier, SecretSeed};

pub use files::{read_manifest, write_manifest};
pub use sqlcipher::{Opened, SqlCipherStore};

/// `SHA-256(order_id)`, the issuance idempotency key.
pub type OrderRef = [u8; 32];

/// The store's single issuer row.
pub struct IssuerRow {
    /// The 48-byte seed the issuer key derives from.
    pub seed: SecretSeed,
    /// The epoch the issuer's contexts derive from.
    pub key_epoch: u32,
    /// The highest full manifest accepted; 0 before the first.
    pub manifest_version: u64,
}

/// A nullifier's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpendStatus {
    /// Never seen.
    NoRow,
    /// A session opened and hasn't settled. With no live slot, the gate crashed.
    Pending,
    /// Settled; the sealed refund.
    Settled(Vec<u8>),
}

/// The result of a compare-and-set settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettleOutcome {
    /// This call's sealed refund is now the row's.
    Won,
    /// Another settlement got there first; the caller unseals this with its own bytes.
    Lost {
        /// The winner's sealed refund.
        sealed_refund: Vec<u8>,
    },
}

/// A stored issuance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedRow {
    /// SHA-256 of the issue request it answered.
    pub request_sha256: [u8; 32],
    /// The issue response, returned again for the same request.
    pub response: Vec<u8>,
}

/// The result of recording an issuance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueOutcome {
    /// The row is new.
    Recorded,
    /// A row for the same request appeared meanwhile; its response stands.
    Existing {
        /// The stored response.
        response: Vec<u8>,
    },
    /// A row for a different request exists: `409 order_issued`.
    Conflict,
}

/// Why a store operation failed. Messages carry no row contents.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite or SQLCipher failed, including a wrong key.
    #[error("store database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// A file or directory operation failed.
    #[error("store file error: {0}")]
    Io(#[from] std::io::Error),
    /// The store's layout or a row isn't what this build writes.
    #[error("store is malformed: {0}")]
    Malformed(&'static str),
}

/// The money store's operations (docs 03 section 5.6, with `record_issue` for vouchers).
pub trait MoneyStore: Send + Sync {
    /// The issuer row.
    fn issuer(&self) -> Result<IssuerRow, StoreError>;

    /// Records a newly accepted manifest version; never lowers it.
    fn set_manifest_version(&self, version: u64) -> Result<(), StoreError>;

    /// The row for a nullifier.
    fn spend_status(&self, nullifier: &Nullifier) -> Result<SpendStatus, StoreError>;

    /// Inserts a Pending row and commits with a full sync. Fails if a row exists.
    fn insert_pending(&self, nullifier: &Nullifier) -> Result<(), StoreError>;

    /// Compare-and-set: a Pending row is settled and a missing row is
    /// inserted settled; a settled row is left alone and returned.
    fn settle(&self, nullifier: &Nullifier, sealed_refund: &[u8]) -> Result<SettleOutcome, StoreError>;

    /// The issuance recorded for an order, if any.
    fn issued(&self, order_ref: &OrderRef) -> Result<Option<IssuedRow>, StoreError>;

    /// Records an issuance unless one exists for the order, in one
    /// immediate transaction that looks the order up again first.
    fn record_issue(
        &self,
        order_ref: &OrderRef,
        request_sha256: &[u8; 32],
        response: &[u8],
    ) -> Result<IssueOutcome, StoreError>;
}
