//! Tier 1: the gate in this process, over its real sockets, against an
//! in-process fake engine, with temp directories only.
//!
//! One test binary, so the gate and its store link once.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a setup step that fails, or an unexpected answer, should fail the test at once"
)]

mod boot;
mod harness;
mod issuance;
mod recovery;
mod sessions;
