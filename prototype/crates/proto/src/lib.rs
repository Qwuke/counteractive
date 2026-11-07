//! Wire types shared by every Counteractive service and client.
//!
//! This crate does no IO (ARCHITECTURE 8.2 rule 1). It defines what goes on
//! the wire, so the gate, storefront, wallet and fakes agree by construction:
//!
//! - [`gate`]: `gate.sock` and `issue.sock` (SPEC section 5)
//! - [`storefront`]: the storefront's orders API (SPEC section 6)
//! - [`stripe`]: the two Checkout calls, written by the storefront and read by `fake-stripe`
//! - [`openai`]: the allowlisted chat subset, used by the gate, engine and walletd
//! - [`walletd`]: walletd's `/wallet` API (SPEC section 7)
//! - [`receipt`], [`error`], [`sse`] and [`encoding`]: shared pieces
//!
//! Binary fields use [`Blob`] or [`SecretBlob`], so `Debug` on any of these
//! types never prints a spend, a refund or a credential request.

pub mod encoding;
pub mod error;
pub mod gate;
pub mod openai;
pub mod receipt;
pub mod sse;
pub mod storefront;
pub mod stripe;
pub mod walletd;

pub use encoding::{B64Error, Blob, SecretBlob, SecretHex32, b64_decode, b64_decode_array, b64_encode};
pub use error::{ErrorBody, ErrorCode};
pub use gate::{SessionId, SessionPolicy, SignedManifest};
pub use receipt::{FinishReason, Receipt};
