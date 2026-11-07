//! Signing manifests, for `xtask` and tests only (feature `sign`).
//!
//! Real releases are signed offline with the minisign tool; this produces
//! the same signature format, so verification can't tell the difference.

use std::fmt;

use proto::{Blob, SignedManifest};
use secrecy::SecretString;

use crate::ManifestError;
use crate::canonical::to_canonical_json;
use crate::signature::OperatorPublicKey;
use crate::types::Manifest;

/// The operator's minisign secret key.
///
/// minisign's own `Debug` prints the key bytes, so this wrapper exists to
/// keep them out of any log.
pub struct OperatorSecretKey(minisign::SecretKey);

impl OperatorSecretKey {
    /// A new, unencrypted key.
    pub fn generate() -> Result<Self, ManifestError> {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().map_err(|_| ManifestError::OperatorKey)?;
        Ok(Self(pair.sk))
    }

    /// Reads an unencrypted minisign secret key file's text.
    pub fn from_text(text: &str) -> Result<Self, ManifestError> {
        minisign::SecretKeyBox::from_string(text)
            .and_then(minisign::SecretKeyBox::into_unencrypted_secret_key)
            .map(Self)
            .map_err(|_| ManifestError::OperatorKey)
    }

    /// The secret key file's text, for writing with mode 0600.
    pub fn to_text(&self) -> Result<SecretString, ManifestError> {
        let key_box = self.0.to_box(None).map_err(|_| ManifestError::OperatorKey)?;
        Ok(SecretString::from(key_box.into_string()))
    }

    /// The matching public key.
    pub fn public_key(&self) -> Result<OperatorPublicKey, ManifestError> {
        let public = minisign::PublicKey::from_secret_key(&self.0).map_err(|_| ManifestError::OperatorKey)?;
        OperatorPublicKey::from_base64(&public.to_base64())
    }

    /// Signs arbitrary bytes, returning minisign's signature text.
    pub fn sign_bytes(&self, bytes: &[u8], trusted_comment: &str) -> Result<String, ManifestError> {
        let signature = minisign::sign(None, &self.0, bytes, Some(trusted_comment), None)
            .map_err(|_| ManifestError::OperatorKey)?;
        Ok(signature.into_string())
    }
}

impl fmt::Debug for OperatorSecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OperatorSecretKey([redacted])")
    }
}

/// Signs a manifest's canonical bytes.
pub fn sign_manifest(manifest: &Manifest, key: &OperatorSecretKey) -> Result<SignedManifest, ManifestError> {
    let bytes = to_canonical_json(manifest).map_err(|_| ManifestError::Malformed)?;
    let comment = format!("counteractive manifest {} version {}", manifest.service.id, manifest.version);
    let signature = key.sign_bytes(&bytes, &comment)?;
    Ok(SignedManifest { manifest: Blob::new(bytes), signature })
}
