//! `POST /v1/sessions/{id}/chat`: one turn (docs 05 section 5, One turn).
//!
//! The hold, the clamp and the one debit rule come from `metering`; this
//! module runs them against the slot and the engine. Once the engine has
//! started, the turn always settles its debit, however it ends: completed,
//! client gone, reader stalled, closed early, or engine failed.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

use backend::{EngineEvent, EngineRequest, EngineStream, OutputLimit};
use bytes::Bytes;
use credit::Nullifier;
use futures::Stream;
use metering::{ModelTerms, TurnCharge, TurnEnd, TurnPlan};
use proto::gate::SessionChatRequest;
use proto::openai::ChatCompletionChunk;
use proto::sse::{EVENT_DONE, EVENT_RECEIPT, SseEvent};
use proto::{ErrorBody, ErrorCode, FinishReason, Receipt, SessionId};
use tokio::sync::mpsc;

use super::{ActiveTurn, Session, Sessions};

/// Encoded SSE frames buffered toward a slow reader.
const TURN_BUFFER: usize = 32;
/// A reader that takes no frame for this long has stalled; the turn aborts
/// so a client can't hold the GPU by not reading.
const STALL_LIMIT: Duration = Duration::from_secs(30);
/// An engine that sends nothing for this long has failed.
const ENGINE_SILENCE_LIMIT: Duration = Duration::from_secs(120);

/// A turn's encoded SSE frames, in order, ending after `event: done`.
pub struct TurnStream(mpsc::Receiver<Bytes>);

impl TurnStream {
    /// The frames as a response body stream. Dropping it tells the turn
    /// its client has gone.
    pub fn into_body(self) -> impl Stream<Item = Result<Bytes, Infallible>> + Send + 'static {
        futures::stream::unfold(self.0, |mut frames| async move {
            let frame = frames.recv().await?;
            Some((Ok(frame), frames))
        })
    }
}

impl Sessions {
    /// Admits a turn and starts it. On success the engine is running and
    /// the debit is certain; errors before that charge nothing.
    pub async fn chat(self: &Arc<Self>, id: SessionId, request: SessionChatRequest) -> Result<TurnStream, ErrorBody> {
        // 1: one turn at a time, and none once closing. 2 was the allowlisted body.
        let claim = self.claim_turn(id)?;
        if request.messages.is_empty() {
            return Err(ErrorCode::BadRequest.into());
        }

        // 3: the model, from the release catalogue.
        let model = self.models.get(&request.model).ok_or(ErrorCode::Model)?;
        let engine_request = EngineRequest { model: model.id.clone(), messages: request.messages };

        // 5: prompt tokens for exactly the messages the engine will get.
        let prompt_tokens = model.backend.count_prompt(&engine_request).await.map_err(|error| {
            tracing::warn!(%error, "counting a prompt failed");
            ErrorCode::EngineUnavailable
        })?;

        // 6: the admission decision, hold and clamp.
        let plan = claim.plan(&model.terms, prompt_tokens, request.max_completion_tokens)?;
        let limit = OutputLimit::new(plan.allowed_output).ok_or(ErrorCode::Internal)?;

        // 7: start. A failure here charges nothing.
        let stream = model.backend.start(engine_request, limit).await.map_err(|error| {
            tracing::warn!(%error, "starting a turn failed");
            ErrorCode::EngineUnavailable
        })?;

        let (frames, body) = mpsc::channel(TURN_BUFFER);
        let turn = Turn { claim, plan, terms: model.terms, model: model.id.clone(), frames };
        self.tasks.spawn(turn.run(stream));
        Ok(TurnStream(body))
    }

    /// Marks a turn active on an open session.
    fn claim_turn(self: &Arc<Self>, id: SessionId) -> Result<TurnClaim, ErrorCode> {
        let mut table = self.table();
        let nullifier = *table.sessions.get(&id).ok_or(ErrorCode::NotFound)?;
        let (session, open) = table.session_mut(&nullifier).ok_or(ErrorCode::NotFound)?;
        if !open {
            return Err(ErrorCode::Closing);
        }
        if session.turn.is_some() {
            return Err(ErrorCode::TurnActive);
        }
        let turn = ActiveTurn::new();
        session.turn = Some(turn.clone());
        session.last_active = Instant::now();
        Ok(TurnClaim { sessions: Arc::clone(self), nullifier, id, turn, released: false })
    }
}

/// A started turn and everything it needs to settle.
struct Turn {
    claim: TurnClaim,
    plan: TurnPlan,
    terms: ModelTerms,
    model: String,
    frames: mpsc::Sender<Bytes>,
}

impl Turn {
    /// Relays the engine's output, then debits, sends the receipt and
    /// `done`, and starts closing if the session reached its turn limit.
    async fn run(self, mut stream: EngineStream) {
        let Self { claim, plan, terms, model, frames } = self;
        let sessions = Arc::clone(&claim.sessions);
        let mut engine_finish = None;
        let end = loop {
            let event = tokio::select! {
                () = claim.turn.abort.cancelled() => break TurnEnd::ClientGone,
                () = sessions.shutdown.cancelled() => break TurnEnd::ClientGone,
                () = frames.closed() => break TurnEnd::ClientGone,
                event = tokio::time::timeout(ENGINE_SILENCE_LIMIT, stream.next()) => event,
            };
            match event {
                Ok(Some(Ok(EngineEvent::Delta { text, .. }))) => {
                    if frames.send_timeout(content_frame(&text), STALL_LIMIT).await.is_err() {
                        break TurnEnd::ClientGone;
                    }
                }
                Ok(Some(Ok(EngineEvent::Finish(reason)))) => engine_finish = Some(reason),
                Ok(Some(Ok(EngineEvent::Usage { .. }))) => {}
                Ok(None) => break TurnEnd::Completed,
                Ok(Some(Err(error))) => {
                    tracing::warn!(%error, "engine failed mid-turn");
                    break TurnEnd::EngineFault;
                }
                Err(_) => {
                    tracing::warn!("engine went silent mid-turn");
                    break TurnEnd::EngineFault;
                }
            }
        };
        // The engine's usage if it sent one, else the tokens relayed.
        let completion = stream.completion_tokens();
        // Closes the engine socket, which stops generation.
        drop(stream);

        let finish = match end {
            TurnEnd::Completed => engine_finish.unwrap_or(FinishReason::Stop),
            TurnEnd::ClientGone => FinishReason::Abort,
            TurnEnd::EngineFault => FinishReason::Error,
        };
        let id = claim.id;
        let Some((charge, turn_limit_reached)) = claim.settle(&plan, &terms, completion, end) else {
            return;
        };
        // A client that's gone misses the receipt; its debit stands.
        let receipt = charge.receipt(&model, &plan, completion, finish);
        if frames.send_timeout(receipt_frame(&receipt), STALL_LIMIT).await.is_ok() {
            let _ = frames.send_timeout(done_frame(), STALL_LIMIT).await;
        }
        if turn_limit_reached {
            // Already closing or gone is fine: someone else got there.
            let _ = sessions.begin_close(id);
        }
    }
}

/// A turn marked active in its slot. Released when settled or dropped, so
/// no exit path leaves the session stuck with a phantom turn.
struct TurnClaim {
    sessions: Arc<Sessions>,
    nullifier: Nullifier,
    id: SessionId,
    turn: ActiveTurn,
    released: bool,
}

impl TurnClaim {
    /// Plans the turn against the session's ledger, if it's still open.
    fn plan(&self, terms: &ModelTerms, prompt_tokens: u32, requested: Option<u32>) -> Result<TurnPlan, ErrorBody> {
        let mut table = self.sessions.table();
        match table.session_mut(&self.nullifier) {
            Some((session, true)) => {
                session.ledger.plan(terms, prompt_tokens, requested).map_err(|refusal| refusal.error_body())
            }
            Some((_, false)) => Err(ErrorCode::Closing.into()),
            None => Err(ErrorCode::NotFound.into()),
        }
    }

    /// Debits the turn under the one debit rule and frees the session for
    /// the next. Returns the charge, and whether an open session has now
    /// reached its turn limit.
    fn settle(
        mut self,
        plan: &TurnPlan,
        terms: &ModelTerms,
        completion: u32,
        end: TurnEnd,
    ) -> Option<(TurnCharge, bool)> {
        let max_turns = self.sessions.policy.max_turns;
        self.release(|session, open| {
            let charge = session.ledger.settle(plan, terms, completion, end);
            (charge, open && charge.turn >= max_turns)
        })
    }

    /// Clears the slot's turn, runs `last` on the session under the same
    /// lock, then signals anyone waiting for the turn to finish.
    fn release<R>(&mut self, last: impl FnOnce(&mut Session, bool) -> R) -> Option<R> {
        let result = {
            let mut table = self.sessions.table();
            table.session_mut(&self.nullifier).map(|(session, open)| {
                session.turn = None;
                session.last_active = Instant::now();
                last(session, open)
            })
        };
        self.released = true;
        self.turn.finished.cancel();
        result
    }
}

impl Drop for TurnClaim {
    fn drop(&mut self) {
        if !self.released {
            self.release(|_, _| ());
        }
    }
}

fn content_frame(text: &str) -> Bytes {
    let chunk = serde_json::to_string(&ChatCompletionChunk::content(text)).expect("a chunk always serialises");
    Bytes::from(SseEvent::data(chunk).encode())
}

fn receipt_frame(receipt: &Receipt) -> Bytes {
    let data = serde_json::to_string(receipt).expect("a receipt always serialises");
    Bytes::from(SseEvent::named(EVENT_RECEIPT, data).encode())
}

fn done_frame() -> Bytes {
    Bytes::from(SseEvent::named(EVENT_DONE, "{}").encode())
}
