//! What the fake engine says, decided from the request alone (SPEC section 8).
//!
//! Everything here is a pure function of the messages and the output limit,
//! so a test can compute the expected token counts and charges without
//! running the engine.

use std::time::Duration;

use proto::openai::{ChatMessage, ChatRole};

/// The context window `/tokenize` reports, and the output limit used when a
/// request names none.
pub const MAX_MODEL_LEN: u32 = 4096;

/// Template overhead charged per message, standing in for a chat template's role markers.
const TOKENS_PER_MESSAGE: u32 = 4;

/// The words the fake model cycles through. Their content is meaningless.
const VOCABULARY: [&str; 16] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima",
    "mike", "november", "oscar", "papa",
];

/// Whitespace-separated words in one text.
pub fn words(text: &str) -> u32 {
    u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX)
}

/// Words across every message's content.
pub fn prompt_words(messages: &[ChatMessage]) -> u32 {
    messages.iter().fold(0, |sum, message| sum.saturating_add(words(&message.content)))
}

/// What `/tokenize` reports: 4 per message plus the words of each content.
pub fn prompt_tokens(messages: &[ChatMessage]) -> u32 {
    messages
        .iter()
        .fold(0, |sum, message| sum.saturating_add(TOKENS_PER_MESSAGE).saturating_add(words(&message.content)))
}

/// Output tokens for a request: `min(limit, 16 + words(prompt) % 17)`, or
/// the whole limit when the `[[long]]` hook is present.
pub fn output_tokens(messages: &[ChatMessage], limit: u32, faults: &Faults) -> u32 {
    if faults.long { limit } else { limit.min(16 + prompt_words(messages) % 17) }
}

/// The text of output token `index`: a word, space-separated from the one before.
pub fn token_text(index: u32) -> String {
    let word = VOCABULARY[index as usize % VOCABULARY.len()];
    if index == 0 { word.to_owned() } else { format!(" {word}") }
}

/// Fault hooks, read from the last user message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Faults {
    /// `[[stall:<ms>]]`: pause this long before the first token.
    pub stall: Option<Duration>,
    /// `[[die-after:<n>]]`: drop the connection after `n` tokens, with no usage chunk.
    pub die_after: Option<u32>,
    /// `[[long]]`: produce the full output limit.
    pub long: bool,
}

impl Faults {
    /// The hooks in the last user message. Malformed hooks are ignored.
    pub fn from_messages(messages: &[ChatMessage]) -> Self {
        messages
            .iter()
            .rev()
            .find(|message| message.role == ChatRole::User)
            .map_or_else(Self::default, |message| Self::from_text(&message.content))
    }

    fn from_text(text: &str) -> Self {
        let mut faults = Self::default();
        let mut rest = text;
        while let Some((_, after)) = rest.split_once("[[") {
            let Some((hook, tail)) = after.split_once("]]") else { break };
            if hook == "long" {
                faults.long = true;
            } else if let Some(ms) = hook.strip_prefix("stall:").and_then(|ms| ms.parse().ok()) {
                faults.stall = Some(Duration::from_millis(ms));
            } else if let Some(n) = hook.strip_prefix("die-after:").and_then(|n| n.parse().ok()) {
                faults.die_after = Some(n);
            }
            rest = tail;
        }
        faults
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> ChatMessage {
        ChatMessage::new(ChatRole::User, text)
    }

    #[test]
    fn counts_follow_the_spec_formula() {
        let messages = [ChatMessage::new(ChatRole::System, "be brief"), user("hello there  friend")];
        assert_eq!(prompt_words(&messages), 5);
        assert_eq!(prompt_tokens(&messages), 4 + 2 + 4 + 3);
        assert_eq!(output_tokens(&messages, 4096, &Faults::default()), 16 + 5);
        assert_eq!(output_tokens(&messages, 7, &Faults::default()), 7);
        // 33 words: 33 % 17 = 16, the longest natural answer.
        let long_prompt = [user(&"w ".repeat(33))];
        assert_eq!(output_tokens(&long_prompt, 4096, &Faults::default()), 32);
    }

    #[test]
    fn hooks_come_from_the_last_user_message() {
        let messages =
            [user("[[long]]"), ChatMessage::new(ChatRole::Assistant, "ok"), user("x [[stall:250]] [[die-after:3]]")];
        let faults = Faults::from_messages(&messages);
        assert_eq!(faults, Faults { stall: Some(Duration::from_millis(250)), die_after: Some(3), long: false });
        assert!(Faults::from_messages(&[user("[[long]] please")]).long);
        assert_eq!(output_tokens(&[user("[[long]]")], 300, &Faults { long: true, ..Faults::default() }), 300);
    }

    #[test]
    fn malformed_hooks_are_ignored() {
        assert_eq!(
            Faults::from_messages(&[user("[[stall:abc]] [[die-after:]] [[unknown]] [[long")]),
            Faults::default()
        );
    }

    #[test]
    fn tokens_are_space_separated_words() {
        let text: String = (0..3).map(token_text).collect();
        assert_eq!(text, "alpha bravo charlie");
        assert_eq!(token_text(16), " alpha");
    }
}
