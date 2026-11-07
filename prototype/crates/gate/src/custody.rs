//! Key custody: how K, the money store's key, reaches the gate (docs 03
//! section 5.3).
//!
//! The PoC's only custody is `OperatorDelivered`: the operator sends
//! `hex(K)` as a bearer token over an attested connection, and the gate
//! accepts it only if `SHA-256(K)` equals the hash pinned in the measured
//! `release.json`. Every operator endpoint authenticates the same way.

use std::fmt;

use secrecy::{ExposeSecret as _, SecretBox};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use zeroize::Zeroizing;

/// K: 32 bytes, the raw SQLCipher key of the money store.
pub struct StoreKey(SecretBox<[u8; 32]>);

impl StoreKey {
    /// The key bytes, for `PRAGMA key`.
    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

impl fmt::Debug for StoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StoreKey([redacted])")
    }
}

/// Checks operator bearer tokens against `release.json`'s `k_sha256`.
#[derive(Debug, Clone)]
pub struct OperatorDelivered {
    k_sha256: [u8; 32],
}

impl OperatorDelivered {
    /// Custody pinned to `SHA-256(K)`.
    pub fn new(k_sha256: [u8; 32]) -> Self {
        Self { k_sha256 }
    }

    /// K from a bearer token `hex(K)`, if its hash matches the pin.
    ///
    /// The hashes are compared in constant time. A token that isn't 64 hex
    /// digits is refused before hashing; its length is no secret.
    pub fn accept(&self, token: &str) -> Option<StoreKey> {
        let mut key = Zeroizing::new([0u8; 32]);
        hex::decode_to_slice(token.trim(), key.as_mut()).ok()?;
        let digest: [u8; 32] = Sha256::digest(key.as_slice()).into();
        if !bool::from(digest.ct_eq(&self.k_sha256)) {
            return None;
        }
        Some(StoreKey(SecretBox::init_with_mut(|stored: &mut [u8; 32]| stored.copy_from_slice(key.as_slice()))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_pinned_key() {
        let k = [7u8; 32];
        let custody = OperatorDelivered::new(Sha256::digest(k).into());
        assert_eq!(custody.accept(&hex::encode(k)).unwrap().expose(), &k);
        assert!(custody.accept(&hex::encode([8u8; 32])).is_none());
        assert!(custody.accept(&hex::encode([7u8; 31])).is_none());
        assert!(custody.accept("not hex").is_none());
        assert_eq!(format!("{:?}", custody.accept(&hex::encode(k)).unwrap()), "StoreKey([redacted])");
    }
}
