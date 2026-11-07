//! The wallet's state, with no network code: the SQLCipher journal, the chain
//! state machine (ARCHITECTURE section 11), the balance view and reconcile
//! (docs 04 sections 7 and 10) and the manifest store with its accept rule.
//!
//! `wallet-cli` and `walletd` both open the same journal and keep to the
//! writing rules of docs 04 section 1: `wallet-cli` writes purchases and
//! the `Ready` chain a collect creates, `walletd` writes chains from then
//! on. Every money step is journalled before the bytes it produced leave
//! the process, so a crash at any point loses nothing.

pub mod chain;
pub mod journal;
mod keys;
pub mod manifest_store;
pub mod pins;
pub mod purchase;
pub mod reconcile;

pub use journal::{ChainId, ChainRow, Journal, PurchaseId, PurchaseRow};
pub use pins::Pins;

/// Why a wallet operation failed.
///
/// No variant carries secret material: SQLite errors name statements, not
/// bound values, and credit errors carry no bytes.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    /// The journal database failed.
    #[error("journal: {0}")]
    Journal(#[from] rusqlite::Error),
    /// A wallet file couldn't be read or written.
    #[error("{path}: {source}")]
    File {
        /// The file.
        path: std::path::PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// The journal key file is missing, the wrong size, or readable by others.
    #[error("journal key: {0}")]
    JournalKey(&'static str),
    /// The journal already exists, so `init` would overwrite money.
    #[error("a journal already exists in this wallet directory")]
    AlreadyInitialised,
    /// `wallet.toml` is missing a pin or has an invalid one.
    #[error("wallet.toml: {0}")]
    Pins(String),
    /// A credit operation failed: a bad issue response, refund or stored state.
    #[error("credit: {0}")]
    Credit(#[from] credit::CreditError),
    /// A manifest was refused by the accept rule.
    #[error("manifest: {0}")]
    Manifest(#[from] manifest::ManifestError),
    /// No manifest has been accepted yet.
    #[error("no accepted manifest: run `wallet-cli manifest fetch --approve` first")]
    NoAcceptedManifest,
    /// There is no fetched manifest waiting for approval.
    #[error("no manifest is waiting for approval")]
    NoPendingManifest,
    /// A row isn't in a state the requested transition starts from.
    #[error("chain or purchase {id} is not in a state that allows this ({found})")]
    WrongState {
        /// The local id.
        id: i64,
        /// The state found.
        found: String,
    },
    /// A journal row contradicts itself, such as a Ready chain without a credential.
    #[error("journal row is inconsistent: {0}")]
    Corrupt(&'static str),
    /// The operating system's random number generator failed.
    #[error("no randomness available")]
    Entropy,
}
