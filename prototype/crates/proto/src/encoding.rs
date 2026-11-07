//! Binary-in-JSON encodings (SPEC section 2).
//!
//! Binary values travel as base64url without padding, and hashes as
//! lowercase hex. Decoding is strict for base64 (no padding, no stray
//! trailing bits), so one value has exactly one encoding, and lenient about
//! hex case.
//!
//! Binary payloads are wrapped in [`Blob`] or [`SecretBlob`] rather than
//! carried as `Vec<u8>`, so that `Debug` on a request or response never
//! prints a spend, a refund or a credential request.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret as _, SecretBox, SecretSlice};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A base64url string that didn't decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid base64url (unpadded) encoding")]
pub struct B64Error;

/// Encodes bytes as base64url without padding.
pub fn b64_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes base64url without padding, rejecting padded or non-canonical input.
pub fn b64_decode(text: &str) -> Result<Vec<u8>, B64Error> {
    URL_SAFE_NO_PAD.decode(text).map_err(|_| B64Error)
}

/// Decodes base64url into exactly `N` bytes.
pub fn b64_decode_array<const N: usize>(text: &str) -> Result<[u8; N], B64Error> {
    b64_decode(text)?.try_into().map_err(|_| B64Error)
}

/// Decodes hex (either case) into exactly `N` bytes.
pub fn hex_decode_array<const N: usize>(text: &str) -> Result<[u8; N], hex::FromHexError> {
    let mut out = [0u8; N];
    hex::decode_to_slice(text, &mut out)?;
    Ok(out)
}

/// Serde adapter for a fixed-size array as lowercase hex.
///
/// Use as `#[serde(with = "proto::encoding::hex_array")]`.
pub mod hex_array {
    use super::*;

    /// Writes the array as lowercase hex.
    pub fn serialize<S: Serializer, const N: usize>(value: &[u8; N], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    /// Reads exactly `N` bytes of hex.
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
        let text = String::deserialize(deserializer)?;
        hex_decode_array(&text).map_err(D::Error::custom)
    }
}

/// Serde adapter for a fixed-size array as base64url without padding.
///
/// Use as `#[serde(with = "proto::encoding::b64_array")]`.
pub mod b64_array {
    use super::*;

    /// Writes the array as unpadded base64url.
    pub fn serialize<S: Serializer, const N: usize>(value: &[u8; N], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&b64_encode(value))
    }

    /// Reads exactly `N` bytes of unpadded base64url.
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
        let text = String::deserialize(deserializer)?;
        b64_decode_array(&text).map_err(D::Error::custom)
    }
}

/// Public binary data: base64url on the wire, and only its length in `Debug`.
///
/// Used for refunds, issue requests and responses, reports, and signed
/// manifest bytes. None of these is a secret on its own, but several reveal
/// amounts or link requests, so they stay out of logs.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Blob(Vec<u8>);

impl Blob {
    /// Wraps bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The bytes, by value.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Blob {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Blob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blob({} bytes)", self.0.len())
    }
}

impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&b64_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        b64_decode(&text).map(Self).map_err(D::Error::custom)
    }
}

/// Secret binary data: base64url on the wire, redacted in `Debug`, wiped on drop.
///
/// Used for spend messages. Their hash derives the key that seals a stored
/// refund (SPEC section 2), so they must never reach a log.
#[derive(Clone)]
pub struct SecretBlob(SecretSlice<u8>);

impl SecretBlob {
    /// Wraps secret bytes, taking ownership so no unwiped copy is left behind.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(SecretSlice::from(bytes))
    }

    /// The secret bytes. Callers must not log or persist them unprotected.
    pub fn expose(&self) -> &[u8] {
        self.0.expose_secret()
    }
}

impl From<Vec<u8>> for SecretBlob {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Debug for SecretBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBlob([redacted])")
    }
}

impl Serialize for SecretBlob {
    /// Exposes the secret deliberately: serialising is how it goes on the wire.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&b64_encode(self.expose()))
    }
}

impl<'de> Deserialize<'de> for SecretBlob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        b64_decode(&text).map(Self::new).map_err(D::Error::custom)
    }
}

/// A 32-byte secret carried as hex, redacted in `Debug`, wiped on drop.
///
/// Used for the order secret that authorises collecting a paid order.
pub struct SecretHex32(SecretBox<[u8; 32]>);

impl SecretHex32 {
    /// Wraps a secret.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    /// The secret bytes.
    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

impl Clone for SecretHex32 {
    fn clone(&self) -> Self {
        Self::new(*self.expose())
    }
}

impl fmt::Debug for SecretHex32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretHex32([redacted])")
    }
}

impl Serialize for SecretHex32 {
    /// Exposes the secret deliberately: serialising is how it goes on the wire.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(self.expose()))
    }
}

impl<'de> Deserialize<'de> for SecretHex32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        hex_decode_array(&text).map(Self::new).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Hashes {
        #[serde(with = "hex_array")]
        hash: [u8; 4],
        #[serde(with = "b64_array")]
        nonce: [u8; 3],
    }

    #[test]
    fn b64_is_unpadded_url_safe() {
        assert_eq!(b64_encode(&[0xfb, 0xff]), "-_8");
        assert_eq!(b64_decode("-_8").unwrap(), vec![0xfb, 0xff]);
    }

    #[test]
    fn b64_rejects_padding_and_standard_alphabet() {
        assert_eq!(b64_decode("-_8="), Err(B64Error));
        assert_eq!(b64_decode("+/8"), Err(B64Error));
    }

    #[test]
    fn b64_rejects_noncanonical_trailing_bits() {
        // "-_8" is canonical for [0xfb, 0xff]; "-_9" sets bits that don't exist.
        assert_eq!(b64_decode("-_9"), Err(B64Error));
    }

    #[test]
    fn array_adapters_round_trip_and_check_length() {
        let value = Hashes { hash: [0xde, 0xad, 0xbe, 0xef], nonce: [1, 2, 3] };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#"{"hash":"deadbeef","nonce":"AQID"}"#);
        assert_eq!(serde_json::from_str::<Hashes>(&json).unwrap(), value);
        serde_json::from_str::<Hashes>(r#"{"hash":"DEADBEEF","nonce":"AQID"}"#).unwrap();
        serde_json::from_str::<Hashes>(r#"{"hash":"deadbe","nonce":"AQID"}"#).unwrap_err();
        serde_json::from_str::<Hashes>(r#"{"hash":"deadbeef","nonce":"AQIDBA"}"#).unwrap_err();
    }

    #[test]
    fn blob_debug_shows_only_length() {
        let blob = Blob::new(vec![7; 12]);
        assert_eq!(format!("{blob:?}"), "Blob(12 bytes)");
        assert_eq!(serde_json::to_string(&blob).unwrap(), "\"BwcHBwcHBwcHBwcH\"");
    }

    #[test]
    fn secrets_round_trip_but_never_debug() {
        let spend = SecretBlob::new(vec![1, 2, 3]);
        let json = serde_json::to_string(&spend).unwrap();
        assert_eq!(json, "\"AQID\"");
        let back: SecretBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), &[1, 2, 3]);
        assert_eq!(format!("{back:?}"), "SecretBlob([redacted])");

        let secret = SecretHex32::new([0xab; 32]);
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
        let back: SecretHex32 = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), &[0xab; 32]);
        assert!(!format!("{back:?}").contains("ab"));
    }
}
