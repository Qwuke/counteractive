//! The inference backend: the engine axis of ARCHITECTURE section 8.3.
//!
//! [`InferenceBackend`] is what the gate calls to count a prompt and to
//! stream a turn. [`OpenAiBackend`] is the one adapter: an OpenAI-compatible
//! engine with vLLM's `/tokenize` shape on a Unix socket, which covers vLLM,
//! llama.cpp's server and `fake-engine` (SPEC sections 5 and 8).
//!
//! This crate knows nothing about credits. It reports token counts, and the
//! gate's metering decides what they cost.

mod openai;
mod stream;
mod unix_http;

use std::fmt;
use std::num::NonZeroU32;

use async_trait::async_trait;
use proto::openai::ChatMessage;
use proto::sse::SseError;

pub use openai::OpenAiBackend;
pub use stream::{EngineEvent, EngineStream};

/// An inference engine, as the gate sees it.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Static facts. The gate compares these with the release's catalogue
    /// and refuses to serve a model rather than quietly downgrading it.
    fn describe(&self) -> BackendCapabilities;

    /// Prompt tokens for exactly the messages [`InferenceBackend::start`] would send.
    async fn count_prompt(&self, request: &EngineRequest) -> Result<u32, BackendError>;

    /// Starts generating at most `limit` tokens. Returns once the engine has
    /// accepted the request; dropping the stream cancels generation upstream.
    async fn start(&self, request: EngineRequest, limit: OutputLimit) -> Result<EngineStream, BackendError>;
}

/// What one turn asks the engine for: a model and the whole conversation.
///
/// `Debug` shows only the model and the number of messages, so a prompt
/// can't reach a log through it.
#[derive(Clone, PartialEq, Eq)]
pub struct EngineRequest {
    /// The model name the engine serves, from the release catalogue.
    pub model: String,
    /// The allowlisted messages, already flattened to text.
    pub messages: Vec<ChatMessage>,
}

impl fmt::Debug for EngineRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineRequest").field("model", &self.model).field("messages", &self.messages.len()).finish()
    }
}

/// The most output tokens a turn may generate; never zero, since the gate
/// refuses a zero-output turn before it reaches the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLimit(NonZeroU32);

impl OutputLimit {
    /// A limit of `tokens`, or `None` for zero.
    pub fn new(tokens: u32) -> Option<Self> {
        NonZeroU32::new(tokens).map(Self)
    }

    /// The limit.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Static facts about a backend (docs 03 section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Who counts the tokens that are charged.
    pub usage_source: UsageSource,
    /// How far those counts can be trusted.
    pub usage_trust: UsageTrust,
    /// What protects prompts between the wallet and the engine.
    pub byte_sealing: ByteSealing,
    /// Whether chunks carry token ids, so output can be counted exactly while streaming.
    pub token_ids: bool,
}

/// Who counts tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSource {
    /// The engine itself, from its tokenizer and generation loop.
    EngineCounted,
    /// A third-party provider's report.
    ProviderReported,
}

/// How far token counts can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTrust {
    /// The engine runs inside the attested boundary with the gate.
    InsideAttestedBoundary,
    /// The counts come from outside it, unsigned.
    Unsigned,
}

impl UsageTrust {
    /// The name a release catalogue's `usage_trust` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InsideAttestedBoundary => "inside_attested_boundary",
            Self::Unsigned => "unsigned",
        }
    }
}

/// What seals prompt and output bytes on the way to the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteSealing {
    /// The gate's inner TLS; the engine is local to the gate.
    InnerTls,
    /// Encrypted HTTP body protocol to a remote provider.
    Ehbp,
}

/// Why a backend call failed. Messages never include request or response bodies.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The engine's socket couldn't be reached.
    #[error("engine connect failed: {0}")]
    Connect(#[source] std::io::Error),
    /// HTTP failed on the engine connection.
    #[error("engine HTTP error: {0}")]
    Http(#[from] hyper::Error),
    /// The engine didn't answer in time.
    #[error("engine timed out")]
    Timeout,
    /// The engine answered with a non-success status.
    #[error("engine returned status {0}")]
    Status(u16),
    /// A response body was larger than this adapter accepts.
    #[error("engine response too large")]
    BodyTooLarge,
    /// A response wasn't the JSON shape expected.
    #[error("engine response malformed")]
    Decode,
    /// The engine's event stream couldn't be split.
    #[error("engine event stream malformed: {0}")]
    Stream(#[from] SseError),
    /// The event stream ended without `[DONE]`: the engine died mid-turn.
    #[error("engine stream ended early")]
    Truncated,
}
