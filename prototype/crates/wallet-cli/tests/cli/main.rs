//! `wallet-cli` as a user and `xtask` run it: the built binary in a temp
//! wallet directory, against a real storefront on fake-stripe (with a stub
//! gate behind its `issue.sock`) or a stub walletd on loopback.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a setup step that fails, or an unexpected answer, should fail the test at once"
)]

mod args;
mod harness;
mod purchase;
mod use_plane;
