//! One walletd per wallet directory.
//!
//! Which chains are being driven is known only inside one process, so a
//! second walletd on the same journal would take a first one's live
//! sessions for orphans and settle them from under it. An exclusive lock
//! on `<wallet_dir>/walletd.lock`, held for the process's life, refuses
//! the second. The operating system releases it however the process ends.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use anyhow::{Context as _, bail};

/// The lock file's name inside the wallet directory.
const LOCK_FILE: &str = "walletd.lock";

/// The held lock; dropping it releases the wallet directory.
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Takes the wallet directory's lock, or fails if another walletd holds it.
    pub fn acquire(wallet_dir: &Path) -> anyhow::Result<Self> {
        let path = wallet_dir.join(LOCK_FILE);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => {
                bail!("another walletd is already serving {}; stop it first", wallet_dir.display())
            }
            Err(TryLockError::Error(error)) => Err(error).with_context(|| format!("locking {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_walletd_is_refused_until_the_first_stops() {
        let dir = tempfile::tempdir().unwrap();
        let first = InstanceLock::acquire(dir.path()).unwrap();
        let second = InstanceLock::acquire(dir.path()).err().expect("a second walletd must be refused");
        assert!(second.to_string().contains("already serving"), "{second}");
        drop(first);
        InstanceLock::acquire(dir.path()).unwrap();
    }
}
