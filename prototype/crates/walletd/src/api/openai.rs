//! `/v1/models` and `/v1/chat/completions`: the OpenAI-compatible surface.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use proto::gate::SessionChatRequest;
use proto::openai::{
    ChatCompletion, ChatCompletionChunk, ChatCompletionRequest, ChatMessage, ChatRole, ChunkChoice, CompletionChoice,
    Delta, ModelList, Usage,
};
use proto::sse::{OPENAI_DONE, SseEvent};
use proto::walletd::{CONVERSATION_HEADER, RECOVERED_HEADER, ROLLOVER_HEADER, WithReceipt};
use proto::{ErrorBody, ErrorCode, Receipt};
use rand::TryRng as _;
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};
use wallet_core::manifest_store::usable_configs;

use super::{ApiJson, to_completion};
use crate::conversation::{Conversation, valid_id};
use crate::error::ApiError;
use crate::state::{Context, Walletd};
use crate::turn::{Notices, StartedTurn};

/// Encoded SSE frames buffered toward a slow client.
const STREAM_BUFFER: usize = 32;

/// `GET /v1/models`: the catalogue of the releases this wallet accepts.
pub async fn models(State(walletd): State<Arc<Walletd>>) -> Result<Json<ModelList>, ApiError> {
    let context = walletd.context()?;
    let ids = offered_models(&walletd, &context);
    Ok(Json(ModelList::new(ids.iter().map(String::as_str), "counteractive")))
}

/// `POST /v1/chat/completions`, streamed when the client asks.
///
/// Once the conversation is free the turn runs to its end, so a client
/// that hangs up can't strand the session or lose the receipt. Only a
/// stream stops early when its client leaves: the gate then debits what
/// was generated, and the turn counts as unknown.
pub async fn chat(
    State(walletd): State<Arc<Walletd>>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    let context = walletd.context()?;
    // A model no accepted release serves is refused before any spend.
    if !offered_models(&walletd, &context).contains(&request.model) {
        return Err(ApiError::gate(400, &ErrorBody::new(ErrorCode::Model)));
    }
    let named = match headers.get(CONVERSATION_HEADER) {
        Some(value) => match value.to_str() {
            Ok(id) if valid_id(id) => Some(id.to_owned()),
            _ => return Err(ApiError::local(StatusCode::BAD_REQUEST, "bad_request", "invalid conversation id")),
        },
        None => None,
    };
    let conversation = match &named {
        Some(id) => walletd.conversations.get_or_create(id),
        None => Arc::new(Mutex::new(Conversation::new())),
    };
    // Waiting for the conversation's previous turn is all a client that
    // leaves may cut short.
    let conversation = conversation.lock_owned().await;
    to_completion(run_turn(walletd, context, conversation, request, named.is_none())).await
}

/// Starts the turn, then relays it: streamed from a task of its own, or
/// collected into one answer.
async fn run_turn(
    walletd: Arc<Walletd>,
    context: Arc<Context>,
    mut conversation: OwnedMutexGuard<Conversation>,
    request: ChatCompletionRequest,
    one_off: bool,
) -> Result<Response, ApiError> {
    let gate_request = SessionChatRequest {
        model: request.model.clone(),
        messages: request.messages.clone(),
        max_completion_tokens: request.output_limit(),
    };
    let turn = match walletd.start_turn(&context, &mut conversation, &gate_request).await {
        Ok(turn) => turn,
        Err(error) => {
            if one_off {
                end_one_off(&walletd, &context, &mut conversation).await;
            }
            return Err(error);
        }
    };
    let notice_headers = notice_headers(&turn.notices);
    let shape = ChunkShape::new(request.model.clone());
    let turn = Relay { walletd, context, conversation, turn, one_off, shape };
    if request.wants_stream() {
        let (frames, body) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(turn.stream(frames));
        let body = Body::from_stream(futures::stream::unfold(body, |mut body| async move {
            Some((Ok::<_, std::convert::Infallible>(body.recv().await?), body))
        }));
        let sse = [(CONTENT_TYPE, "text/event-stream"), (CACHE_CONTROL, "no-cache")];
        Ok((notice_headers, sse, body).into_response())
    } else {
        Ok((notice_headers, Json(turn.collect().await)).into_response())
    }
}

/// A started turn and everything needed to finish it.
struct Relay {
    walletd: Arc<Walletd>,
    context: Arc<Context>,
    conversation: OwnedMutexGuard<Conversation>,
    turn: StartedTurn,
    one_off: bool,
    shape: ChunkShape,
}

impl Relay {
    /// Streams the turn as OpenAI chunks, ending with one carrying the
    /// usage and receipt, then `[DONE]`. A client that leaves stops the
    /// turn: the gate debits what was generated, and with no receipt the
    /// turn is counted as unknown.
    async fn stream(mut self, frames: mpsc::Sender<Bytes>) {
        let mut client_gone = false;
        while let Some(text) = self.turn.next_content(&self.walletd).await {
            let chunk = self.shape.chunk(Delta { role: Some(ChatRole::Assistant), content: Some(text) }, None);
            if frames.send(frame(&chunk)).await.is_err() {
                client_gone = true;
                break;
            }
        }
        let receipt = self.finish().await;
        if !client_gone {
            let last = WithReceipt { body: self.shape.last_chunk(receipt.as_ref()), receipt };
            let _ = frames.send(frame(&last)).await;
            let _ = frames.send(Bytes::from(SseEvent::data(OPENAI_DONE).encode())).await;
        }
    }

    /// Reads the whole turn into one completion with its receipt.
    async fn collect(mut self) -> WithReceipt<ChatCompletion> {
        let mut text = String::new();
        while let Some(piece) = self.turn.next_content(&self.walletd).await {
            text.push_str(&piece);
        }
        let receipt = self.finish().await;
        let body = ChatCompletion {
            id: self.shape.id.clone(),
            object: "chat.completion".into(),
            created: self.shape.created,
            model: self.shape.model.clone(),
            choices: vec![CompletionChoice {
                index: 0,
                message: ChatMessage::new(ChatRole::Assistant, text),
                finish_reason: Some(finish_reason(receipt.as_ref()).into()),
            }],
            usage: receipt.as_ref().map(usage),
        };
        WithReceipt { body, receipt }
    }

    /// Ends the turn and, for a request without a conversation id, its session.
    async fn finish(&mut self) -> Option<Receipt> {
        let receipt = self.turn.finish(&self.walletd, &mut self.conversation);
        if self.one_off {
            end_one_off(&self.walletd, &self.context, &mut self.conversation).await;
        }
        receipt
    }
}

/// Closes the session of a request that named no conversation, so nothing
/// links it to a later one. A failure leaves the chain for reconcile.
async fn end_one_off(walletd: &Walletd, context: &Context, conversation: &mut Conversation) {
    if let Err(error) = walletd.close_conversation(context, conversation).await {
        tracing::warn!(code = error.code(), "closing a one-off session failed");
    }
}

/// The models of every release this wallet may use, sorted.
fn offered_models(walletd: &Walletd, context: &Context) -> BTreeSet<String> {
    usable_configs(&context.manifest, walletd.pins.allow_dev_attestation)
        .into_iter()
        .flat_map(|(_, config)| config.catalogue.into_iter().map(|entry| entry.id))
        .collect()
}

/// Headers telling the client its session rolled over or was recovered.
fn notice_headers(notices: &Notices) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(reason) = notices.rollover.as_deref().and_then(|reason| HeaderValue::from_str(reason).ok()) {
        headers.insert(ROLLOVER_HEADER, reason);
    }
    if let Some(outcome) = notices.recovered {
        headers.insert(RECOVERED_HEADER, HeaderValue::from_static(outcome.as_str()));
    }
    headers
}

/// The id, time and model every chunk of one answer shares.
struct ChunkShape {
    id: String,
    created: u64,
    model: String,
}

impl ChunkShape {
    fn new(model: String) -> Self {
        let mut bytes = [0u8; 12];
        // The id only names the answer to its client; zeros are harmless.
        let _ = rand::rngs::SysRng.try_fill_bytes(&mut bytes);
        let created = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs());
        Self { id: format!("chatcmpl-{}", hex::encode(bytes)), created, model }
    }

    fn chunk(&self, delta: Delta, finish_reason: Option<String>) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: Some(self.id.clone()),
            object: Some("chat.completion.chunk".into()),
            created: Some(self.created),
            model: Some(self.model.clone()),
            choices: vec![ChunkChoice { index: 0, delta, finish_reason }],
            usage: None,
        }
    }

    fn last_chunk(&self, receipt: Option<&Receipt>) -> ChatCompletionChunk {
        let mut chunk = self.chunk(Delta::default(), Some(finish_reason(receipt).into()));
        chunk.usage = receipt.map(usage);
        chunk
    }
}

/// The receipt's finish reason; a turn without one ended in an error.
fn finish_reason(receipt: Option<&Receipt>) -> &'static str {
    receipt.map_or("error", |receipt| receipt.finish.as_str())
}

fn usage(receipt: &Receipt) -> Usage {
    Usage {
        prompt_tokens: receipt.prompt_tokens,
        completion_tokens: receipt.completion_tokens,
        total_tokens: Some(receipt.prompt_tokens.saturating_add(receipt.completion_tokens)),
    }
}

fn frame<T: serde::Serialize>(body: &T) -> Bytes {
    let data = serde_json::to_string(body).unwrap_or_default();
    Bytes::from(SseEvent::data(data).encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_chunk_carries_usage_and_receipt() {
        let receipt = Receipt {
            turn: 1,
            model: "fake-small".into(),
            prompt_tokens: 9,
            completion_tokens: 21,
            charge: 72,
            remaining: 499_928,
            finish: proto::FinishReason::Stop,
        };
        let shape = ChunkShape::new("fake-small".into());
        let last = WithReceipt { body: shape.last_chunk(Some(&receipt)), receipt: Some(receipt.clone()) };
        let value = serde_json::to_value(&last).unwrap();
        assert_eq!(value["choices"][0]["finish_reason"], "stop");
        assert_eq!(value["usage"]["total_tokens"], 30);
        assert_eq!(value["receipt"]["charge"], 72);
        assert_eq!(value["object"], "chat.completion.chunk");
    }
}
