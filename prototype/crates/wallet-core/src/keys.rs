//! The journal key, in `<wallet_dir>/journal.key` with mode 0600, standing
//! in for the OS keychain (SPEC section 7).

use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use rand::TryRng as _;
use secrecy::{ExposeSecret as _, ExposeSecretMut as _, SecretBox};

use crate::WalletError;

/// The raw 32-byte SQLCipher key. Never logged; wiped on drop.
pub(crate) struct JournalKey(SecretBox<[u8; 32]>);

impl JournalKey {
    /// Generates a key and writes it to a new file only the user can read.
    /// Refuses to replace an existing file: that would orphan a journal.
    pub(crate) fn create(path: &Path) -> Result<Self, WalletError> {
        let mut key = SecretBox::new(Box::new([0u8; 32]));
        rand::rngs::SysRng.try_fill_bytes(key.expose_secret_mut()).map_err(|_| WalletError::Entropy)?;
        let file_error = |source| WalletError::File { path: path.to_owned(), source };
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(file_error)?;
        file.write_all(key.expose_secret()).map_err(file_error)?;
        file.sync_all().map_err(file_error)?;
        Ok(Self(key))
    }

    /// Reads the key, refusing a file that others can read.
    pub(crate) fn read(path: &Path) -> Result<Self, WalletError> {
        let file_error = |source| WalletError::File { path: path.to_owned(), source };
        let mut file = OpenOptions::new().read(true).open(path).map_err(file_error)?;
        if file.metadata().map_err(file_error)?.mode() & 0o077 != 0 {
            return Err(WalletError::JournalKey("journal.key must be readable by its owner only (chmod 600)"));
        }
        let mut key = SecretBox::new(Box::new([0u8; 32]));
        file.read_exact(key.expose_secret_mut()).map_err(|_| WalletError::JournalKey("journal.key is too short"))?;
        if file.read(&mut [0u8; 1]).map_err(file_error)? != 0 {
            return Err(WalletError::JournalKey("journal.key is too long"));
        }
        Ok(Self(key))
    }

    /// The key bytes, for the `PRAGMA key` statement only.
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn creates_once_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.key");
        let created = JournalKey::create(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(JournalKey::read(&path).unwrap().expose(), created.expose());
        assert!(JournalKey::create(&path).is_err());
    }

    #[test]
    fn refuses_a_readable_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.key");
        JournalKey::create(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(JournalKey::read(&path), Err(WalletError::JournalKey(_))));
    }
}
