//! The local API's bearer token, in `<wallet_dir>/walletd.token` with mode
//! 0600 (SPEC section 7). Browsers can reach localhost, so every route
//! but the page and `/paid` needs it.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use anyhow::{Context as _, bail};
use rand::TryRng as _;
use secrecy::{ExposeSecret as _, SecretString};
use subtle::ConstantTimeEq as _;
use zeroize::Zeroizing;

/// The token's file name inside the wallet directory.
pub const TOKEN_FILE: &str = "walletd.token";

/// Random bytes in a token, written as lowercase hex.
const TOKEN_BYTES: usize = 32;

/// The bearer token.
pub struct Token(SecretString);

impl Token {
    /// Reads the token, creating it on first run.
    pub fn load_or_create(wallet_dir: &Path) -> anyhow::Result<Self> {
        let path = wallet_dir.join(TOKEN_FILE);
        let mut bytes = Zeroizing::new([0u8; TOKEN_BYTES]);
        rand::rngs::SysRng.try_fill_bytes(bytes.as_mut()).context("no randomness for the token")?;
        let fresh = Zeroizing::new(hex::encode(bytes.as_ref()));
        match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path) {
            Ok(mut file) => {
                file.write_all(fresh.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
                Ok(Self(SecretString::from(fresh.as_str())))
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Self::read(&path),
            Err(error) => Err(error).with_context(|| format!("creating {}", path.display())),
        }
    }

    /// Reads an existing token. Refuses a file other users can read, as
    /// the journal key does, and one that doesn't hold a token walletd
    /// wrote: an empty file would make every protected route unusable.
    #[expect(
        clippy::verbose_file_reads,
        reason = "reading the handle whose mode was checked, so a swapped path can't slip in between"
    )]
    fn read(path: &Path) -> anyhow::Result<Self> {
        let mut file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
        if file.metadata().with_context(|| format!("reading {}", path.display()))?.mode() & 0o077 != 0 {
            bail!("{} must be readable by its owner only (chmod 600)", path.display());
        }
        let mut text = Zeroizing::new(String::new());
        file.read_to_string(&mut text).with_context(|| format!("reading {}", path.display()))?;
        let token = text.trim();
        if token.len() != 2 * TOKEN_BYTES || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("{} does not hold a walletd token; delete it and walletd writes a new one", path.display());
        }
        Ok(Self(SecretString::from(token)))
    }

    /// Whether `candidate` is the token, compared in constant time.
    pub fn matches(&self, candidate: &str) -> bool {
        let expected = self.0.expose_secret().as_bytes();
        expected.len() == candidate.len() && bool::from(expected.ct_eq(candidate.as_bytes()))
    }

    /// The token, for embedding in the page served at the right host.
    pub fn expose(&self) -> &str {
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
        let created = Token::load_or_create(dir.path()).unwrap();
        let path = dir.path().join(TOKEN_FILE);
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let read = Token::load_or_create(dir.path()).unwrap();
        assert!(read.matches(created.expose()));
    }

    #[test]
    fn refuses_a_token_others_can_read_or_an_empty_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TOKEN_FILE);
        Token::load_or_create(dir.path()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let error = Token::load_or_create(dir.path()).err().expect("a readable token must be refused");
        assert!(error.to_string().contains("chmod 600"), "{error}");

        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let error = Token::load_or_create(dir.path()).err().expect("an empty token must be refused");
        assert!(error.to_string().contains("does not hold a walletd token"), "{error}");
    }
}
