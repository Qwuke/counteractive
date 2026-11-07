//! Attestation evidence and its verification (SPEC section 3).
//!
//! Domain crate, no IO (ARCHITECTURE 8.2 rule 1). A service builds
//! [`Evidence`] for a [`ReportBinding`] through an [`AttestationSource`]; a
//! client checks it with an [`AttestationVerifier`] against the binding *it*
//! expects (its role, its nonce, the SPKI it saw in the handshake) and the
//! manifest it accepted.

mod binding;
mod dev;
mod evidence;
mod snp;
mod verifier;

pub use binding::{NO_GPU_EVIDENCE, ReportBinding, Role};
pub use dev::{DevAttestationKey, DevInsecureSource, DevInsecureVerifier, DevKeyError};
pub use evidence::Evidence;
pub use snp::SnpVerifier;
pub use verifier::{AttestError, AttestationSource, AttestationVerifier, PlatformVerifier, Rejection, Verdict};

#[cfg(test)]
mod tests;
