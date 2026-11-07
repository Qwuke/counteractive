//! Closing a session (docs 05 section 5, Closing).
//!
//! A wallet's close, the idle and age timers, and the turn limit all run
//! the same path. A closing task owns the work from the moment the slot
//! becomes `Closing`: it produces the refund for `t = s − debits` and
//! retries its own disk write with backoff until it lands, whether or not
//! any caller is still waiting. The sealed result then waits for recover.

use std::sync::Arc;
use std::time::Duration;

use credit::Nullifier;
use proto::gate::CloseResponse;
use proto::{Blob, ErrorBody, ErrorCode, SessionId};
use tokio::sync::oneshot;

use super::{ActiveTurn, Phase, Sessions, Settled};

/// How long closing lets a running turn finish before aborting it.
const TURN_GRACE: Duration = Duration::from_secs(30);
/// The first and the longest wait between attempts to store the refund.
const FIRST_RETRY: Duration = Duration::from_millis(100);
const MAX_RETRY: Duration = Duration::from_secs(5);

/// The closing task's first answer: the refund, or why there isn't one yet.
type CloseReply = Result<Vec<u8>, ErrorCode>;

impl Sessions {
    /// A wallet's close: the refund, `404` with no session, or `503 retry`
    /// while closing is already under way or its write is being retried.
    pub async fn close(self: &Arc<Self>, id: SessionId) -> Result<CloseResponse, ErrorBody> {
        let reply = self.begin_close(id)?;
        match reply.await {
            Ok(Ok(refund)) => Ok(CloseResponse { refund: Blob::new(refund) }),
            Ok(Err(code)) => Err(code.into()),
            Err(_) => Err(ErrorCode::Retry.into()),
        }
    }

    /// Moves an `Open` session to `Closing` and starts its closing task.
    /// The receiver gets the task's first settlement attempt.
    pub(super) fn begin_close(self: &Arc<Self>, id: SessionId) -> Result<oneshot::Receiver<CloseReply>, ErrorCode> {
        let (nullifier, turn) = {
            let mut table = self.table();
            let nullifier = *table.sessions.get(&id).ok_or(ErrorCode::NotFound)?;
            let slot = table.slots.get_mut(&nullifier).ok_or(ErrorCode::NotFound)?;
            let (phase, turn) = match std::mem::replace(&mut slot.phase, Phase::Reserving) {
                Phase::Open(session) => {
                    let turn = session.turn.clone();
                    (Phase::Closing(session), Ok(turn))
                }
                // Closing already: its task will finish the job.
                other => (other, Err(ErrorCode::Retry)),
            };
            slot.phase = phase;
            (nullifier, turn?)
        };
        let (reply, receiver) = oneshot::channel();
        self.tasks.spawn(Arc::clone(self).run_close(nullifier, turn, reply));
        Ok(receiver)
    }

    async fn run_close(
        self: Arc<Self>,
        nullifier: Nullifier,
        turn: Option<ActiveTurn>,
        reply: oneshot::Sender<CloseReply>,
    ) {
        let mut reply = Some(reply);
        let mut answer = |result: CloseReply| {
            if let Some(reply) = reply.take() {
                let _ = reply.send(result);
            }
        };

        // 2: let a running turn finish, or abort it after the grace period.
        if let Some(turn) = turn
            && tokio::time::timeout(TURN_GRACE, turn.finished.cancelled()).await.is_err()
        {
            turn.abort.cancel();
            turn.finished.cancelled().await;
        }

        // 3: t = s − debits, read once no turn can debit again.
        let Some((spend, t)) =
            self.table().session_mut(&nullifier).map(|(s, _)| (Arc::clone(&s.spend), s.ledger.remaining()))
        else {
            return answer(Err(ErrorCode::NotFound));
        };

        // 4: verify again and refund.
        let refund = {
            let _permit = self.verify.wait().await;
            self.refund(&spend, t).await
        };
        let refund = match refund {
            Ok(refund) => refund,
            Err(error) => {
                // A spend verified at open can't fail here with the same
                // issuer. Drop the slot rather than leave it stuck; the row
                // stays Pending, as after a crash.
                tracing::error!(%error, "refunding a closing session failed");
                self.table().remove(&nullifier);
                return answer(Err(ErrorCode::Internal));
            }
        };

        // 5: seal and compare-and-set, retrying the write until it lands.
        let mut backoff = FIRST_RETRY;
        loop {
            match self.settle(nullifier, &spend, refund.clone()).await {
                Ok(settled) => {
                    // 6: drop the slot, and the spend bytes with it.
                    self.table().remove(&nullifier);
                    return answer(match settled {
                        Settled::Refund(refund) => Ok(refund),
                        Settled::Spent => Err(ErrorCode::Spent),
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "storing a refund failed; retrying");
                    answer(Err(ErrorCode::Retry));
                }
            }
            tokio::select! {
                () = self.shutdown.cancelled() => return,
                () = tokio::time::sleep(backoff) => {}
            }
            backoff = (backoff * 2).min(MAX_RETRY);
        }
    }
}
