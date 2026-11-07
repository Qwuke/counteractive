//! The money store on SQLCipher, inside a generation directory.
//!
//! Layout (docs 05 section 3, Import):
//!
//! ```text
//! <data>/store/current -> gen-1     relative symlink naming the live generation
//! <data>/store/gen-1/gate.db        the store, with its own -wal and -shm beside it
//! ```
//!
//! Each generation keeps its WAL in its own directory, so a store is only
//! ever opened with its own `-wal` (docs 05 invariant 13). Creating the
//! first store writes and syncs `gen-1` completely before the `current`
//! link exists, so a crash part way leaves no store, and the next unlock
//! starts over.

use std::fs::{self, DirBuilder, File};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use credit::{Nullifier, SecretSeed};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior};
use zeroize::Zeroizing;

use super::{IssueOutcome, IssuedRow, IssuerRow, MoneyStore, OrderRef, SettleOutcome, SpendStatus, StoreError};
use crate::custody::StoreKey;

/// The directory under `<data>` holding generations, the link and the manifest files.
pub const STORE_DIR: &str = "store";
const CURRENT_LINK: &str = "current";
const FIRST_GENERATION: &str = "gen-1";
const DATABASE_FILE: &str = "gate.db";
const SCHEMA: &str = include_str!("schema.sql");

/// How long a connection waits for another's write lock before failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether unlocking found a store or made one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// A new store with a fresh issuer seed.
    Created,
    /// The existing live generation.
    Existing,
}

/// The SQLCipher store, with one connection for writes and one for reads.
///
/// In WAL mode a read never waits for a write's sync, so reads under the
/// slot lock stay fast while a commit is in flight on the writer.
pub struct SqlCipherStore {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
}

impl SqlCipherStore {
    /// Opens the live store under `data`, or creates the first generation
    /// with a fresh issuer seed for `key_epoch` if there is none.
    pub fn open_or_create(data: &Path, key: &StoreKey, key_epoch: u32) -> Result<(Self, Opened), StoreError> {
        let store_dir = data.join(STORE_DIR);
        create_private_dir(&store_dir)?;
        let link = store_dir.join(CURRENT_LINK);
        let (database, opened) = if fs::symlink_metadata(&link).is_ok() {
            (live_generation(&store_dir)?.join(DATABASE_FILE), Opened::Existing)
        } else {
            (create_first_generation(&store_dir, key, key_epoch)?, Opened::Created)
        };
        if !database.is_file() {
            return Err(StoreError::Malformed("the live generation has no database"));
        }
        let store = Self { writer: Mutex::new(connect(&database, key)?), reader: Mutex::new(connect(&database, key)?) };
        Ok((store, opened))
    }

    fn writer(&self) -> MutexGuard<'_, Connection> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reader(&self) -> MutexGuard<'_, Connection> {
        self.reader.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl MoneyStore for SqlCipherStore {
    fn issuer(&self) -> Result<IssuerRow, StoreError> {
        let (seed, key_epoch, manifest_version) = self.reader().query_row(
            "SELECT seed, key_epoch, manifest_version FROM issuer WHERE id = 1",
            [],
            |row| Ok((Zeroizing::new(row.get::<_, Vec<u8>>(0)?), row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
        )?;
        Ok(IssuerRow {
            seed: SecretSeed::from_bytes(&seed).map_err(|_| StoreError::Malformed("issuer seed"))?,
            key_epoch: u32::try_from(key_epoch).map_err(|_| StoreError::Malformed("key epoch"))?,
            manifest_version: u64::try_from(manifest_version).map_err(|_| StoreError::Malformed("manifest version"))?,
        })
    }

    fn set_manifest_version(&self, version: u64) -> Result<(), StoreError> {
        let version = i64::try_from(version).map_err(|_| StoreError::Malformed("manifest version out of range"))?;
        self.writer()
            .execute("UPDATE issuer SET manifest_version = ?1 WHERE id = 1 AND manifest_version <= ?1", [version])?;
        Ok(())
    }

    fn spend_status(&self, nullifier: &Nullifier) -> Result<SpendStatus, StoreError> {
        let row: Option<Option<Vec<u8>>> = self
            .reader()
            .query_row("SELECT sealed_refund FROM spend WHERE nullifier = ?1", [nullifier.as_bytes()], |row| row.get(0))
            .optional()?;
        Ok(match row {
            None => SpendStatus::NoRow,
            Some(None) => SpendStatus::Pending,
            Some(Some(sealed)) => SpendStatus::Settled(sealed),
        })
    }

    fn insert_pending(&self, nullifier: &Nullifier) -> Result<(), StoreError> {
        let mut writer = self.writer();
        let transaction = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction
            .execute("INSERT INTO spend (nullifier, sealed_refund) VALUES (?1, NULL)", [nullifier.as_bytes()])?;
        transaction.commit()?;
        Ok(())
    }

    fn settle(&self, nullifier: &Nullifier, sealed_refund: &[u8]) -> Result<SettleOutcome, StoreError> {
        let mut writer = self.writer();
        let transaction = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "INSERT INTO spend (nullifier, sealed_refund) VALUES (?1, ?2)
             ON CONFLICT (nullifier) DO UPDATE SET sealed_refund = excluded.sealed_refund
             WHERE spend.sealed_refund IS NULL",
            (nullifier.as_bytes(), sealed_refund),
        )?;
        let outcome = if changed == 1 {
            SettleOutcome::Won
        } else {
            let sealed_refund = transaction.query_row(
                "SELECT sealed_refund FROM spend WHERE nullifier = ?1 AND sealed_refund IS NOT NULL",
                [nullifier.as_bytes()],
                |row| row.get(0),
            )?;
            SettleOutcome::Lost { sealed_refund }
        };
        transaction.commit()?;
        Ok(outcome)
    }

    fn issued(&self, order_ref: &OrderRef) -> Result<Option<IssuedRow>, StoreError> {
        Ok(self
            .reader()
            .query_row("SELECT request_sha256, response FROM issued WHERE order_ref = ?1", [order_ref], |row| {
                Ok(IssuedRow { request_sha256: row.get(0)?, response: row.get(1)? })
            })
            .optional()?)
    }

    fn record_issue(
        &self,
        order_ref: &OrderRef,
        request_sha256: &[u8; 32],
        response: &[u8],
    ) -> Result<IssueOutcome, StoreError> {
        let mut writer = self.writer();
        let transaction = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<([u8; 32], Vec<u8>)> = transaction
            .query_row("SELECT request_sha256, response FROM issued WHERE order_ref = ?1", [order_ref], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let outcome = match existing {
            Some((hash, response)) if &hash == request_sha256 => IssueOutcome::Existing { response },
            Some(_) => IssueOutcome::Conflict,
            None => {
                transaction.execute(
                    "INSERT INTO issued (order_ref, request_sha256, response) VALUES (?1, ?2, ?3)",
                    (order_ref, request_sha256, response),
                )?;
                IssueOutcome::Recorded
            }
        };
        transaction.commit()?;
        Ok(outcome)
    }
}

/// The directory `current` names, checked to be a `gen-N` sibling.
fn live_generation(store_dir: &Path) -> Result<PathBuf, StoreError> {
    let target = fs::read_link(store_dir.join(CURRENT_LINK))?;
    let name = target.to_str().ok_or(StoreError::Malformed("current link"))?;
    let number = name.strip_prefix("gen-").ok_or(StoreError::Malformed("current link"))?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(StoreError::Malformed("current link"));
    }
    Ok(store_dir.join(name))
}

/// Writes `gen-1` with a fresh issuer, syncs it, then links `current` to it.
fn create_first_generation(store_dir: &Path, key: &StoreKey, key_epoch: u32) -> Result<PathBuf, StoreError> {
    let generation = store_dir.join(FIRST_GENERATION);
    if generation.exists() {
        // Left by a create that crashed before linking; it was never live.
        fs::remove_dir_all(&generation)?;
    }
    create_private_dir(&generation)?;
    let database = generation.join(DATABASE_FILE);
    {
        let mut connection = connect(&database, key)?;
        fs::set_permissions(&database, fs::Permissions::from_mode(0o600))?;
        let seed = SecretSeed::generate().map_err(|_| StoreError::Malformed("no entropy for the issuer seed"))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(SCHEMA)?;
        transaction.execute(
            "INSERT INTO issuer (id, seed, key_epoch, manifest_version) VALUES (1, ?1, ?2, 0)",
            (seed.expose().as_slice(), i64::from(key_epoch)),
        )?;
        transaction.commit()?;
    }
    sync_dir(&generation)?;

    let staged = store_dir.join(format!("{CURRENT_LINK}.new"));
    if fs::symlink_metadata(&staged).is_ok() {
        fs::remove_file(&staged)?;
    }
    symlink(FIRST_GENERATION, &staged)?;
    fs::rename(&staged, store_dir.join(CURRENT_LINK))?;
    sync_dir(store_dir)?;
    Ok(database)
}

/// Opens a connection with the raw key K and the store's pragmas, and
/// reads the schema once so a wrong key fails here rather than later.
fn connect(database: &Path, key: &StoreKey) -> Result<Connection, StoreError> {
    let connection = Connection::open(database)?;
    let hex_key = Zeroizing::new(hex::encode(key.expose()));
    let pragma = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", hex_key.as_str()));
    connection.execute_batch(&pragma)?;
    connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA secure_delete = ON;")?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))?;
    Ok(connection)
}

/// Creates a directory, and any missing parents, readable by the gate only.
/// An existing directory is fine.
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)
}

/// Syncs a directory, so renames and new entries in it are durable.
fn sync_dir(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custody::OperatorDelivered;
    use sha2::{Digest as _, Sha256};

    fn key(byte: u8) -> StoreKey {
        let k = [byte; 32];
        OperatorDelivered::new(Sha256::digest(k).into()).accept(&hex::encode(k)).unwrap()
    }

    fn nullifier(byte: u8) -> Nullifier {
        Nullifier::from_bytes([byte; 32])
    }

    #[test]
    fn creates_once_then_opens_the_same_issuer() {
        let dir = tempfile::tempdir().unwrap();
        let (store, opened) = SqlCipherStore::open_or_create(dir.path(), &key(1), 7).unwrap();
        assert_eq!(opened, Opened::Created);
        let seed = *store.issuer().unwrap().seed.expose();
        drop(store);

        let (store, opened) = SqlCipherStore::open_or_create(dir.path(), &key(1), 7).unwrap();
        assert_eq!(opened, Opened::Existing);
        let issuer = store.issuer().unwrap();
        assert_eq!((issuer.seed.expose(), issuer.key_epoch, issuer.manifest_version), (&seed, 7, 0));

        let link = fs::read_link(dir.path().join("store/current")).unwrap();
        assert_eq!(link, Path::new("gen-1"), "the link is relative");
        let mode = fs::metadata(dir.path().join("store/gen-1/gate.db")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_wrong_key_fails_to_open() {
        let dir = tempfile::tempdir().unwrap();
        drop(SqlCipherStore::open_or_create(dir.path(), &key(1), 1).unwrap());
        assert!(SqlCipherStore::open_or_create(dir.path(), &key(2), 1).is_err());
    }

    #[test]
    fn an_unlinked_generation_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("store/gen-1")).unwrap();
        fs::write(dir.path().join("store/gen-1/gate.db"), b"half-written").unwrap();
        let (_, opened) = SqlCipherStore::open_or_create(dir.path(), &key(1), 1).unwrap();
        assert_eq!(opened, Opened::Created);
    }

    #[test]
    fn settlement_is_compare_and_set() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = SqlCipherStore::open_or_create(dir.path(), &key(1), 1).unwrap();
        assert_eq!(store.spend_status(&nullifier(1)).unwrap(), SpendStatus::NoRow);

        store.insert_pending(&nullifier(1)).unwrap();
        assert_eq!(store.spend_status(&nullifier(1)).unwrap(), SpendStatus::Pending);
        assert!(store.insert_pending(&nullifier(1)).is_err(), "a second open can't reset a row");

        assert_eq!(store.settle(&nullifier(1), b"first").unwrap(), SettleOutcome::Won);
        assert_eq!(
            store.settle(&nullifier(1), b"second").unwrap(),
            SettleOutcome::Lost { sealed_refund: b"first".to_vec() }
        );
        assert_eq!(store.spend_status(&nullifier(1)).unwrap(), SpendStatus::Settled(b"first".to_vec()));

        // A missing row is inserted settled: the out-of-policy path.
        assert_eq!(store.settle(&nullifier(2), b"full").unwrap(), SettleOutcome::Won);
        assert_eq!(store.spend_status(&nullifier(2)).unwrap(), SpendStatus::Settled(b"full".to_vec()));
    }

    #[test]
    fn issuance_is_keyed_by_order() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = SqlCipherStore::open_or_create(dir.path(), &key(1), 1).unwrap();
        let order = [3u8; 32];
        assert_eq!(store.issued(&order).unwrap(), None);
        assert_eq!(store.record_issue(&order, &[1; 32], b"response").unwrap(), IssueOutcome::Recorded);
        assert_eq!(
            store.record_issue(&order, &[1; 32], b"other").unwrap(),
            IssueOutcome::Existing { response: b"response".to_vec() }
        );
        assert_eq!(store.record_issue(&order, &[2; 32], b"other").unwrap(), IssueOutcome::Conflict);
        assert_eq!(
            store.issued(&order).unwrap(),
            Some(IssuedRow { request_sha256: [1; 32], response: b"response".to_vec() })
        );
    }

    #[test]
    fn manifest_version_only_rises() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = SqlCipherStore::open_or_create(dir.path(), &key(1), 1).unwrap();
        store.set_manifest_version(3).unwrap();
        store.set_manifest_version(2).unwrap();
        assert_eq!(store.issuer().unwrap().manifest_version, 3);
    }
}
