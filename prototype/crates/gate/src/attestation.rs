//! `POST /v1/attestation`: evidence that binds this boot's TLS key to the
//! measured release (SPEC section 3).
//!
//! Reports go one at a time behind a short queue (docs 05 section 7): on
//! SEV-SNP each is a firmware call of unknown rate. The dev source only
//! signs, so it runs inline.

use attest::{AttestationSource, Evidence};
use gate_conn::server::ServerIdentity;
use proto::ErrorCode;

use crate::admission::Admission;

/// Reports produced at once.
const REPORTS_AT_ONCE: usize = 1;
/// Report requests allowed to wait; more get `503 busy`.
const REPORTS_WAITING: usize = 16;

/// This boot's TLS identity and the source of its evidence.
pub struct Attestor {
    identity: ServerIdentity,
    source: Box<dyn AttestationSource>,
    admission: Admission,
}

impl Attestor {
    /// An attestor for `identity`, with reports from `source`.
    pub fn new(identity: ServerIdentity, source: impl AttestationSource + 'static) -> Self {
        Self { identity, source: Box::new(source), admission: Admission::new(REPORTS_AT_ONCE, REPORTS_WAITING) }
    }

    /// The TLS identity `gate.sock` serves.
    pub fn identity(&self) -> &ServerIdentity {
        &self.identity
    }

    /// Evidence for a client's nonce, or `503 busy` when the queue is full.
    pub async fn evidence(&self, nonce: [u8; 32]) -> Result<Evidence, ErrorCode> {
        let _permit = self.admission.enter().await.ok_or(ErrorCode::Busy)?;
        self.source.evidence(&self.identity.binding(nonce)).map_err(|error| {
            tracing::warn!(%error, "attestation report failed");
            ErrorCode::Internal
        })
    }
}
