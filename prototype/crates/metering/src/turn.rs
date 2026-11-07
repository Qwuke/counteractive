//! One turn: the admission decision, the hold, and the one debit rule (docs 05 section 5).

use proto::{ErrorBody, ErrorCode};

use crate::MeteringError;

/// A model's prices and output cap, from the release catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelTerms {
    input_rate: u64,
    output_rate: u64,
    max_output: u32,
}

impl ModelTerms {
    /// Terms with credits per input and output token. The output rate and
    /// cap must be at least 1, or a hold couldn't bound a turn.
    pub fn new(input_rate: u64, output_rate: u64, max_output: u32) -> Result<Self, MeteringError> {
        if output_rate == 0 || max_output == 0 {
            return Err(MeteringError::InvalidTerms);
        }
        Ok(Self { input_rate, output_rate, max_output })
    }

    /// Credits per prompt token.
    pub fn input_rate(&self) -> u64 {
        self.input_rate
    }

    /// Credits per output token.
    pub fn output_rate(&self) -> u64 {
        self.output_rate
    }

    /// The largest output one turn may ask for.
    pub fn max_output(&self) -> u32 {
        self.max_output
    }

    /// `in_rate × prompt_tokens`. Saturates: a saturated value exceeds every
    /// budget, so it's refused like any other prompt that's too large.
    pub fn input_charge(&self, prompt_tokens: u32) -> u64 {
        self.input_rate.saturating_mul(u64::from(prompt_tokens))
    }

    /// `out_rate × tokens`, saturating like [`ModelTerms::input_charge`].
    pub fn output_charge(&self, tokens: u32) -> u64 {
        self.output_rate.saturating_mul(u64::from(tokens))
    }
}

/// An admitted turn: what to ask the engine for, and what's held for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnPlan {
    /// Prompt tokens from the engine's tokenizer.
    pub prompt_tokens: u32,
    /// `in_rate × prompt_tokens`.
    pub input_charge: u64,
    /// `max_completion_tokens` for the engine.
    pub allowed_output: u32,
    /// `H = input_charge + out_rate × allowed_output`; no debit exceeds it.
    pub hold: u64,
}

/// Why a turn wasn't admitted. Each maps to one error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The client asked for zero output tokens.
    ZeroOutput,
    /// Even a fresh session of this size can't cover the prompt plus one token.
    PromptExceedsBudget {
        /// `in_rate × prompt + out_rate`.
        required: u64,
    },
    /// This session can't, but a fresh one could.
    SessionExhausted,
}

impl Refusal {
    /// The response body for this refusal.
    pub fn error_body(&self) -> ErrorBody {
        match *self {
            Self::ZeroOutput => ErrorBody::new(ErrorCode::ZeroOutput),
            Self::PromptExceedsBudget { required } => ErrorBody::prompt_exceeds_budget(required),
            Self::SessionExhausted => ErrorBody::new(ErrorCode::SessionExhausted),
        }
    }
}

/// The admission decision, in the order of docs 05 section 5, one turn, step 6.
///
/// `budget` is the session's `s` and `remaining` what's left of it.
/// `requested_output` is the client's limit; absent means the model's cap.
pub fn plan_turn(
    terms: &ModelTerms,
    prompt_tokens: u32,
    requested_output: Option<u32>,
    budget: u64,
    remaining: u64,
) -> Result<TurnPlan, Refusal> {
    if requested_output == Some(0) {
        return Err(Refusal::ZeroOutput);
    }
    let input_charge = terms.input_charge(prompt_tokens);
    let required = input_charge.saturating_add(terms.output_rate);
    if required > budget {
        return Err(Refusal::PromptExceedsBudget { required });
    }
    if required > remaining {
        return Err(Refusal::SessionExhausted);
    }
    // remaining ≥ required > input_charge, so this can't underflow.
    let affordable = (remaining - input_charge) / terms.output_rate;
    let allowed_output = requested_output
        .unwrap_or(terms.max_output)
        .min(terms.max_output)
        .min(u32::try_from(affordable).unwrap_or(u32::MAX));
    let hold = input_charge + terms.output_charge(allowed_output);
    Ok(TurnPlan { prompt_tokens, input_charge, allowed_output, hold })
}

/// How a turn ended, once the engine had started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    /// The engine finished, by stop or by length.
    Completed,
    /// The client disconnected or stalled; the stream was dropped.
    ClientGone,
    /// The engine failed mid-turn, which a client can't cause.
    EngineFault,
}

/// The one debit rule: input plus counted output, capped at the hold.
///
/// It applies on every exit path once the engine has started, so dropping
/// the stream can't avoid the input charge. The only exception is an
/// engine fault, which charges counted output alone.
pub fn turn_debit(plan: &TurnPlan, terms: &ModelTerms, completion_tokens: u32, end: TurnEnd) -> u64 {
    let output = terms.output_charge(completion_tokens);
    let debit = match end {
        TurnEnd::Completed | TurnEnd::ClientGone => plan.input_charge.saturating_add(output),
        TurnEnd::EngineFault => output,
    };
    debit.min(plan.hold)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> ModelTerms {
        ModelTerms::new(1, 3, 4096).unwrap()
    }

    #[test]
    fn terms_need_a_positive_output_rate_and_cap() {
        assert_eq!(ModelTerms::new(1, 0, 10), Err(MeteringError::InvalidTerms));
        assert_eq!(ModelTerms::new(1, 1, 0), Err(MeteringError::InvalidTerms));
        ModelTerms::new(0, 1, 1).unwrap();
    }

    #[test]
    fn admits_and_holds() {
        let plan = plan_turn(&small(), 9, Some(20), 500_000, 500_000).unwrap();
        assert_eq!(plan, TurnPlan { prompt_tokens: 9, input_charge: 9, allowed_output: 20, hold: 69 });
    }

    #[test]
    fn decision_order_follows_the_docs() {
        // Zero output wins over everything, even an unaffordable prompt.
        assert_eq!(plan_turn(&small(), u32::MAX, Some(0), 10, 10), Err(Refusal::ZeroOutput));
        // required = 7 + 3 = 10 > s = 9: a fresh session wouldn't help.
        assert_eq!(plan_turn(&small(), 7, None, 9, 9), Err(Refusal::PromptExceedsBudget { required: 10 }));
        // required = 10 ≤ s, but > remaining: a fresh session would.
        assert_eq!(plan_turn(&small(), 7, None, 100, 9), Err(Refusal::SessionExhausted));
        // Exactly affordable: one output token.
        let plan = plan_turn(&small(), 7, None, 100, 10).unwrap();
        assert_eq!((plan.allowed_output, plan.hold), (1, 10));
    }

    #[test]
    fn allowed_output_is_the_least_of_three_limits() {
        let terms = ModelTerms::new(1, 3, 100).unwrap();
        // The client's limit.
        assert_eq!(plan_turn(&terms, 10, Some(5), 1000, 1000).unwrap().allowed_output, 5);
        // The model's cap, also when the client asks for more or nothing.
        assert_eq!(plan_turn(&terms, 10, Some(500), 1000, 1000).unwrap().allowed_output, 100);
        assert_eq!(plan_turn(&terms, 10, None, 1000, 1000).unwrap().allowed_output, 100);
        // What's affordable: floor((200 − 10) / 3) = 63.
        let plan = plan_turn(&terms, 10, None, 1000, 200).unwrap();
        assert_eq!((plan.allowed_output, plan.hold), (63, 199));
    }

    #[test]
    fn huge_prompts_saturate_into_a_refusal() {
        let terms = ModelTerms::new(u64::MAX, u64::MAX, 1).unwrap();
        assert_eq!(
            plan_turn(&terms, u32::MAX, None, 500_000, 500_000),
            Err(Refusal::PromptExceedsBudget { required: u64::MAX })
        );
    }

    #[test]
    fn one_debit_rule() {
        let terms = small();
        let plan = plan_turn(&terms, 9, Some(20), 500_000, 500_000).unwrap();
        assert_eq!(turn_debit(&plan, &terms, 20, TurnEnd::Completed), 69);
        // A disconnect before the first token still pays for input.
        assert_eq!(turn_debit(&plan, &terms, 0, TurnEnd::ClientGone), 9);
        // An engine fault charges output only.
        assert_eq!(turn_debit(&plan, &terms, 4, TurnEnd::EngineFault), 12);
        // An overrun past the hold is the operator's cost.
        assert_eq!(turn_debit(&plan, &terms, 22, TurnEnd::Completed), 69);
    }

    #[test]
    fn refusals_map_to_error_bodies() {
        assert_eq!(Refusal::ZeroOutput.error_body().http_status(), 400);
        let body = Refusal::PromptExceedsBudget { required: 42 }.error_body();
        assert_eq!((body.error, body.required), (ErrorCode::PromptExceedsBudget, Some(42)));
        assert_eq!(Refusal::SessionExhausted.error_body().http_status(), 402);
    }
}
