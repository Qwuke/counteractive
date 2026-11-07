//! One chat turn through a conversation's session, with the exhaustion
//! handling of docs 04 section 7 and the chat rows of the failure table.
//!
//! A request rolls over to a new session at most once, whatever the
//! reason, so a retried request can't loop (docs 04 invariant 10).

use std::time::Duration;

use gate_conn::{ConnError, SseStream};
use proto::gate::SessionChatRequest;
use proto::openai::ChatCompletionChunk;
use proto::sse::{EVENT_DONE, EVENT_RECEIPT};
use proto::walletd::{ChainState, SettlementOutcome};
use proto::{ErrorCode, Receipt};
use wallet_core::ChainId;

use crate::conversation::Conversation;
use crate::error::ApiError;
use crate::session::Reached;
use crate::state::{Context, Walletd};

/// How long to wait before asking again after `409 turn_active`, which
/// follows a lost stream whose turn hasn't ended at the gate yet.
const TURN_ACTIVE_WAIT: Duration = Duration::from_millis(500);
/// How many times to ask; the gate aborts a stalled turn within 30 s.
const TURN_ACTIVE_TRIES: u32 = 80;

/// What happened to the conversation's session before the turn started.
#[derive(Debug, Default, Clone)]
pub struct Notices {
    /// Why walletd ended the session and opened a new one, as a gate code.
    pub rollover: Option<String>,
    /// The outcome of a session the gate had lost, recovered on the way.
    pub recovered: Option<SettlementOutcome>,
}

/// A turn the gate has started: its events, and where its receipt goes.
pub struct StartedTurn {
    stream: SseStream,
    chain: ChainId,
    spend_seq: u64,
    receipt: Option<Receipt>,
    ended: bool,
    /// What happened to the session before the turn started.
    pub notices: Notices,
}

impl StartedTurn {
    /// The next piece of model text, or `None` once the turn has ended.
    /// The receipt is journalled as soon as it arrives.
    pub async fn next_content(&mut self, walletd: &Walletd) -> Option<String> {
        while !self.ended {
            let Ok(Some(event)) = self.stream.next_event().await else { break };
            if event.is(EVENT_RECEIPT) {
                let Ok(receipt) = serde_json::from_str::<Receipt>(&event.data) else { break };
                if let Err(error) = walletd.journal().insert_receipt(self.chain, self.spend_seq, &receipt) {
                    tracing::error!(%error, "journalling a receipt failed");
                }
                self.receipt = Some(receipt);
            } else if event.is(EVENT_DONE) {
                break;
            } else if event.event.is_none() {
                let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(&event.data) else { break };
                if let Some(text) = chunk.text().filter(|text| !text.is_empty()) {
                    return Some(text.to_owned());
                }
            }
        }
        self.ended = true;
        None
    }

    /// Ends the turn; call it once. One that ended without a receipt is an
    /// unknown turn: counted for reconcile, never rerun, and its connection
    /// is dropped, since an unfinished response leaves it unusable.
    pub fn finish(&mut self, walletd: &Walletd, conversation: &mut Conversation) -> Option<Receipt> {
        self.ended = true;
        conversation.last_used = std::time::Instant::now();
        if self.receipt.is_none() {
            if let Err(error) = walletd.journal().add_unknown_turn(self.chain, self.spend_seq) {
                tracing::error!(%error, "journalling an unknown turn failed");
            }
            if let Some(session) = conversation.session.as_mut() {
                session.link.drop_connection();
            }
        }
        self.receipt.clone()
    }
}

impl Walletd {
    /// Starts a turn, opening, rolling over or recovering the session as
    /// the gate's answers require. Errors before the stream starts charge
    /// nothing, except a lost connection, counted as an unknown turn.
    pub async fn start_turn(
        &self,
        context: &Context,
        conversation: &mut Conversation,
        request: &SessionChatRequest,
    ) -> Result<StartedTurn, ApiError> {
        let mut notices = Notices::default();
        let mut opened_new = false;
        let mut required = None;
        let mut waits = 0;
        loop {
            let session = match conversation.session.as_mut() {
                Some(session) => session,
                None => conversation.session.insert(self.open_session(context, required).await?),
            };
            self.gate.ensure_connected(&mut session.link, &context.manifest).await?;
            let error = match session.link.chat(&session.session_id, request).await {
                Ok(stream) => {
                    let (chain, spend_seq) = (session.chain, session.spend_seq);
                    return Ok(StartedTurn { stream, chain, spend_seq, receipt: None, ended: false, notices });
                }
                Err(error) => error,
            };
            let (chain, spend_seq) = (session.chain, session.spend_seq);
            match error.error_code() {
                // This session can't take the turn: settle it, and roll
                // over once to a session that can. A prompt too large for
                // this session needs a chain whose tier covers `required`.
                Some(code @ (ErrorCode::SessionExhausted | ErrorCode::Closing | ErrorCode::PromptExceedsBudget)) => {
                    let needed = match &error {
                        ConnError::Api { error: Some(body), .. } => body.required,
                        _ => None,
                    };
                    if let Some(session) = conversation.session.take() {
                        self.close_session(context, session).await?;
                    }
                    if opened_new {
                        return Err(error.into());
                    }
                    opened_new = true;
                    notices.rollover = Some(code.to_string());
                    if code == ErrorCode::PromptExceedsBudget {
                        required = needed;
                    }
                }
                // The gate lost the session, as after a crash: recover it.
                // A live answer continues it; a settled one ends it.
                Some(ErrorCode::NotFound) => {
                    self.journal().set_session_state(chain, spend_seq, ChainState::Recovering)?;
                    match self.drive(context, &mut session.link, chain, false).await? {
                        Reached::Open(session_id) => session.session_id = session_id,
                        Reached::Settled { settled, .. } => {
                            tracing::info!(chain = chain.0, outcome = ?settled.outcome, "recovered a lost session");
                            conversation.session = None;
                            if opened_new {
                                return Err(error.into());
                            }
                            opened_new = true;
                            notices.recovered = Some(settled.outcome);
                        }
                    }
                }
                Some(ErrorCode::TurnActive) if waits < TURN_ACTIVE_TRIES => {
                    waits += 1;
                    tokio::time::sleep(TURN_ACTIVE_WAIT).await;
                }
                // `400 model`, `400 zero_output` and the rest go to the
                // client; the session stays open.
                Some(_) => return Err(error.into()),
                None => {
                    // The connection failed after the request may have
                    // reached the gate, so the turn may have been charged.
                    self.journal().add_unknown_turn(chain, spend_seq)?;
                    session.link.drop_connection();
                    return Err(error.into());
                }
            }
        }
    }
}
