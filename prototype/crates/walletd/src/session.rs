//! A chain's spend, driven through the gate's answers by the failure table
//! of docs 04 section 8.
//!
//! Each step reads the chain's journalled state, makes one gate call, and
//! journals the state that answer leads to before the next call. So an
//! error, a crash or a dropped connection leaves the chain in a state the
//! next attempt (or `reconcile`) can resume from with the same bytes.

use std::time::Duration;

use axum::http::StatusCode;
use crash_point::crash_point;
use gate_conn::{ConnError, DialScope};
use proto::gate::{CloseResponse, OpenSessionResponse, RecoverResponse};
use proto::walletd::{ChainState, SettledSessionView};
use proto::{Blob, ErrorCode, SessionId};
use wallet_core::chain::{self, PreparedSpend};
use wallet_core::{ChainId, WalletError};

use crate::error::ApiError;
use crate::gate::Link;
use crate::state::{Attachment, Context, Walletd};

/// Enough steps for the longest path in the failure table (Spending, use
/// recover, Recovering, live, Open, Closing, not found, Recovering, settled)
/// plus a reconnect, and few enough that a gate flapping between answers
/// can't hold a request forever.
const MAX_STEPS: usize = 10;

/// How long to wait after `503 retry`, such as while the gate's own
/// closing task finishes a session, and how many times.
const BUSY_WAIT: Duration = Duration::from_millis(200);
const MAX_BUSY: usize = 50;

/// A live session, and the connection it runs on.
pub struct LiveSession {
    /// The chain it spends.
    pub chain: ChainId,
    /// Which spend of that chain, for its receipts.
    pub spend_seq: u64,
    /// The gate's id for it.
    pub session_id: SessionId,
    /// Its connection and dial scope.
    pub link: Link,
    /// Keeps reconcile off the chain for as long as the session is held.
    attachment: Attachment,
}

/// Where driving a chain ended.
pub enum Reached {
    /// A live session with this id.
    Open(SessionId),
    /// Settled; `via_recover` when the gate had already ended the session,
    /// so the refund came from `recover`.
    Settled {
        /// The classified session.
        settled: SettledSessionView,
        /// Whether recover produced the refund.
        via_recover: bool,
    },
}

impl Walletd {
    /// Opens a session on a new connection in a fresh dial scope, so the
    /// gate has no connection-level link to any earlier session.
    ///
    /// `required` is the size a too-large prompt needs; without it any
    /// tier will do.
    pub async fn open_session(&self, context: &Context, required: Option<u64>) -> Result<LiveSession, ApiError> {
        let mut link = self.gate.connect(&context.manifest, DialScope::new()).await?;
        let maximum_spend = link
            .release()
            .map(|release| release.session_policy.maximum_spend)
            .ok_or_else(|| ApiError::local(StatusCode::BAD_GATEWAY, "gate_unreachable", "the connection closed"))?;
        // An open the gate settles at once (an out-of-policy spend) leaves
        // the chain Ready again, so another attempt may follow.
        for _ in 0..MAX_STEPS {
            let (prepared, attachment) = {
                let mut journal = self.journal();
                let tiers = &self.pins.tier_ladder;
                let prepared =
                    chain::prepare_spend(&mut journal, &context.holder, tiers, maximum_spend, required.unwrap_or(0))?;
                let Some(prepared) = prepared else { return Err(ApiError::no_chain(required)) };
                // Attached before the journal is released, so reconcile
                // never sees this Spending chain as an orphan.
                let attachment = self.attached.attach(prepared.chain);
                (prepared, attachment)
            };
            let PreparedSpend { chain, spend_seq, .. } = prepared;
            if let Reached::Open(session_id) = self.drive(context, &mut link, chain, false).await? {
                return Ok(LiveSession { chain, spend_seq, session_id, link, attachment });
            }
        }
        Err(ApiError::local(StatusCode::SERVICE_UNAVAILABLE, "retry", "the gate settled every spend at once"))
    }

    /// Closes a live session and settles its refund, recovering it if the
    /// gate had already ended it.
    pub async fn close_session(&self, context: &Context, session: LiveSession) -> Result<Reached, ApiError> {
        let LiveSession { chain, mut link, attachment, .. } = session;
        let reached = self.drive(context, &mut link, chain, true).await;
        drop(attachment);
        reached
    }

    /// Moves an unsettled chain on by its journalled state until it is
    /// open (when `close` is false) or settled. The caller holds the
    /// chain's [`Attachment`], so no other flow drives it meanwhile.
    ///
    /// Only the spend journalled when it starts is driven. If the chain
    /// moves on to another spend, the bytes in hand belong to a settled
    /// one, so it stops; the journal would refuse every step with them.
    pub async fn drive(
        &self,
        context: &Context,
        link: &mut Link,
        chain: ChainId,
        close: bool,
    ) -> Result<Reached, ApiError> {
        let spend = chain::stored_spend(&self.journal().chain(chain)?)?;
        let mut reconnected = false;
        let mut via_recover = false;
        let (mut steps, mut busy) = (0, 0);
        while steps < MAX_STEPS {
            self.gate.ensure_connected(link, &context.manifest).await?;
            let row = self.journal().chain(chain)?;
            if row.spend_seq != spend.spend_seq {
                return Err(ApiError::local(
                    StatusCode::CONFLICT,
                    "not_in_session",
                    format!("chain {chain} has moved on from the spend being settled"),
                ));
            }
            let outcome = match row.state {
                ChainState::Spending => self.step_open(context, link, &spend).await,
                ChainState::Open if !close => {
                    return Ok(Reached::Open(row.session_id.ok_or(WalletError::Corrupt("open chain without id"))?));
                }
                ChainState::Open | ChainState::Closing => {
                    let session_id = row.session_id.ok_or(WalletError::Corrupt("open chain without id"))?;
                    self.step_close(context, link, &spend, &session_id).await
                }
                ChainState::Recovering => {
                    via_recover = true;
                    self.step_recover(context, link, &spend).await
                }
                ChainState::Ready | ChainState::Dust | ChainState::Frozen => {
                    return Err(ApiError::local(
                        StatusCode::CONFLICT,
                        "not_in_session",
                        format!("chain {chain} has no unsettled spend"),
                    ));
                }
            };
            match outcome {
                Ok(Some(settled)) => return Ok(Reached::Settled { settled, via_recover }),
                Ok(None) => steps += 1,
                Err(Step::Busy(_)) if busy < MAX_BUSY => {
                    busy += 1;
                    tokio::time::sleep(BUSY_WAIT).await;
                }
                // A connection lost mid-call: every call here resends the
                // same bytes, so one retry on a new connection is safe.
                Err(Step::Transport(_)) if !reconnected => {
                    reconnected = true;
                    link.drop_connection();
                }
                Err(Step::Busy(error) | Step::Transport(error)) => return Err(error.into()),
                Err(Step::Failed(error)) => return Err(error),
            }
        }
        Err(ApiError::local(StatusCode::SERVICE_UNAVAILABLE, "retry", "the gate's answers did not settle; try again"))
    }

    /// Spending: resend the stored spend to open.
    async fn step_open(
        &self,
        context: &Context,
        link: &mut Link,
        spend: &PreparedSpend,
    ) -> Result<Option<SettledSessionView>, Step> {
        let (chain, seq) = (spend.chain, spend.spend_seq);
        match link.open(&spend.message).await {
            Ok(OpenSessionResponse::Open { session_id, .. }) => {
                // The gate holds a live session the journal doesn't know:
                // resending the spend must return that same session.
                crash_point!("after_open_response");
                self.journal().mark_open(chain, seq, &session_id).map_err(Step::wallet)?;
                Ok(None)
            }
            Ok(OpenSessionResponse::Settled { refund }) => self.settle(context, spend, &refund).map(Some),
            Err(error) => match error.error_code() {
                Some(ErrorCode::UseRecover) => {
                    self.journal().set_session_state(chain, seq, ChainState::Recovering).map_err(Step::wallet)?;
                    Ok(None)
                }
                Some(ErrorCode::Spent | ErrorCode::BadRequest) => Err(self.freeze(spend, &error)),
                _ => Err(Step::from(error)),
            },
        }
    }

    /// Open or Closing: close, or recover if the session is gone.
    async fn step_close(
        &self,
        context: &Context,
        link: &mut Link,
        spend: &PreparedSpend,
        session_id: &SessionId,
    ) -> Result<Option<SettledSessionView>, Step> {
        let (chain, seq) = (spend.chain, spend.spend_seq);
        self.journal().set_session_state(chain, seq, ChainState::Closing).map_err(Step::wallet)?;
        match link.close(session_id).await {
            Ok(CloseResponse { refund }) => self.settle(context, spend, &refund).map(Some),
            Err(error) if error.error_code() == Some(ErrorCode::NotFound) => {
                tracing::info!(chain = chain.0, "the gate no longer has the session; recovering");
                self.journal().set_session_state(chain, seq, ChainState::Recovering).map_err(Step::wallet)?;
                Ok(None)
            }
            Err(error) => Err(Step::from(error)),
        }
    }

    /// Recovering: ask what happened to the spend.
    async fn step_recover(
        &self,
        context: &Context,
        link: &mut Link,
        spend: &PreparedSpend,
    ) -> Result<Option<SettledSessionView>, Step> {
        let (chain, seq) = (spend.chain, spend.spend_seq);
        match link.recover(&spend.message).await {
            Ok(RecoverResponse::Settled { refund }) => self.settle(context, spend, &refund).map(Some),
            Ok(RecoverResponse::Live { session_id }) => {
                self.journal().mark_open(chain, seq, &session_id).map_err(Step::wallet)?;
                Ok(None)
            }
            Err(error) => match error.error_code() {
                // The gate has no row, as after a host rollback: the chain
                // keeps its value, and the same spend is sent again.
                Some(ErrorCode::NoRow) => {
                    self.journal().set_session_state(chain, seq, ChainState::Spending).map_err(Step::wallet)?;
                    Ok(None)
                }
                Some(ErrorCode::Spent) => Err(self.freeze(spend, &error)),
                _ => Err(Step::from(error)),
            },
        }
    }

    /// Finalises the spend's refund and commits the classified session.
    fn settle(&self, context: &Context, spend: &PreparedSpend, refund: &Blob) -> Result<SettledSessionView, Step> {
        let mut journal = self.journal();
        let tiers = &self.pins.tier_ladder;
        chain::settle(&mut journal, &context.holder, tiers, spend.chain, spend.spend_seq, refund.as_bytes())
            .map_err(Step::wallet)
    }

    /// Freezes a chain the gate refused a journalled proof for.
    fn freeze(&self, spend: &PreparedSpend, error: &ConnError) -> Step {
        let reason = format!("the gate refused a journalled spend: {error}");
        if let Err(journal_error) = chain::freeze(&mut self.journal(), spend.chain, spend.spend_seq, &reason) {
            return Step::wallet(journal_error);
        }
        let chain = spend.chain;
        Step::Failed(ApiError::local(StatusCode::CONFLICT, "chain_frozen", format!("chain {chain} frozen: {reason}")))
    }
}

/// Why a step stopped.
enum Step {
    /// `503 retry`: the gate is busy with this spend; ask again shortly.
    Busy(ConnError),
    /// The connection failed; the gate may not have seen the call.
    Transport(ConnError),
    /// Anything else, already in the client's terms.
    Failed(ApiError),
}

impl Step {
    fn wallet(error: WalletError) -> Self {
        Self::Failed(error.into())
    }
}

impl From<ConnError> for Step {
    fn from(error: ConnError) -> Self {
        match error.error_code() {
            Some(ErrorCode::Retry) => Self::Busy(error),
            Some(_) => Self::Failed(error.into()),
            None if matches!(error, ConnError::Api { .. }) => Self::Failed(error.into()),
            None => Self::Transport(error),
        }
    }
}
