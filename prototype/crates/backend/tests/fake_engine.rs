//! The `openai` adapter against an in-process `fake-engine` on a temp socket:
//! counting, a full stream, a mid-stream death with no usage chunk, and a
//! dropped stream reaching the engine as a cancellation.

#![expect(clippy::unwrap_used, reason = "test helpers: a setup step that fails should fail the test at once")]

use std::time::{Duration, Instant};

use backend::{BackendError, EngineEvent, EngineRequest, InferenceBackend as _, OpenAiBackend, OutputLimit};
use fake_engine::RunningEngine;
use fake_engine::script::{self, Faults};
use proto::FinishReason;
use proto::openai::{ChatMessage, ChatRole};
use tokio::net::UnixListener;

/// The fake engine on a temp socket, stopped when dropped.
struct Engine {
    backend: OpenAiBackend,
    running: RunningEngine,
    _dir: tempfile::TempDir,
}

fn start() -> Engine {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("engine.sock");
    let running = fake_engine::spawn(UnixListener::bind(&socket).unwrap());
    Engine { backend: OpenAiBackend::new(socket), running, _dir: dir }
}

fn request(text: &str) -> EngineRequest {
    EngineRequest {
        model: "fake-small".into(),
        messages: vec![ChatMessage::new(ChatRole::System, "be brief"), ChatMessage::new(ChatRole::User, text)],
    }
}

/// Reads every event, returning the deltas' text, the finish reason and the end result.
async fn read_all(stream: &mut backend::EngineStream) -> (String, Option<FinishReason>, Result<(), BackendError>) {
    let (mut text, mut finish) = (String::new(), None);
    while let Some(event) = stream.next().await {
        match event {
            Ok(EngineEvent::Delta { text: delta, .. }) => text.push_str(&delta),
            Ok(EngineEvent::Finish(reason)) => finish = Some(reason),
            Ok(EngineEvent::Usage { .. }) => {}
            Err(error) => return (text, finish, Err(error)),
        }
    }
    (text, finish, Ok(()))
}

#[tokio::test]
async fn counts_the_prompt_like_tokenize() {
    let engine = start();
    let req = request("how are you today");
    assert_eq!(engine.backend.count_prompt(&req).await.unwrap(), script::prompt_tokens(&req.messages));
    assert_eq!(engine.backend.count_prompt(&req).await.unwrap(), 4 + 2 + 4 + 4);
}

#[tokio::test]
async fn streams_a_whole_turn_with_usage() {
    let engine = start();
    let req = request("how are you today");
    let expected = script::output_tokens(&req.messages, 4096, &Faults::default());
    let mut stream = engine.backend.start(req, OutputLimit::new(4096).unwrap()).await.unwrap();
    let (text, finish, result) = read_all(&mut stream).await;
    result.unwrap();
    assert_eq!(u32::try_from(text.split_whitespace().count()), Ok(expected));
    assert_eq!(finish, Some(FinishReason::Stop));
    assert!(stream.usage_reported());
    assert_eq!(stream.completion_tokens(), expected);
    drop(stream);
    assert_eq!(engine.running.stats().completed(), 1);
}

#[tokio::test]
async fn the_limit_caps_output_and_reports_length() {
    let engine = start();
    let mut stream = engine.backend.start(request("[[long]]"), OutputLimit::new(40).unwrap()).await.unwrap();
    let (_, finish, result) = read_all(&mut stream).await;
    result.unwrap();
    assert_eq!(finish, Some(FinishReason::Length));
    assert_eq!(stream.completion_tokens(), 40);
}

#[tokio::test]
async fn a_dying_engine_truncates_and_tokens_are_counted() {
    let engine = start();
    let mut stream = engine.backend.start(request("go [[die-after:5]]"), OutputLimit::new(100).unwrap()).await.unwrap();
    let (text, finish, result) = read_all(&mut stream).await;
    assert!(result.is_err(), "a cut stream is an error, not a clean end");
    assert_eq!(text.split_whitespace().count(), 5);
    assert_eq!(finish, None);
    assert!(!stream.usage_reported());
    assert_eq!(stream.completion_tokens(), 5);
    assert!(stream.next().await.is_none(), "the stream ends after its error");
    assert_eq!(engine.running.stats().died(), 1);
}

#[tokio::test]
async fn dropping_the_stream_cancels_upstream() {
    let engine = start();
    let stream =
        engine.backend.start(request("[[stall:300]] [[long]]"), OutputLimit::new(4000).unwrap()).await.unwrap();
    drop(stream);
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.running.stats().cancelled() == 0 {
        assert!(Instant::now() < deadline, "the engine never saw the cancellation");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(engine.running.stats().completed(), 0);
}

#[tokio::test]
async fn an_absent_engine_fails_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let backend = OpenAiBackend::new(dir.path().join("missing.sock"));
    let error = backend.start(request("hi"), OutputLimit::new(1).unwrap()).await.unwrap_err();
    assert!(matches!(error, BackendError::Connect(_)));
    assert!(matches!(backend.count_prompt(&request("hi")).await, Err(BackendError::Connect(_))));
}
