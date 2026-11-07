//! A started turn's output, as engine-neutral events.

use std::fmt;
use std::pin::Pin;

use futures::{Stream, StreamExt as _};
use proto::FinishReason;

use crate::BackendError;

/// One thing the engine reported during a turn.
#[derive(Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// New output text and the tokens it took.
    Delta {
        /// The text; model output, so `Debug` shows only its length.
        text: String,
        /// Output tokens this text accounts for.
        tokens: u32,
    },
    /// The engine's own final count.
    Usage {
        /// Prompt tokens.
        prompt: u32,
        /// Output tokens.
        completion: u32,
    },
    /// Why generation stopped.
    Finish(FinishReason),
}

impl fmt::Debug for EngineEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delta { text, tokens } => {
                f.debug_struct("Delta").field("text_bytes", &text.len()).field("tokens", tokens).finish()
            }
            Self::Usage { prompt, completion } => {
                f.debug_struct("Usage").field("prompt", prompt).field("completion", completion).finish()
            }
            Self::Finish(reason) => f.debug_tuple("Finish").field(reason).finish(),
        }
    }
}

type Events = Pin<Box<dyn Stream<Item = Result<EngineEvent, BackendError>> + Send>>;

/// A running turn. Dropping it cancels generation upstream.
///
/// It also keeps the turn's output count, whichever way the turn ends. The
/// engine's usage report is the count when it sent one; when it didn't, as
/// when it died or the stream was dropped first, the count is the tokens
/// seen in deltas (docs 05 section 5, finish reasons).
pub struct EngineStream {
    events: Events,
    counted: u32,
    reported: Option<u32>,
}

impl EngineStream {
    /// Wraps an adapter's event stream. The stream must end after its first error.
    pub fn new(events: impl Stream<Item = Result<EngineEvent, BackendError>> + Send + 'static) -> Self {
        Self { events: Box::pin(events), counted: 0, reported: None }
    }

    /// The next event; `None` once the engine has finished cleanly or after an error.
    pub async fn next(&mut self) -> Option<Result<EngineEvent, BackendError>> {
        let event = self.events.next().await;
        match &event {
            Some(Ok(EngineEvent::Delta { tokens, .. })) => self.counted = self.counted.saturating_add(*tokens),
            Some(Ok(EngineEvent::Usage { completion, .. })) => {
                if *completion != self.counted {
                    tracing::debug!(reported = completion, counted = self.counted, "engine usage differs from deltas");
                }
                self.reported = Some(*completion);
            }
            _ => {}
        }
        event
    }

    /// Output tokens so far: the engine's report if it sent one, else the tokens counted from deltas.
    pub fn completion_tokens(&self) -> u32 {
        self.reported.unwrap_or(self.counted)
    }

    /// Whether the engine sent its own usage count.
    pub fn usage_reported(&self) -> bool {
        self.reported.is_some()
    }
}

impl fmt::Debug for EngineStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineStream")
            .field("counted", &self.counted)
            .field("reported", &self.reported)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(clippy::unnecessary_wraps, reason = "a stream item, which is a Result")]
    fn delta(tokens: u32) -> Result<EngineEvent, BackendError> {
        Ok(EngineEvent::Delta { text: "w".into(), tokens })
    }

    async fn drain(mut stream: EngineStream) -> EngineStream {
        while stream.next().await.is_some() {}
        stream
    }

    #[tokio::test]
    async fn usage_wins_when_reported() {
        let events = vec![delta(1), delta(1), Ok(EngineEvent::Usage { prompt: 9, completion: 3 })];
        let stream = drain(EngineStream::new(futures::stream::iter(events))).await;
        assert!(stream.usage_reported());
        assert_eq!(stream.completion_tokens(), 3);
    }

    #[tokio::test]
    async fn counted_tokens_stand_in_for_missing_usage() {
        let events = vec![delta(1), delta(2), Ok(EngineEvent::Finish(FinishReason::Stop))];
        let stream = drain(EngineStream::new(futures::stream::iter(events))).await;
        assert!(!stream.usage_reported());
        assert_eq!(stream.completion_tokens(), 3);
    }

    #[test]
    fn debug_hides_output_text() {
        let event = EngineEvent::Delta { text: "secret answer".into(), tokens: 2 };
        assert!(!format!("{event:?}").contains("secret"));
    }
}
