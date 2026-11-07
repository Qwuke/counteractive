//! walletd against a gate running in this process ([`testkit`]): the local
//! API and its money flows, walletd's own idle close, and the redial after
//! a stalled connection. The unit tests of single modules live beside them.

mod api;
mod idle;
mod redial;
mod testkit;
