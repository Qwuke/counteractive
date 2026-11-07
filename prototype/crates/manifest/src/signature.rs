//! Checking a signed manifest: signature first, then canonical form, then parsing.

use std::fmt;

use proto::SignedManifest;

use crate::ManifestError;
use crate::canonical::is_canonical;
use crate::types::{Manifest, SCHEMA};

/// The operator's minisign public key, from `release.json` or a wallet pin.
#[derive(Clone, PartialEq, Eq)]
pub struct OperatorPublicKey {
    key: minisign_verify::PublicKey,
    base64: String,
}

impl OperatorPublicKey {
    /// Parses minisign's base64 public key form (the second line of a `.pub` file).
    pub fn from_base64(text: &str) -> Result<Self, ManifestError> {
        let text = text.trim();
        let key = minisign_verify::PublicKey::from_base64(text).map_err(|_| ManifestError::OperatorKey)?;
        Ok(Self { key, base64: text.to_owned() })
    }

    /// The base64 form.
    pub fn as_base64(&self) -> &str {
        &self.base64
    }
}

impl fmt::Debug for OperatorPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OperatorPublicKey({})", self.base64)
    }
}

/// Verifies a signed manifest and parses it.
///
/// The minisign signature is checked over the exact bytes first, with
/// legacy (non-prehashed) signatures refused. The bytes must then be
/// canonical JSON, so one manifest has one encoding, and the schema known.
pub fn verify_signed(signed: &SignedManifest, key: &OperatorPublicKey) -> Result<Manifest, ManifestError> {
    #[derive(serde::Deserialize)]
    struct SchemaOnly {
        schema: u32,
    }
    let bytes = signed.manifest.as_bytes();
    let signature = minisign_verify::Signature::decode(&signed.signature).map_err(|_| ManifestError::Signature)?;
    key.key.verify(bytes, &signature, false).map_err(|_| ManifestError::Signature)?;
    if !is_canonical(bytes) {
        return Err(ManifestError::NotCanonical);
    }
    let SchemaOnly { schema } = serde_json::from_slice(bytes).map_err(|_| ManifestError::Malformed)?;
    if schema != SCHEMA {
        return Err(ManifestError::UnknownSchema(schema));
    }
    serde_json::from_slice(bytes).map_err(|_| ManifestError::Malformed)
}
