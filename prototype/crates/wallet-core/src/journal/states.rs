//! How states are spelled in the journal. Spelled out here rather than
//! borrowed from a `Debug` or serde form, so a rename elsewhere can't
//! silently strand rows.

use proto::walletd::{ChainState, PurchaseState, SettlementOutcome};

use crate::WalletError;

/// A state stored by name.
pub(crate) trait StateName: Sized + Copy + PartialEq + 'static {
    /// Every value with its stored name.
    const NAMES: &'static [(Self, &'static str)];

    /// The stored name. Every value is listed in [`StateName::NAMES`].
    fn name(self) -> &'static str {
        Self::NAMES.iter().find(|(value, _)| *value == self).map_or("?", |(_, name)| *name)
    }

    /// The value a stored name means.
    fn from_name(name: &str) -> Result<Self, WalletError> {
        Self::NAMES
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(value, _)| *value)
            .ok_or(WalletError::Corrupt("unknown state name"))
    }
}

impl StateName for PurchaseState {
    const NAMES: &'static [(Self, &'static str)] = &[
        (Self::Ordered, "Ordered"),
        (Self::AwaitingPayment, "AwaitingPayment"),
        (Self::Collected, "Collected"),
        (Self::Frozen, "Frozen"),
        (Self::Abandoned, "Abandoned"),
    ];
}

impl StateName for ChainState {
    const NAMES: &'static [(Self, &'static str)] = &[
        (Self::Ready, "Ready"),
        (Self::Dust, "Dust"),
        (Self::Spending, "Spending"),
        (Self::Open, "Open"),
        (Self::Closing, "Closing"),
        (Self::Recovering, "Recovering"),
        (Self::Frozen, "Frozen"),
    ];
}

impl StateName for SettlementOutcome {
    const NAMES: &'static [(Self, &'static str)] = &[
        (Self::Exact, "exact"),
        (Self::FullRefund, "full_refund"),
        (Self::ReceiptGap, "receipt_gap"),
        (Self::Mismatch, "mismatch"),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_round_trips() {
        for (state, name) in ChainState::NAMES {
            assert_eq!(state.name(), *name);
            assert_eq!(ChainState::from_name(name).unwrap(), *state);
        }
        for (state, name) in PurchaseState::NAMES {
            assert_eq!(PurchaseState::from_name(name).unwrap(), *state);
        }
        assert_eq!(SettlementOutcome::FullRefund.name(), "full_refund");
        ChainState::from_name("Bogus").unwrap_err();
    }
}
