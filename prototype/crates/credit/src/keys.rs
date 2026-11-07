//! Issuer key material and the identifiers derived from spends.

use std::fmt;

use act::{Backend as _, DefaultBackend};
use secrecy::{ExposeSecret as _, SecretBox};

use crate::CreditError;

/// Length of the issuer seed.
pub const SEED_LENGTH: usize = act::SEED_LENGTH;

/// Length of a compressed issuer public key.
pub const PUBLIC_KEY_LENGTH: usize = 33;

/// The 48-byte seed the issuer key is derived from. Lives in the gate's store.
pub struct SecretSeed(SecretBox<[u8; SEED_LENGTH]>);

impl SecretSeed {
    /// A fresh seed from the system random number generator.
    pub fn generate() -> Result<Self, CreditError> {
        let seed = DefaultBackend::generate_seed().map_err(act::Error::from)?;
        Ok(Self(SecretBox::new(Box::new(*seed))))
    }

    /// A seed read back from storage.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CreditError> {
        let seed: [u8; SEED_LENGTH] = bytes.try_into().map_err(|_| CreditError::Decode)?;
        Ok(Self(SecretBox::new(Box::new(seed))))
    }

    /// The seed bytes, for writing to the store. Never log them.
    pub fn expose(&self) -> &[u8; SEED_LENGTH] {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SecretSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretSeed([redacted])")
    }
}

/// The issuer's public key: a compressed P-256 point, checked on construction.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IssuerPublicKey([u8; PUBLIC_KEY_LENGTH]);

impl IssuerPublicKey {
    /// Parses and checks a compressed point.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CreditError> {
        let array: [u8; PUBLIC_KEY_LENGTH] = bytes.try_into().map_err(|_| CreditError::Decode)?;
        act::PublicKey::<DefaultBackend>::from_bytes(&array)?;
        Ok(Self(array))
    }

    /// The compressed point.
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.0
    }

    pub(crate) fn to_act(self) -> Result<act::PublicKey, CreditError> {
        Ok(act::PublicKey::from_bytes(&self.0)?)
    }

    pub(crate) fn from_act(key: &act::PublicKey) -> Self {
        Self(key.to_bytes())
    }
}

impl fmt::Debug for IssuerPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IssuerPublicKey({})", hex::encode(self.0))
    }
}

/// A spend's nullifier: the store's key for that spend.
///
/// Public on the wire, but it identifies a session at the gate, so it
/// stays out of `Debug` and logs.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nullifier([u8; 32]);

impl Nullifier {
    /// Wraps stored bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The bytes, as the store's primary key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Nullifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Nullifier([redacted])")
    }
}

/// What the gate learns from a verified spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendFacts {
    /// The spend's nullifier.
    pub nullifier: Nullifier,
    /// `s`, the amount spent into the session.
    pub budget: u64,
}
