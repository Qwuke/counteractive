//! The wallet's arithmetic: session sizing, balance contributions and
//! settlement classification (docs 04 sections 7 and 10).

use proto::walletd::SettlementOutcome;

/// Credits per US dollar: `usd-1` buys 1,000,000 credits.
pub const CREDITS_PER_USD: u64 = 1_000_000;

/// `s`: the largest tier not above `min(balance, maximum_spend)`.
///
/// `None` means no tier fits. When that's because the balance is below the
/// smallest tier, the chain is `Dust`.
pub fn session_budget(tiers: &[u64], balance: u64, maximum_spend: u64) -> Option<u64> {
    let cap = balance.min(maximum_spend);
    tiers.iter().copied().filter(|&tier| tier > 0 && tier <= cap).max()
}

/// Whether a balance is below every tier, so the chain can't spend again.
pub fn is_dust(tiers: &[u64], balance: u64) -> bool {
    tiers.iter().all(|&tier| balance < tier)
}

/// An open session's contribution to the total: what the credential kept,
/// plus what the session has left by the last receipt, or `s` before one.
pub fn in_session_value(remainder: u64, budget: u64, last_receipt_remaining: Option<u64>) -> u64 {
    remainder.saturating_add(last_receipt_remaining.unwrap_or(budget).min(budget))
}

/// An upper bound on output tokens: credits divided by the output rate.
pub fn output_tokens_at_most(credits: u64, output_rate: u64) -> u64 {
    credits.checked_div(output_rate).unwrap_or(0)
}

/// Credits as dollars with six decimal places, such as `4.998980`.
pub fn format_usd(credits: u64) -> String {
    format!("{}.{:06}", credits / CREDITS_PER_USD, credits % CREDITS_PER_USD)
}

/// Classifies a settled session by comparing receipts with `s − t`.
///
/// `unknown_turns` counts turns whose stream ended without a receipt.
pub fn classify_settlement(s: u64, t: u64, receipts_sum: u64, unknown_turns: u32) -> SettlementOutcome {
    let Some(charged) = s.checked_sub(t) else {
        return SettlementOutcome::Mismatch;
    };
    if receipts_sum == charged {
        SettlementOutcome::Exact
    } else if t == s {
        // Receipts were present but nothing was kept, as after a gate crash.
        SettlementOutcome::FullRefund
    } else if receipts_sum < charged && unknown_turns > 0 {
        SettlementOutcome::ReceiptGap
    } else {
        SettlementOutcome::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LADDER: [u64; 3] = [500_000, 50_000, 5_000];

    #[test]
    fn sizing_follows_the_ladder() {
        assert_eq!(session_budget(&LADDER, 5_000_000, 500_000), Some(500_000));
        assert_eq!(session_budget(&LADDER, 499_999, 500_000), Some(50_000));
        assert_eq!(session_budget(&LADDER, 5_000_000, 60_000), Some(50_000));
        assert_eq!(session_budget(&LADDER, 5_000, 500_000), Some(5_000));
        assert_eq!(session_budget(&LADDER, 4_999, 500_000), None);
        assert!(is_dust(&LADDER, 4_999));
        assert!(!is_dust(&LADDER, 5_000));
        // A low maximum_spend leaves no tier without making the chain dust.
        assert_eq!(session_budget(&LADDER, 5_000_000, 1_000), None);
        assert!(!is_dust(&LADDER, 5_000_000));
    }

    #[test]
    fn balance_contributions() {
        assert_eq!(in_session_value(4_500_000, 500_000, None), 5_000_000);
        assert_eq!(in_session_value(4_500_000, 500_000, Some(499_931)), 4_999_931);
        assert_eq!(output_tokens_at_most(4_998_980, 3), 1_666_326);
        assert_eq!(output_tokens_at_most(10, 0), 0);
        assert_eq!(format_usd(4_998_980), "4.998980");
        assert_eq!(format_usd(5_000_000), "5.000000");
        assert_eq!(format_usd(7), "0.000007");
    }

    #[test]
    fn settlement_classes() {
        use SettlementOutcome::*;
        assert_eq!(classify_settlement(500_000, 499_931, 69, 0), Exact);
        assert_eq!(classify_settlement(500_000, 500_000, 0, 0), Exact);
        assert_eq!(classify_settlement(500_000, 500_000, 69, 0), FullRefund);
        assert_eq!(classify_settlement(500_000, 499_900, 69, 1), ReceiptGap);
        assert_eq!(classify_settlement(500_000, 499_900, 69, 0), Mismatch);
        assert_eq!(classify_settlement(500_000, 499_990, 69, 1), Mismatch);
        assert_eq!(classify_settlement(5, 6, 0, 0), Mismatch);
    }
}
