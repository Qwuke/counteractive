//! The gate: the service inside the confidential VM (SPEC section 5,
//! docs 05).
//!
//! It serves inner TLS on `gate.sock` (health, manifest, attestation,
//! sessions, operator calls) and plain HTTP on `issue.sock` (issuance for
//! the storefront), and reaches its inference engine on one more socket.
//! It has no outbound client (ARCHITECTURE 8.2 rule 6).
//!
//! Modules follow docs 05 section 1. Handlers in [`api`] parse and respond;
//! [`state`] drives the boot states; [`sessions`] and [`issue`] hold the
//! request flows; [`store`] is the only code that touches the disk.
//! Draining, export and import are not built yet, so the states are
//! Locked, `AwaitingManifest` and Ready.

mod admission;
mod api;
mod attestation;
mod config;
mod custody;
mod issue;
mod models;
mod seal;
mod serve;
mod sessions;
mod state;
mod store;

pub use config::GateOptions;
pub use serve::{RunningGate, start};

/// Runs blocking work, a store write or a proof, on the blocking pool so
/// it never stalls the async workers. A panic there is re-raised here.
#[expect(
    clippy::panic,
    reason = "only a runtime that is shutting down cancels a blocking task, and no caller is left to answer"
)]
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(work).await {
        Ok(value) => value,
        Err(error) if error.is_cancelled() => panic!("blocking task cancelled: the runtime is shutting down"),
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}
