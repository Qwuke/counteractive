//! The journal: SQLCipher in WAL mode with `synchronous=FULL`, keyed by a
//! raw random key from `journal.key` (docs 04 section 4, SPEC section 7).
//!
//! Both wallet processes open it at once, so every write runs in an
//! immediate transaction and waits up to [`BUSY_TIMEOUT`] for the other.
//! The submodules hold the row operations per table; the state machine
//! that decides which to call lives in [`crate::purchase`] and [`crate::chain`].

mod chain;
mod manifest;
mod purchase;
mod receipts;
mod states;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use zeroize::Zeroizing;

pub use chain::ChainRow;
pub use manifest::{ManifestSlot, StoredManifest};
pub use purchase::{NewPurchase, PurchaseRow};

use crate::WalletError;
use crate::keys::JournalKey;

/// The journal database's file name inside the wallet directory.
pub const JOURNAL_FILE: &str = "journal.db";
/// The journal key's file name inside the wallet directory.
pub const KEY_FILE: &str = "journal.key";

/// How long a write waits for the other wallet process's write lock.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA: &str = include_str!("schema.sql");

/// A purchase's local id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PurchaseId(pub i64);

/// A chain's local id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChainId(pub i64);

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An open journal.
pub struct Journal {
    connection: Connection,
}

impl Journal {
    /// Creates the key and an empty journal in `wallet_dir`, refusing if
    /// either exists already.
    pub fn create(wallet_dir: &Path) -> Result<Self, WalletError> {
        let database = wallet_dir.join(JOURNAL_FILE);
        if database.exists() {
            return Err(WalletError::AlreadyInitialised);
        }
        let key = JournalKey::create(&wallet_dir.join(KEY_FILE))?;
        let mut journal = Self { connection: connect(&database, &key)? };
        journal.write(|transaction| Ok(transaction.execute_batch(SCHEMA)?))?;
        Ok(journal)
    }

    /// Opens an existing journal.
    pub fn open(wallet_dir: &Path) -> Result<Self, WalletError> {
        let database = wallet_dir.join(JOURNAL_FILE);
        if !database.is_file() {
            return Err(WalletError::File {
                path: database,
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no journal: run `wallet-cli init` first"),
            });
        }
        let key = JournalKey::read(&wallet_dir.join(KEY_FILE))?;
        Ok(Self { connection: connect(&database, &key)? })
    }

    /// The journal file inside `wallet_dir`.
    pub fn path(wallet_dir: &Path) -> PathBuf {
        wallet_dir.join(JOURNAL_FILE)
    }

    /// Runs `body` in one immediate transaction, committing only if it succeeds.
    fn write<T>(&mut self, body: impl FnOnce(&Transaction<'_>) -> Result<T, WalletError>) -> Result<T, WalletError> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = body(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }
}

impl fmt::Debug for Journal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Journal").finish_non_exhaustive()
    }
}

/// Opens the database with the raw key and the durability pragmas, and
/// reads it once so a wrong key fails here rather than at first use.
fn connect(database: &Path, key: &JournalKey) -> Result<Connection, WalletError> {
    let connection = Connection::open(database)?;
    let pragma = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", Zeroizing::new(hex::encode(key.expose())).as_str()));
    connection.execute_batch(&pragma)?;
    connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA secure_delete = ON;")?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))?;
    Ok(connection)
}

/// SQLite integers are signed; credits, counts and versions never exceed `i64`.
fn to_sql(value: u64) -> Result<i64, WalletError> {
    i64::try_from(value).map_err(|_| WalletError::Corrupt("value out of range"))
}

/// The reverse of [`to_sql`], rejecting a negative stored value.
fn from_sql<T: TryFrom<i64>>(value: i64, what: &'static str) -> Result<T, WalletError> {
    T::try_from(value).map_err(|_| WalletError::Corrupt(what))
}

#[cfg(test)]
mod tests;
