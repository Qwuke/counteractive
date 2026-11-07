//! Sealing stored refunds (SPEC section 2, docs 05 section 4).
//!
//! A refund carries `t`, and the operator holding K must not read it from
//! the store at rest. The seal key derives from the spend bytes, which the
//! gate holds only during a session and the wallet presents again to
//! recover; neither they nor their hash are ever stored.
//!
//! ```text
//! seal_key = HKDF-SHA256(salt = "counteractive-seal-v1", ikm = SHA-256(spend_bytes), info = "refund")
//! sealed   = nonce(24) || XChaCha20-Poly1305(seal_key, nonce, refund_bytes)
//! ```

use chacha20poly1305::aead::Aead as _;
use chacha20poly1305::{Key, KeyInit as _, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::TryRng as _;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

const SALT: &[u8] = b"counteractive-seal-v1";
const INFO: &[u8] = b"refund";
const NONCE_LENGTH: usize = 24;

/// Sealing needs a fresh nonce, and the system's generator failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no entropy for the seal nonce")]
pub struct SealError;

/// Seals `refund` under a key derived from `spend`.
pub fn seal(spend: &[u8], refund: &[u8]) -> Result<Vec<u8>, SealError> {
    let mut nonce = [0u8; NONCE_LENGTH];
    rand::rngs::SysRng.try_fill_bytes(&mut nonce).map_err(|_| SealError)?;
    let ciphertext = cipher(spend).encrypt(&XNonce::from(nonce), refund).map_err(|_| SealError)?;
    let mut sealed = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Opens a sealed refund with `spend`'s key; `None` if these bytes didn't seal it.
///
/// That `None` is how a different proof for a settled nullifier is told
/// apart from a retry: only the original spend bytes open the row.
pub fn unseal(spend: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LENGTH)?;
    let nonce: [u8; NONCE_LENGTH] = nonce.try_into().ok()?;
    cipher(spend).decrypt(&XNonce::from(nonce), ciphertext).ok()
}

fn cipher(spend: &[u8]) -> XChaCha20Poly1305 {
    let ikm = Zeroizing::new(<[u8; 32]>::from(Sha256::digest(spend)));
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(SALT), ikm.as_slice())
        .expand(INFO, key.as_mut())
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    XChaCha20Poly1305::new(&Key::from(*key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_the_same_spend_only() {
        let refund = vec![9u8; 137];
        let sealed = seal(b"spend one", &refund).unwrap();
        assert_eq!(sealed.len(), NONCE_LENGTH + refund.len() + 16);
        assert_eq!(unseal(b"spend one", &sealed).unwrap(), refund);
        assert!(unseal(b"spend two", &sealed).is_none());
        assert!(unseal(b"spend one", &sealed[..10]).is_none());
    }

    #[test]
    fn each_seal_uses_a_fresh_nonce() {
        assert_ne!(seal(b"spend", b"refund").unwrap(), seal(b"spend", b"refund").unwrap());
    }

    /// Pins the derivation to SPEC section 2 by computing it step by step.
    #[test]
    fn key_derivation_matches_the_spec() {
        let spend = b"spend bytes";
        let sealed = seal(spend, b"refund").unwrap();
        let ikm: [u8; 32] = Sha256::digest(spend).into();
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(Some(b"counteractive-seal-v1"), &ikm).expand(b"refund", &mut key).unwrap();
        let nonce: [u8; 24] = sealed[..24].try_into().unwrap();
        let opened = XChaCha20Poly1305::new(&Key::from(key)).decrypt(&XNonce::from(nonce), &sealed[24..]).unwrap();
        assert_eq!(opened, b"refund");
    }
}
