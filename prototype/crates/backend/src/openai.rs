//! The `openai` adapter: an OpenAI-compatible engine with vLLM's `/tokenize`
//! shape, on a Unix socket (SPEC sections 5 and 8).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use http_body_util::BodyExt as _;
use hyper::body::Incoming;
use proto::FinishReason;
use proto::openai::{ChatCompletionChunk, EngineChatRequest, TokenizeRequest, TokenizeResponse};
use proto::sse::{OPENAI_DONE, SseEvent, SseSplitter};

use crate::stream::{EngineEvent, EngineStream};
use crate::unix_http::{ConnectionGuard, Exchange, UnixHttp};
use crate::{
    BackendCapabilities, BackendError, ByteSealing, EngineRequest, InferenceBackend, OutputLimit, UsageSource,
    UsageTrust,
};

/// How long the engine may take to accept a connection and send response
/// headers. Generation itself is bounded by the gate's turn deadlines.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest `/tokenize` answer read. vLLM includes the token ids, about
/// 6 bytes each, so this covers a long context with room to spare.
const MAX_TOKENIZE_BYTES: usize = 4 << 20;

/// Largest single event in the engine's stream. A chunk carries a token or
/// a few, so anything near this is malformed.
const MAX_ENGINE_EVENT_BYTES: usize = 64 << 10;

/// An OpenAI-compatible engine on a Unix socket: vLLM, llama.cpp's server or `fake-engine`.
#[derive(Debug, Clone)]
pub struct OpenAiBackend {
    http: UnixHttp,
}

impl OpenAiBackend {
    /// An adapter for the engine listening on `socket`.
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self { http: UnixHttp::new(socket.into(), RESPONSE_TIMEOUT) }
    }
}

#[async_trait]
impl InferenceBackend for OpenAiBackend {
    /// The engine is local, inside the attested boundary, and counts its
    /// own tokens. SPEC section 5's upstream body doesn't ask for token
    /// ids, so output is counted per chunk until the usage chunk arrives.
    fn describe(&self) -> BackendCapabilities {
        BackendCapabilities {
            usage_source: UsageSource::EngineCounted,
            usage_trust: UsageTrust::InsideAttestedBoundary,
            byte_sealing: ByteSealing::InnerTls,
            token_ids: false,
        }
    }

    async fn count_prompt(&self, request: &EngineRequest) -> Result<u32, BackendError> {
        let body = TokenizeRequest { model: request.model.clone(), messages: request.messages.clone() };
        let answer: TokenizeResponse = self.http.post_for_json("/tokenize", &body, MAX_TOKENIZE_BYTES).await?;
        Ok(answer.count)
    }

    async fn start(&self, request: EngineRequest, limit: OutputLimit) -> Result<EngineStream, BackendError> {
        let body = EngineChatRequest::new(request.model, request.messages, limit.get());
        let Exchange { response, connection } = self.http.post_json("/v1/chat/completions", &body).await?;
        let state = ChunkReader { body: response.into_body(), decoder: ChunkDecoder::new(), _connection: connection };
        Ok(EngineStream::new(futures::stream::unfold(Some(state), read_next)))
    }
}

/// The response body and its decoder. Owning the connection guard means
/// dropping the stream closes the engine socket.
struct ChunkReader {
    body: Incoming,
    decoder: ChunkDecoder,
    _connection: ConnectionGuard,
}

/// Yields the next event; after an error or the end, the state is gone and the stream ends.
async fn read_next(state: Option<ChunkReader>) -> Option<(Result<EngineEvent, BackendError>, Option<ChunkReader>)> {
    let mut reader = state?;
    loop {
        if let Some(event) = reader.decoder.ready.pop_front() {
            return Some((Ok(event), Some(reader)));
        }
        let failure = match reader.body.frame().await {
            Some(Ok(frame)) => match frame.into_data() {
                Ok(data) => match reader.decoder.push(&data) {
                    Ok(()) => continue,
                    Err(error) => error,
                },
                Err(_) => continue,
            },
            Some(Err(error)) => BackendError::Http(error),
            None => match reader.decoder.finish() {
                Ok(()) => return None,
                Err(error) => error,
            },
        };
        return Some((Err(failure), None));
    }
}

/// Turns the engine's SSE bytes into [`EngineEvent`]s. Pure, so it's tested on canned bytes.
struct ChunkDecoder {
    splitter: SseSplitter,
    ready: VecDeque<EngineEvent>,
    done: bool,
}

impl ChunkDecoder {
    fn new() -> Self {
        Self { splitter: SseSplitter::new(MAX_ENGINE_EVENT_BYTES), ready: VecDeque::new(), done: false }
    }

    /// Feeds body bytes, queueing the events they complete.
    fn push(&mut self, bytes: &[u8]) -> Result<(), BackendError> {
        for event in self.splitter.push(bytes)? {
            self.take(&event)?;
        }
        Ok(())
    }

    /// The body ended. Without `[DONE]` the engine died mid-turn.
    fn finish(&mut self) -> Result<(), BackendError> {
        let splitter = std::mem::replace(&mut self.splitter, SseSplitter::new(0));
        splitter.finish()?;
        if self.done { Ok(()) } else { Err(BackendError::Truncated) }
    }

    fn take(&mut self, event: &SseEvent) -> Result<(), BackendError> {
        if self.done {
            return Ok(());
        }
        if event.data == OPENAI_DONE {
            self.done = true;
            return Ok(());
        }
        let chunk: ChatCompletionChunk = serde_json::from_str(&event.data).map_err(|_| BackendError::Decode)?;
        // vLLM's first chunk has an empty content and no token; skip it.
        if let Some(text) = chunk.text().filter(|text| !text.is_empty()) {
            self.ready.push_back(EngineEvent::Delta { text: text.to_owned(), tokens: 1 });
        }
        if let Some(reason) = chunk.finish_reason() {
            let finish = FinishReason::from_engine(reason).unwrap_or_else(|| {
                tracing::debug!(reason, "unknown engine finish reason; reporting stop");
                FinishReason::Stop
            });
            self.ready.push_back(EngineEvent::Finish(finish));
        }
        if let Some(usage) = chunk.usage {
            self.ready
                .push_back(EngineEvent::Usage { prompt: usage.prompt_tokens, completion: usage.completion_tokens });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(input: &str) -> (Vec<EngineEvent>, Result<(), BackendError>) {
        let mut decoder = ChunkDecoder::new();
        let pushed = decoder.push(input.as_bytes());
        let result = pushed.and_then(|()| decoder.finish());
        (decoder.ready.into_iter().collect(), result)
    }

    const ROLE: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n";

    #[test]
    fn decodes_a_vllm_stream() {
        let input = format!(
            "{ROLE}data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"Hi\"}},\"token_ids\":[9]}}]}}\n\n\
             data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\" there\"}},\"finish_reason\":\"length\"}}]}}\n\n\
             data: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":2}}}}\n\ndata: [DONE]\n\n"
        );
        let (events, result) = decode(&input);
        result.unwrap();
        assert_eq!(
            events,
            vec![
                EngineEvent::Delta { text: "Hi".into(), tokens: 1 },
                EngineEvent::Delta { text: " there".into(), tokens: 1 },
                EngineEvent::Finish(FinishReason::Length),
                EngineEvent::Usage { prompt: 7, completion: 2 },
            ]
        );
    }

    #[test]
    fn a_stream_without_usage_still_ends_cleanly() {
        let input = format!(
            "{ROLE}data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"x\"}},\"finish_reason\":\"stop\"}}]}}\n\n\
             data: [DONE]\n\n"
        );
        let (events, result) = decode(&input);
        result.unwrap();
        assert_eq!(events.len(), 2);
        assert!(!events.iter().any(|event| matches!(event, EngineEvent::Usage { .. })));
    }

    #[test]
    fn a_stream_cut_before_done_is_truncated() {
        let input = format!("{ROLE}data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"x\"}}}}]}}\n\n");
        let (events, result) = decode(&input);
        assert_eq!(events.len(), 1);
        assert!(matches!(result, Err(BackendError::Truncated)));
    }

    #[test]
    fn malformed_chunks_and_unknown_reasons() {
        assert!(matches!(decode("data: {not json\n\n").1, Err(BackendError::Decode)));
        let input =
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n";
        let (events, result) = decode(input);
        result.unwrap();
        assert_eq!(events, vec![EngineEvent::Finish(FinishReason::Stop)]);
    }
}
