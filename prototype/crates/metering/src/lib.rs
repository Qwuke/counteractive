//! Credit arithmetic for turns, sessions and settlement.
//!
//! Domain crate, no IO (ARCHITECTURE 8.2 rule 1). The gate uses
//! [`SessionLedger`] to admit, hold and debit turns; the wallet uses the
//! functions in [`wallet`] to size sessions and check settlements. All
//! arithmetic is checked or saturating: an overflow refuses, never wraps.

mod ledger;
mod turn;
pub mod wallet;

pub use ledger::{SessionLedger, TurnCharge};
pub use turn::{ModelTerms, Refusal, TurnEnd, TurnPlan, plan_turn, turn_debit};

/// Invalid metering input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MeteringError {
    /// A model's output rate or output cap is zero.
    #[error("a model's output rate and output cap must be at least 1")]
    InvalidTerms,
}
