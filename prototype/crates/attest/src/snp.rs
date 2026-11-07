//! SEV-SNP on Verda: recognised by name, not verifiable in the prototype.
//!
//! The real verifier checks the AMD chain, VCEK, TCB and measurement (docs
//! 04 section 6). Until it exists this one refuses everything, so a
//! manifest listing only SNP releases fails closed instead of passing.

use manifest::Manifest;

use crate::verifier::{AttestationVerifier, Rejection, Verdict};
use crate::{Evidence, ReportBinding};

/// Rejects every SNP report with [`Rejection::Unsupported`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SnpVerifier;

impl AttestationVerifier for SnpVerifier {
    fn verify(&self, _: &Evidence, _: &ReportBinding, _: &Manifest) -> Result<Verdict, Rejection> {
        Err(Rejection::Unsupported)
    }
}
