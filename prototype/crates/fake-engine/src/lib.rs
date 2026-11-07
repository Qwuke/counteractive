//! A deterministic OpenAI-compatible engine on a Unix socket (SPEC section 8),
//! as a library so the backend's and the gate's tests can run it in-process.
//!
//! It speaks the subset the gate's `openai` backend adapter uses, in vLLM's
//! shapes: `GET /health`, `POST /tokenize` and a streaming
//! `POST /v1/chat/completions` that ends with a usage chunk and
//! `data: [DONE]`. Output depends only on the request ([`script`]), and fault
//! hooks in the last user message stall, cut off or lengthen a stream.
//!
//! [`EngineStats`] counts how streams ended, so a test can see that a client
//! dropping its stream really reached the engine as a cancellation.

pub mod script;

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use proto::openai::{
    ChatCompletionChunk, ChatMessage, ChatRole, ChunkChoice, Delta, StreamOptions, TokenizeRequest, TokenizeResponse,
    Usage,
};
use proto::sse::{OPENAI_DONE, SseEvent};
use serde::Deserialize;
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;

use crate::script::{Faults, MAX_MODEL_LEN};

/// How long a dying stream waits after its last token before dropping the connection.
const DEATH_PAUSE: Duration = Duration::from_millis(50);

/// How the engine's streams have ended so far.
#[derive(Debug, Default)]
pub struct EngineStats {
    completed: AtomicU64,
    cancelled: AtomicU64,
    died: AtomicU64,
}

impl EngineStats {
    /// Streams that sent everything, through `[DONE]`.
    pub fn completed(&self) -> u64 {
        self.completed.load(Ordering::SeqCst)
    }

    /// Streams the client dropped before the end.
    pub fn cancelled(&self) -> u64 {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Streams cut off on purpose by `[[die-after:<n>]]`.
    pub fn died(&self) -> u64 {
        self.died.load(Ordering::SeqCst)
    }
}

/// The engine's routes, sharing `stats`, pausing `token_delay` before
/// each output token (zero in tests, so streams arrive at once).
pub fn router(stats: Arc<EngineStats>, token_delay: Duration) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/tokenize", post(tokenize))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(Engine { stats, token_delay })
}

/// What every request shares.
#[derive(Clone)]
struct Engine {
    stats: Arc<EngineStats>,
    token_delay: Duration,
}

/// Serves the engine on `listener` until `shutdown` fires.
pub async fn serve(
    listener: UnixListener,
    stats: Arc<EngineStats>,
    token_delay: Duration,
    shutdown: CancellationToken,
) -> io::Result<()> {
    axum::serve(listener, router(stats, token_delay)).with_graceful_shutdown(shutdown.cancelled_owned()).await
}

/// A fake engine running on this process's runtime, for in-process tests.
/// It stops when dropped.
#[derive(Debug)]
pub struct RunningEngine {
    stats: Arc<EngineStats>,
    shutdown: CancellationToken,
}

impl RunningEngine {
    /// How its streams have ended so far.
    pub fn stats(&self) -> &EngineStats {
        &self.stats
    }
}

impl Drop for RunningEngine {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// Starts serving on `listener` in a background task. Must be called within a Tokio runtime.
pub fn spawn(listener: UnixListener) -> RunningEngine {
    let (stats, shutdown) = (Arc::new(EngineStats::default()), CancellationToken::new());
    let task = serve(listener, Arc::clone(&stats), Duration::ZERO, shutdown.clone());
    tokio::spawn(async move {
        if let Err(error) = task.await {
            tracing::warn!(%error, "fake engine stopped with an error");
        }
    });
    RunningEngine { stats, shutdown }
}

async fn tokenize(body: Bytes) -> Response {
    let Ok(request) = serde_json::from_slice::<TokenizeRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let count = script::prompt_tokens(&request.messages);
    axum::Json(TokenizeResponse { count, max_model_len: Some(MAX_MODEL_LEN) }).into_response()
}

/// The fields of a chat request the fake reads; others are ignored, as vLLM ignores them.
#[derive(Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: bool,
    stream_options: Option<StreamOptions>,
    max_completion_tokens: Option<u32>,
}

async fn chat_completions(State(engine): State<Engine>, body: Bytes) -> Response {
    let Ok(request) = serde_json::from_slice::<ChatRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // The gate only ever streams, so the fake doesn't implement the other mode.
    if !request.stream {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let guard = StreamGuard { stats: engine.stats, ended: false };
    let body = futures::stream::unfold((plan_stream(&request, engine.token_delay), guard), next_step);
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// One thing the response stream does next.
enum Step {
    Send(Bytes),
    Pause(Duration),
    /// Ends the body with an error, so the server drops the connection.
    Die,
}

/// The whole response, decided up front from the request.
fn plan_stream(request: &ChatRequest, token_delay: Duration) -> VecDeque<Step> {
    let faults = Faults::from_messages(&request.messages);
    let limit = request.max_completion_tokens.unwrap_or(MAX_MODEL_LEN);
    let total = script::output_tokens(&request.messages, limit, &faults);
    let finish = if total == limit { "length" } else { "stop" };
    let chunk = |delta: Delta, finish_reason: Option<&str>| ChatCompletionChunk {
        id: Some("cmpl-fake".into()),
        object: Some("chat.completion.chunk".into()),
        created: Some(0),
        model: Some(request.model.clone()),
        choices: vec![ChunkChoice { index: 0, delta, finish_reason: finish_reason.map(str::to_owned) }],
        usage: None,
    };

    let mut steps = VecDeque::new();
    // vLLM opens with a role-only chunk that carries no token.
    steps.push_back(Step::Send(frame(&chunk(
        Delta { role: Some(ChatRole::Assistant), content: Some(String::new()) },
        None,
    ))));
    if let Some(stall) = faults.stall {
        steps.push_back(Step::Pause(stall));
    }
    for index in 0..total {
        if faults.die_after == Some(index) {
            // Pausing first lets the server flush what was sent, so the
            // client sees the tokens and then a cut, as from a real crash.
            steps.push_back(Step::Pause(DEATH_PAUSE));
            steps.push_back(Step::Die);
            return steps;
        }
        if !token_delay.is_zero() {
            steps.push_back(Step::Pause(token_delay));
        }
        let last = index + 1 == total;
        let delta = Delta { role: None, content: Some(script::token_text(index)) };
        steps.push_back(Step::Send(frame(&chunk(delta, last.then_some(finish)))));
    }
    if total == 0 {
        steps.push_back(Step::Send(frame(&chunk(Delta::default(), Some(finish)))));
    }
    if request.stream_options.is_some_and(|options| options.include_usage) {
        let prompt_tokens = script::prompt_tokens(&request.messages);
        let usage = Usage { prompt_tokens, completion_tokens: total, total_tokens: Some(prompt_tokens + total) };
        let usage_chunk = ChatCompletionChunk { choices: vec![], usage: Some(usage), ..chunk(Delta::default(), None) };
        steps.push_back(Step::Send(frame(&usage_chunk)));
    }
    steps.push_back(Step::Send(Bytes::from(SseEvent::data(OPENAI_DONE).encode())));
    steps
}

fn frame(chunk: &ChatCompletionChunk) -> Bytes {
    let json = serde_json::to_string(chunk).unwrap_or_default();
    Bytes::from(SseEvent::data(json).encode())
}

/// Counts how a stream ended. Dropped before the end means the client left.
struct StreamGuard {
    stats: Arc<EngineStats>,
    ended: bool,
}

impl StreamGuard {
    fn end(&mut self, counter: fn(&EngineStats) -> &AtomicU64) {
        self.ended = true;
        counter(&self.stats).fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        if !self.ended {
            self.stats.cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }
}

async fn next_step(
    (mut steps, mut guard): (VecDeque<Step>, StreamGuard),
) -> Option<(Result<Bytes, io::Error>, (VecDeque<Step>, StreamGuard))> {
    loop {
        match steps.pop_front() {
            Some(Step::Send(bytes)) => return Some((Ok(bytes), (steps, guard))),
            Some(Step::Pause(duration)) => tokio::time::sleep(duration).await,
            Some(Step::Die) => {
                guard.end(|stats| &stats.died);
                return Some((Err(io::Error::other("die-after fault hook")), (steps, guard)));
            }
            None => {
                if !guard.ended {
                    guard.end(|stats| &stats.completed);
                }
                return None;
            }
        }
    }
}
