//! A session's running account, and the receipt each turn ends with.

use proto::{FinishReason, Receipt};

use crate::turn::{ModelTerms, Refusal, TurnEnd, TurnPlan, plan_turn, turn_debit};

/// What one settled turn cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnCharge {
    /// Turn number within the session, from 1.
    pub turn: u32,
    /// Credits debited.
    pub charge: u64,
    /// Credits left in the session afterwards.
    pub remaining: u64,
}

impl TurnCharge {
    /// The receipt the gate sends after the turn's last content chunk.
    pub fn receipt(&self, model: &str, plan: &TurnPlan, completion_tokens: u32, finish: FinishReason) -> Receipt {
        Receipt {
            turn: self.turn,
            model: model.to_owned(),
            prompt_tokens: plan.prompt_tokens,
            completion_tokens,
            charge: self.charge,
            remaining: self.remaining,
            finish,
        }
    }
}

/// One session's budget `s`, the debits against it, and the turn count.
///
/// The sum of debits never exceeds `s` (docs 05 invariant 6), so the
/// refund `t = s − debited` is always well defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLedger {
    budget: u64,
    debited: u64,
    turns: u32,
}

impl SessionLedger {
    /// A fresh session with budget `s`.
    pub fn new(budget: u64) -> Self {
        Self { budget, debited: 0, turns: 0 }
    }

    /// `s`.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// The sum of debits so far.
    pub fn debited(&self) -> u64 {
        self.debited
    }

    /// `s − debited`.
    pub fn remaining(&self) -> u64 {
        self.budget - self.debited
    }

    /// Turns settled so far.
    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// Admits a turn against this session, or refuses it.
    pub fn plan(
        &self,
        terms: &ModelTerms,
        prompt_tokens: u32,
        requested_output: Option<u32>,
    ) -> Result<TurnPlan, Refusal> {
        plan_turn(terms, prompt_tokens, requested_output, self.budget, self.remaining())
    }

    /// Records a finished turn under the one debit rule.
    pub fn settle(&mut self, plan: &TurnPlan, terms: &ModelTerms, completion_tokens: u32, end: TurnEnd) -> TurnCharge {
        // The hold was within `remaining` when the turn was admitted, and only
        // one turn runs at a time; the `min` keeps the invariant regardless.
        let charge = turn_debit(plan, terms, completion_tokens, end).min(self.remaining());
        self.debited += charge;
        self.turns += 1;
        TurnCharge { turn: self.turns, charge, remaining: self.remaining() }
    }

    /// The refund amount `t` if the session closed now.
    pub fn refund_amount(&self) -> u64 {
        self.remaining()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_settles_exactly() {
        let small = ModelTerms::new(1, 3, 4096).unwrap();
        let mut ledger = SessionLedger::new(500_000);

        let plan = ledger.plan(&small, 9, Some(20)).unwrap();
        let first = ledger.settle(&plan, &small, 20, TurnEnd::Completed);
        assert_eq!(first, TurnCharge { turn: 1, charge: 69, remaining: 499_931 });
        let receipt = first.receipt("fake-small", &plan, 20, FinishReason::Length);
        assert_eq!((receipt.turn, receipt.charge, receipt.remaining), (1, 69, 499_931));
        assert_eq!((receipt.prompt_tokens, receipt.completion_tokens), (9, 20));

        let plan = ledger.plan(&small, 40, None).unwrap();
        let second = ledger.settle(&plan, &small, 17, TurnEnd::ClientGone);
        assert_eq!(second.charge, 40 + 51);
        assert_eq!(ledger.turns(), 2);
        assert_eq!(ledger.debited(), 69 + 91);
        assert_eq!(ledger.refund_amount(), 500_000 - 160);
    }

    #[test]
    fn debits_never_exceed_the_budget() {
        let terms = ModelTerms::new(2, 5, 1000).unwrap();
        let mut ledger = SessionLedger::new(5_000);
        let mut receipts = 0;
        while let Ok(plan) = ledger.plan(&terms, 100, None) {
            // Engines overrun their limit by a token or two; the hold absorbs it.
            receipts += ledger.settle(&plan, &terms, plan.allowed_output + 2, TurnEnd::Completed).charge;
            assert!(ledger.debited() <= ledger.budget());
        }
        assert_eq!(receipts, ledger.debited());
        assert_eq!(ledger.plan(&terms, 100, None), Err(Refusal::SessionExhausted));
        assert_eq!(ledger.refund_amount(), 5_000 - receipts);
    }
}
