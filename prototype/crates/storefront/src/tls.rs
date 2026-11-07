//! This boot's inner-TLS key, and evidence binding it with role `storefront`.
//!
//! The key is generated in memory at each start and never written down, so
//! the only thing vouching for it is a report that binds its SPKI hash. The
//! role in that binding is what stops this storefront's evidence standing
//! in for the gate's, though both run the same image.

use attest::{AttestError, AttestationSource, Evidence, Role};
use gate_conn::server::{IdentityError, ServerIdentity};

/// The storefront's TLS identity and the source that attests to it.
pub struct Attestor {
    identity: ServerIdentity,
    source: Box<dyn AttestationSource>,
}

impl Attestor {
    /// Generates a fresh TLS key for role `storefront`, attested by `source`.
    pub fn generate(source: Box<dyn AttestationSource>) -> Result<Self, IdentityError> {
        Ok(Self { identity: ServerIdentity::generate(Role::Storefront)?, source })
    }

    /// Evidence for a client's `nonce`, binding this boot's TLS key.
    pub fn evidence(&self, nonce: [u8; 32]) -> Result<Evidence, AttestError> {
        self.source.evidence(&self.identity.binding(nonce))
    }

    /// The TLS key and certificate a serve loop accepts with.
    pub fn identity(&self) -> &ServerIdentity {
        &self.identity
    }
}

impl std::fmt::Debug for Attestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attestor").field("identity", &self.identity).field("platform", &self.source.platform()).finish()
    }
}
