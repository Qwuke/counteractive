//! The two act-rs contexts (SPEC section 2, docs 07 section 7).
//!
//! Both come from values the wallet compiles in and the gate reads from
//! `release.json` and its store, never from a manifest: a hostile signer
//! could otherwise hand each user their own context and link their spends.

use sha2::{Digest as _, Sha256};

use crate::CreditError;

const CRED_LABEL: &[u8] = b"counteractive-cred-v1";
const SPEND_LABEL: &[u8] = b"counteractive-spend-v1";

/// A service id and key epoch, with the contexts derived from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContext {
    service_id: String,
    key_epoch: u32,
    ctx_cred: [u8; 32],
    ctx_spend: [u8; 32],
}

impl ServiceContext {
    /// Derives the contexts. The id must be 1 to 255 bytes, so its one-byte
    /// length prefix keeps one id and epoch from colliding with another.
    pub fn new(service_id: &str, key_epoch: u32) -> Result<Self, CreditError> {
        let len = u8::try_from(service_id.len()).map_err(|_| CreditError::ServiceId)?;
        if len == 0 {
            return Err(CreditError::ServiceId);
        }
        let ctx_cred = Sha256::new()
            .chain_update(CRED_LABEL)
            .chain_update([len])
            .chain_update(service_id)
            .chain_update(key_epoch.to_be_bytes())
            .finalize()
            .into();
        let ctx_spend =
            Sha256::new().chain_update(SPEND_LABEL).chain_update([len]).chain_update(service_id).finalize().into();
        Ok(Self { service_id: service_id.to_owned(), key_epoch, ctx_cred, ctx_spend })
    }

    /// The service id.
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    /// The key epoch.
    pub fn key_epoch(&self) -> u32 {
        self.key_epoch
    }

    /// `ctx_cred`, binding credentials to this service and issuer key.
    pub fn ctx_cred(&self) -> &[u8; 32] {
        &self.ctx_cred
    }

    /// `ctx_spend`, binding spend proofs to this service.
    pub fn ctx_spend(&self) -> &[u8; 32] {
        &self.ctx_spend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_answers() {
        // Computed independently with Python's hashlib.
        let ctx = ServiceContext::new("counteractive-poc", 1).unwrap();
        assert_eq!(hex::encode(ctx.ctx_cred()), "4e4a018c715585e68230acd2e3156bd68591d199d3336e8a78df737128b958d5");
        assert_eq!(hex::encode(ctx.ctx_spend()), "df1527b27ebe53e9e748eb613789f931b0a248968b386543762eca1e1cd198e9");
    }

    #[test]
    fn epoch_changes_only_the_credential_context() {
        let one = ServiceContext::new("svc", 1).unwrap();
        let two = ServiceContext::new("svc", 2).unwrap();
        assert_ne!(one.ctx_cred(), two.ctx_cred());
        assert_eq!(one.ctx_spend(), two.ctx_spend());
    }

    #[test]
    fn service_id_length_is_bounded() {
        assert_eq!(ServiceContext::new("", 1), Err(CreditError::ServiceId));
        assert_eq!(ServiceContext::new(&"x".repeat(256), 1), Err(CreditError::ServiceId));
        ServiceContext::new(&"x".repeat(255), 1).unwrap();
    }
}
