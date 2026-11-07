//! The wallet's side of EC-ACT, with a byte encoding for every state.
//!
//! The wallet journals each state before sending the message that goes
//! with it, so a crash can always resume with identical bytes (docs 04
//! section 5). Encodings are secret: they're returned as [`SecretSlice`]
//! and the types' `Debug` prints nothing.

use std::fmt;

use act::{ClientIssuanceState, ClientSpendState, IssueResponseMessage, Params, PublicKey, RefundMessage};
use secrecy::SecretSlice;
use zeroize::Zeroizing;

use crate::{BALANCE_WIDTH, CreditError, IssuerPublicKey, ServiceContext};

/// Builds and finishes the wallet's credential operations for one service.
pub struct Holder {
    params: Params,
    issuer: PublicKey,
    ctx: ServiceContext,
}

impl Holder {
    /// A holder for credentials from `issuer` in `ctx`.
    pub fn new(ctx: &ServiceContext, issuer: &IssuerPublicKey) -> Result<Self, CreditError> {
        Ok(Self { params: Params::new(BALANCE_WIDTH)?, issuer: issuer.to_act()?, ctx: ctx.clone() })
    }

    /// A fresh issue request and the state that finishes it.
    pub fn issue_request(&self) -> Result<(IssueState, Vec<u8>), CreditError> {
        let (state, request) = act::issue_request(&self.params)?;
        Ok((IssueState(state), request.to_bytes()))
    }

    /// Checks the issuer's response and produces the credential.
    pub fn finalize_issue(&self, state: IssueState, response: &[u8]) -> Result<Credential, CreditError> {
        let response = IssueResponseMessage::from_bytes(&self.params, response)?;
        let credential = act::finalize_issue(&self.params, &self.issuer, self.ctx.ctx_cred(), state.0, &response)?;
        Ok(Credential(credential))
    }

    /// Spends `s` credits with allowance 0, consuming the credential.
    ///
    /// Journal the state and message before sending: a second proof from
    /// the same credential would reveal the same nullifier.
    pub fn prove_spend(&self, credential: Credential, s: u64) -> Result<(SpendState, Vec<u8>), CreditError> {
        let (state, spend) =
            act::prove_spend(&self.params, credential.0, self.ctx.ctx_cred(), s, 0, self.ctx.ctx_spend())?;
        Ok((SpendState(state), spend.to_bytes()))
    }

    /// Checks a refund and produces the next credential, worth `remainder + t`.
    pub fn finalize_refund(&self, state: SpendState, refund: &[u8]) -> Result<Credential, CreditError> {
        let refund = RefundMessage::from_bytes(&self.params, refund)?;
        let credential = act::finalize_refund(&self.params, &self.issuer, self.ctx.ctx_cred(), state.0, &refund)?;
        Ok(Credential(credential))
    }

    /// The `t` a refund message carries, read without verifying it.
    ///
    /// For the settlement record only: the balance always comes from
    /// [`Holder::finalize_refund`], which verifies.
    pub fn refund_amount(&self, refund: &[u8]) -> Result<u64, CreditError> {
        Ok(RefundMessage::from_bytes(&self.params, refund)?.return_amount())
    }

    /// The credits a spend message spends, read without verifying it.
    pub fn spend_amount(&self, spend: &[u8]) -> Result<u64, CreditError> {
        Ok(act::SpendMessage::from_bytes(&self.params, spend)?.amount())
    }

    /// Decodes a journalled issue state.
    pub fn issue_state_from_bytes(&self, bytes: &[u8]) -> Result<IssueState, CreditError> {
        Ok(IssueState(ClientIssuanceState::from_bytes(&self.params, bytes)?))
    }

    /// Decodes a journalled credential.
    pub fn credential_from_bytes(&self, bytes: &[u8]) -> Result<Credential, CreditError> {
        Ok(Credential(act::Credential::from_bytes(&self.params, bytes)?))
    }

    /// Decodes a journalled spend state.
    pub fn spend_state_from_bytes(&self, bytes: &[u8]) -> Result<SpendState, CreditError> {
        Ok(SpendState(ClientSpendState::from_bytes(&self.params, bytes)?))
    }
}

impl fmt::Debug for Holder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Holder")
            .field("service_id", &self.ctx.service_id())
            .field("key_epoch", &self.ctx.key_epoch())
            .finish_non_exhaustive()
    }
}

/// Moves act-rs's zeroizing encoding into a secret without leaving a copy behind.
fn into_secret(mut bytes: Zeroizing<Vec<u8>>) -> SecretSlice<u8> {
    SecretSlice::from(std::mem::take(&mut *bytes))
}

/// The state between sending an issue request and receiving the response.
pub struct IssueState(ClientIssuanceState);

impl IssueState {
    /// The secret encoding, for the journal.
    pub fn to_bytes(&self) -> SecretSlice<u8> {
        into_secret(self.0.to_bytes())
    }
}

/// A credential: the chain's current balance.
pub struct Credential(act::Credential);

impl Credential {
    /// The balance it carries.
    pub fn balance(&self) -> u64 {
        self.0.balance()
    }

    /// The secret encoding, for the journal.
    pub fn to_bytes(&self) -> SecretSlice<u8> {
        into_secret(self.0.to_bytes())
    }
}

/// The state between sending a spend and receiving its refund.
pub struct SpendState(ClientSpendState);

impl SpendState {
    /// The balance left after spending, before any refund.
    pub fn remainder(&self) -> u64 {
        self.0.remainder()
    }

    /// The secret encoding, for the journal.
    pub fn to_bytes(&self) -> SecretSlice<u8> {
        into_secret(self.0.to_bytes())
    }
}

macro_rules! redacted_debug {
    ($($name:ident),*) => {$(
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "([redacted])"))
            }
        }
    )*};
}

redacted_debug!(IssueState, Credential, SpendState);
