//! The receipt that ends every gate chat turn (SPEC section 5, Chat SSE).

use serde::{Deserialize, Serialize};

/// Why generation stopped.
///
/// vLLM's list is wider than OpenAI's, so the receipt carries vLLM's names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// The model ended its answer.
    Stop,
    /// The output limit was reached.
    Length,
    /// Generation was cut short: the client left or a deadline passed.
    Abort,
    /// The engine failed mid-turn.
    Error,
    /// The engine stopped a repetition loop.
    Repetition,
}

impl FinishReason {
    /// Maps an engine's `finish_reason` string; `None` for one we don't know.
    pub fn from_engine(reason: &str) -> Option<Self> {
        match reason {
            "stop" => Some(Self::Stop),
            "length" => Some(Self::Length),
            "abort" => Some(Self::Abort),
            "error" => Some(Self::Error),
            "repetition" => Some(Self::Repetition),
            _ => None,
        }
    }

    /// The wire name, also used in OpenAI-compatible chunks.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::Abort => "abort",
            Self::Error => "error",
            Self::Repetition => "repetition",
        }
    }
}

/// What one turn cost, sent as `event: receipt` after the last content chunk.
///
/// `charge` is `in_rate × prompt_tokens + out_rate × completion_tokens`,
/// capped at the turn's hold (docs 05, one debit rule), and `remaining` is
/// the session's budget left after it. Receipts are unsigned: they travel
/// inside attested TLS, and the credential's balance stays authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Turn number within the session, from 1.
    pub turn: u32,
    /// The model id from the release catalogue.
    pub model: String,
    /// Prompt tokens counted by the engine's tokenizer.
    pub prompt_tokens: u32,
    /// Output tokens generated.
    pub completion_tokens: u32,
    /// Credits debited for this turn.
    pub charge: u64,
    /// Credits left in the session after this turn.
    pub remaining: u64,
    /// Why generation stopped.
    pub finish: FinishReason,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_has_the_spec_fields() {
        let receipt = Receipt {
            turn: 1,
            model: "fake-small".into(),
            prompt_tokens: 9,
            completion_tokens: 20,
            charge: 69,
            remaining: 499_931,
            finish: FinishReason::Length,
        };
        let value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "turn": 1, "model": "fake-small", "prompt_tokens": 9, "completion_tokens": 20,
                "charge": 69, "remaining": 499_931, "finish": "length"
            })
        );
    }

    #[test]
    fn engine_reasons_map_and_unknown_ones_do_not() {
        assert_eq!(FinishReason::from_engine("length"), Some(FinishReason::Length));
        assert_eq!(FinishReason::from_engine("tool_calls"), None);
        for reason in [FinishReason::Stop, FinishReason::Repetition] {
            assert_eq!(FinishReason::from_engine(reason.as_str()), Some(reason));
        }
    }
}
