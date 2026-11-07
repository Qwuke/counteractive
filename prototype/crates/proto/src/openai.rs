//! The OpenAI chat subset, with allowlisted fields.
//!
//! vLLM accepts unknown fields without complaint, so these structs are the
//! only filter between a client and the engine (docs 05, one turn, step 2).
//! Deserialising drops every field not named here, and message content is
//! flattened to plain text: a part that isn't text fails to parse, which a
//! service answers with `400`.
//!
//! The same chunk type serves three directions: engine to gate (read),
//! gate to wallet (content only, per SPEC section 5) and walletd to an
//! OpenAI client (with id, model and finish reason).

use serde::{Deserialize, Deserializer, Serialize};

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    /// Instructions. OpenAI's newer `developer` role is read as this.
    #[serde(alias = "developer")]
    System,
    /// The user.
    User,
    /// The model.
    Assistant,
}

/// One message, with its content flattened to text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Who wrote it.
    pub role: ChatRole,
    /// The text. Read from a string or from an array of text parts.
    #[serde(deserialize_with = "flatten_content")]
    pub content: String,
}

impl ChatMessage {
    /// A message from `role` with `content`.
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

/// Text parts are joined with a newline, as vLLM joins them.
fn flatten_content<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Content {
        Text(String),
        Parts(Vec<Part>),
    }

    #[derive(Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum Part {
        Text { text: String },
    }

    Ok(match Content::deserialize(deserializer)? {
        Content::Text(text) => text,
        Content::Parts(parts) => parts.into_iter().map(|Part::Text { text }| text).collect::<Vec<_>>().join("\n"),
    })
}

/// `POST /v1/chat/completions` on walletd, from any OpenAI client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model id from the release catalogue.
    pub model: String,
    /// The conversation so far.
    pub messages: Vec<ChatMessage>,
    /// Output limit (current OpenAI name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    /// Output limit (legacy OpenAI name), used when the current one is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Whether the client wants SSE chunks. OpenAI's default is `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl ChatCompletionRequest {
    /// The output limit the client asked for, by either name.
    pub fn output_limit(&self) -> Option<u32> {
        self.max_completion_tokens.or(self.max_tokens)
    }

    /// Whether to answer with SSE.
    pub fn wants_stream(&self) -> bool {
        self.stream.unwrap_or(false)
    }
}

/// `stream_options` for the engine: always ask for a final usage chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOptions {
    /// Ask for a final chunk carrying `usage`.
    pub include_usage: bool,
}

/// The body the gate sends upstream to the engine (SPEC section 5, engine body).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineChatRequest {
    /// Model name the engine serves.
    pub model: String,
    /// The conversation.
    pub messages: Vec<ChatMessage>,
    /// Always `true`.
    pub stream: bool,
    /// Always `{include_usage: true}`.
    pub stream_options: StreamOptions,
    /// The turn's `allowed_output`, never more.
    pub max_completion_tokens: u32,
}

impl EngineChatRequest {
    /// A streaming request with usage, capped at `allowed_output`.
    pub fn new(model: String, messages: Vec<ChatMessage>, allowed_output: u32) -> Self {
        Self {
            model,
            messages,
            stream: true,
            stream_options: StreamOptions { include_usage: true },
            max_completion_tokens: allowed_output,
        }
    }
}

/// `POST /tokenize` on the engine: count prompt tokens for the same messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenizeRequest {
    /// Model name the engine serves.
    pub model: String,
    /// Exactly the messages the chat request will send.
    pub messages: Vec<ChatMessage>,
}

/// The engine's `/tokenize` answer. vLLM also returns token ids, which are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenizeResponse {
    /// Prompt tokens after the chat template.
    pub count: u32,
    /// The engine's context window, when it reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_model_len: Option<u32>,
}

/// Token counts reported by the engine, or by walletd to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens.
    pub prompt_tokens: u32,
    /// Output tokens.
    pub completion_tokens: u32,
    /// Their sum, when the sender includes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
}

/// The incremental part of a streamed choice.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Delta {
    /// Set on an engine's first chunk only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ChatRole>,
    /// New text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// One streamed choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkChoice {
    /// Always 0 here: one choice per request.
    pub index: u32,
    /// The new text.
    #[serde(default)]
    pub delta: Delta,
    /// Set on the last content chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// One SSE `data:` payload of a streamed chat.
///
/// Optional fields are omitted when unset, so the gate's content chunk is
/// exactly `{"choices":[{"index":0,"delta":{"content":"..."}}]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// Completion id, for OpenAI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `"chat.completion.chunk"`, for OpenAI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Unix seconds, for OpenAI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    /// Model id, for OpenAI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The choices; empty on the engine's final usage chunk.
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    /// Final token counts, on the engine's last chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl ChatCompletionChunk {
    /// The gate's content chunk: one choice with only new text.
    pub fn content(text: impl Into<String>) -> Self {
        Self {
            id: None,
            object: None,
            created: None,
            model: None,
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: None, content: Some(text.into()) },
                finish_reason: None,
            }],
            usage: None,
        }
    }

    /// The text this chunk adds, if any.
    pub fn text(&self) -> Option<&str> {
        self.choices.first().and_then(|choice| choice.delta.content.as_deref())
    }

    /// The finish reason this chunk carries, if any.
    pub fn finish_reason(&self) -> Option<&str> {
        self.choices.first().and_then(|choice| choice.finish_reason.as_deref())
    }
}

/// A full message in a non-streamed answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionChoice {
    /// Always 0 here.
    pub index: u32,
    /// The answer.
    pub message: ChatMessage,
    /// Why generation stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// walletd's answer to a non-streamed chat request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompletion {
    /// Completion id.
    pub id: String,
    /// `"chat.completion"`.
    pub object: String,
    /// Unix seconds.
    pub created: u64,
    /// Model id.
    pub model: String,
    /// One choice.
    pub choices: Vec<CompletionChoice>,
    /// Token counts from the receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// One entry of `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model id from the release catalogue.
    pub id: String,
    /// `"model"`.
    pub object: String,
    /// Who serves it.
    pub owned_by: String,
}

/// `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelList {
    /// `"list"`.
    pub object: String,
    /// The models.
    pub data: Vec<ModelInfo>,
}

impl ModelList {
    /// A list of model ids, all owned by `owner`.
    pub fn new<'a>(ids: impl IntoIterator<Item = &'a str>, owner: &str) -> Self {
        Self {
            object: "list".into(),
            data: ids
                .into_iter()
                .map(|id| ModelInfo { id: id.into(), object: "model".into(), owned_by: owner.into() })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_fields_vanish() {
        let request: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "fake-small",
            "messages": [{"role": "user", "content": "hi", "name": "eve"}],
            "max_tokens": 5,
            "logprobs": true,
            "tools": [{"type": "function"}],
            "return_token_ids": true
        }))
        .unwrap();
        assert_eq!(request.output_limit(), Some(5));
        assert!(!request.wants_stream());
        let echoed = serde_json::to_value(&request).unwrap();
        assert_eq!(
            echoed,
            json!({"model": "fake-small", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 5})
        );
    }

    #[test]
    fn text_parts_flatten_and_other_parts_fail() {
        let message: ChatMessage = serde_json::from_value(json!({
            "role": "user",
            "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]
        }))
        .unwrap();
        assert_eq!(message.content, "a\nb");

        let image = serde_json::from_value::<ChatMessage>(json!({
            "role": "user",
            "content": [{"type": "image_url", "image_url": {"url": "https://example.com/x.png"}}]
        }));
        image.unwrap_err();
    }

    #[test]
    fn roles_are_allowlisted() {
        serde_json::from_value::<ChatMessage>(json!({"role": "tool", "content": "x"})).unwrap_err();
        let dev: ChatMessage = serde_json::from_value(json!({"role": "developer", "content": "x"})).unwrap();
        assert_eq!(dev.role, ChatRole::System);
    }

    #[test]
    fn max_completion_tokens_wins_over_max_tokens() {
        let request: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "m", "messages": [], "max_tokens": 9, "max_completion_tokens": 3, "stream": true
        }))
        .unwrap();
        assert_eq!(request.output_limit(), Some(3));
        assert!(request.wants_stream());
    }

    #[test]
    fn gate_content_chunk_is_minimal() {
        let chunk = ChatCompletionChunk::content("hello");
        assert_eq!(serde_json::to_string(&chunk).unwrap(), r#"{"choices":[{"index":0,"delta":{"content":"hello"}}]}"#);
    }

    #[test]
    fn engine_chunks_parse_and_extras_vanish() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "id": "cmpl-1", "object": "chat.completion.chunk", "created": 1, "model": "m",
            "choices": [{"index": 0, "delta": {"content": "w"}, "finish_reason": null,
                         "logprobs": null, "token_ids": [17]}],
            "prompt_token_ids": [1, 2, 3]
        }))
        .unwrap();
        assert_eq!(chunk.text(), Some("w"));
        assert_eq!(chunk.finish_reason(), None);

        let usage: ChatCompletionChunk = serde_json::from_value(json!({
            "choices": [], "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }))
        .unwrap();
        assert_eq!(usage.usage.unwrap().completion_tokens, 2);
        assert_eq!(usage.text(), None);
    }

    #[test]
    fn engine_request_has_the_spec_shape() {
        let request = EngineChatRequest::new("m".into(), vec![ChatMessage::new(ChatRole::User, "x")], 7);
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "model": "m", "messages": [{"role": "user", "content": "x"}], "stream": true,
                "stream_options": {"include_usage": true}, "max_completion_tokens": 7
            })
        );
    }

    #[test]
    fn model_list_shape() {
        let list = ModelList::new(["fake-small", "fake-large"], "counteractive");
        assert_eq!(
            serde_json::to_value(&list).unwrap(),
            json!({"object": "list", "data": [
                {"id": "fake-small", "object": "model", "owned_by": "counteractive"},
                {"id": "fake-large", "object": "model", "owned_by": "counteractive"}
            ]})
        );
    }
}
