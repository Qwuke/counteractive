//! EC-ACT credentials for Counteractive, through `act-rs`.
//!
//! This is the only crate that names `act` types (ARCHITECTURE 8.2 rule 2).
//! [`Moderator`] is the gate's side and [`Holder`] the wallet's. Every
//! value crosses this boundary as bytes or as a wrapper with redacted
//! `Debug`, so no secret state leaks into a service's logs.
//!
//! Fixed choices (SPEC section 2): balance width `L = 24`, allowance
//! always 0, contexts from [`ServiceContext`].

mod context;
mod holder;
mod keys;
mod moderator;

pub use context::ServiceContext;
pub use holder::{Credential, Holder, IssueState, SpendState};
pub use keys::{IssuerPublicKey, Nullifier, PUBLIC_KEY_LENGTH, SEED_LENGTH, SecretSeed, SpendFacts};
pub use moderator::Moderator;

/// Balance width `L`: balances and amounts are below `2^24`.
pub const BALANCE_WIDTH: u8 = 24;

/// The largest balance a credential can hold: 16,777,215 credits.
pub const MAX_BALANCE: u64 = (1 << BALANCE_WIDTH) - 1;

/// The scheme id manifests name in `service.credit_scheme`.
pub const SCHEME_ID: &str = "ec-act-p256-v0";

/// Why a credential operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CreditError {
    /// Bytes aren't a canonical encoding of the expected message or state.
    #[error("malformed credential message")]
    Decode,
    /// A proof or signature didn't verify, for instance under another context or key.
    #[error("credential message failed verification")]
    Verify,
    /// An amount is out of range, or a refund exceeds its spend.
    #[error("amount out of range")]
    Amount,
    /// A spend asked for a nonzero allowance.
    #[error("spend allowance must be 0")]
    Allowance,
    /// The service id is empty or longer than 255 bytes.
    #[error("service id must be 1 to 255 bytes")]
    ServiceId,
    /// The system random number generator failed.
    #[error("no entropy available")]
    Entropy,
    /// An input was rejected, or an internal derivation failed.
    #[error("invalid credential input")]
    InvalidInput,
}

impl From<act::Error> for CreditError {
    fn from(error: act::Error) -> Self {
        match error {
            act::Error::Deserialize => Self::Decode,
            act::Error::Verify => Self::Verify,
            act::Error::Amount => Self::Amount,
            act::Error::Entropy => Self::Entropy,
            _ => Self::InvalidInput,
        }
    }
}

#[cfg(test)]
mod tests;
