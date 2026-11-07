//! The gate's side of EC-ACT: issuing, verifying spends, refunding.

use std::fmt;

use act::{IssueRequestMessage, Params, PublicKey, SecretKey, SpendMessage};

use crate::{BALANCE_WIDTH, CreditError, IssuerPublicKey, Nullifier, SecretSeed, ServiceContext, SpendFacts};

/// Holds the issuer key for one service and epoch. Does no IO.
pub struct Moderator {
    params: Params,
    key: SecretKey,
    public: PublicKey,
    ctx: ServiceContext,
}

impl Moderator {
    /// Derives the issuer key from the stored seed.
    pub fn from_seed(seed: &SecretSeed, ctx: &ServiceContext) -> Result<Self, CreditError> {
        let params = Params::new(BALANCE_WIDTH)?;
        let key = SecretKey::from_seed(seed.expose())?;
        let public = key.public_key();
        Ok(Self { params, key, public, ctx: ctx.clone() })
    }

    /// The key wallets verify issuance and refunds against.
    pub fn public_key(&self) -> IssuerPublicKey {
        IssuerPublicKey::from_act(&self.public)
    }

    /// Answers an issue request with a credential of `credits`.
    ///
    /// Each call is randomised, so the gate stores the response and returns
    /// the stored bytes for a repeated request.
    pub fn issue(&self, credits: u64, request: &[u8]) -> Result<Vec<u8>, CreditError> {
        let request = IssueRequestMessage::from_bytes(&self.params, request)?;
        let verified = act::verify_issue_request(&self.params, &request)?;
        let response = act::issue_response(&self.key, self.ctx.ctx_cred(), credits, verified)?;
        Ok(response.to_bytes())
    }

    /// Decodes a spend and returns its nullifier, without verifying the proof.
    ///
    /// Cheap, so the gate can look the nullifier up before spending a full
    /// verification on it. Malformed bytes fail here; altered but
    /// well-formed bytes pass and fail later in [`Moderator::verify_spend`].
    pub fn peek_nullifier(&self, spend: &[u8]) -> Result<Nullifier, CreditError> {
        let spend = SpendMessage::from_bytes(&self.params, spend)?;
        Ok(Nullifier::from_bytes(spend.nullifier()))
    }

    /// Verifies a spend and returns its nullifier and amount.
    ///
    /// Rejects any allowance other than 0, so no refund can raise a balance.
    /// act-rs's verified value borrows the message and can't be kept, so
    /// [`Moderator::refund`] verifies again.
    pub fn verify_spend(&self, spend: &[u8]) -> Result<SpendFacts, CreditError> {
        let spend = SpendMessage::from_bytes(&self.params, spend)?;
        let verified = self.verify(&spend)?;
        Ok(SpendFacts { nullifier: Nullifier::from_bytes(verified.nullifier()), budget: verified.amount() })
    }

    /// Verifies a spend again and refunds `t` of its `s` credits.
    pub fn refund(&self, spend: &[u8], t: u64) -> Result<Vec<u8>, CreditError> {
        let spend = SpendMessage::from_bytes(&self.params, spend)?;
        let verified = self.verify(&spend)?;
        if t > verified.amount() {
            return Err(CreditError::Amount);
        }
        Ok(act::issue_refund(verified, t)?.to_bytes())
    }

    fn verify<'a>(&'a self, spend: &'a SpendMessage) -> Result<act::VerifiedSpend<'a>, CreditError> {
        if spend.allowance() != 0 {
            return Err(CreditError::Allowance);
        }
        let verified = act::verify_spend(&self.params, &self.key, self.ctx.ctx_cred(), self.ctx.ctx_spend(), spend)?;
        Ok(verified)
    }
}

impl fmt::Debug for Moderator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Moderator")
            .field("service_id", &self.ctx.service_id())
            .field("key_epoch", &self.ctx.key_epoch())
            .field("public_key", &self.public_key())
            .finish_non_exhaustive()
    }
}
