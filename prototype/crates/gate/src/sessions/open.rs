//! `POST /v1/sessions`, in the order of docs 05 section 5, Opening a session.
//!
//! Nothing reaches the engine before the Pending row commits, and a valid
//! proof never gets a bare rejection: the wallet gave up its credential to
//! make it, so an out-of-policy spend settles at once with a full refund.

use std::sync::Arc;
use std::time::Instant;

use crash_point::crash_point;
use credit::Nullifier;
use metering::SessionLedger;
use proto::gate::OpenSessionResponse;
use proto::{Blob, ErrorBody, ErrorCode, SecretBlob, SessionId};
use rand::TryRng as _;

use super::{Phase, Session, Sessions, Settled, Slot, SlotTable, SpendDigest};
use crate::blocking;
use crate::seal;
use crate::store::SpendStatus;

/// The admission table's verdict when it doesn't refuse.
enum Admit {
    /// No slot and no row: open a new session.
    Proceed,
    /// A retry of the open that made this live session.
    Existing(OpenSessionResponse),
}

impl Sessions {
    /// Opens a session for a spend, or settles it at once if its amount is
    /// out of policy.
    pub async fn open(&self, spend: SecretBlob) -> Result<OpenSessionResponse, ErrorBody> {
        let spend = Arc::new(spend);
        // 1 and 2: decode, read the nullifier, apply the admission table.
        let nullifier = self.moderator.peek_nullifier(spend.expose()).map_err(|_| ErrorCode::BadRequest)?;
        let digest = SpendDigest::of(spend.expose());
        if let Admit::Existing(open) = self.admit(&self.table(), &nullifier, &spend, &digest)? {
            return Ok(open);
        }

        // 3 and 4: verify behind the semaphore.
        let permit = self.verify.enter().await.ok_or(ErrorCode::Retry)?;
        let budget = self.verify_spend(&spend).await?;

        // 5: under the slot lock, admit again and reserve.
        let reservation = {
            let mut table = self.table();
            if let Admit::Existing(open) = self.admit(&table, &nullifier, &spend, &digest)? {
                return Ok(open);
            }
            table.slots.insert(nullifier, Slot { digest, phase: Phase::Reserving });
            Reservation { sessions: self, nullifier, armed: true }
        };

        // 6: out of policy settles now with t = s.
        if !(1..=self.policy.maximum_spend).contains(&budget) {
            let refund = self.refund(&spend, budget).await.map_err(|error| {
                tracing::error!(%error, "refunding a verified spend failed");
                ErrorCode::Internal
            })?;
            drop(permit);
            return match self.settle(nullifier, &spend, refund).await {
                Ok(Settled::Refund(refund)) => Ok(OpenSessionResponse::Settled { refund: Blob::new(refund) }),
                Ok(Settled::Spent) => Err(ErrorCode::Spent.into()),
                Err(error) => {
                    tracing::warn!(%error, "settling an out-of-policy spend failed");
                    Err(ErrorCode::Retry.into())
                }
            };
        }
        drop(permit);

        // 7: the Pending row, durable before anything reaches the engine.
        let id = new_session_id()?;
        let store = Arc::clone(&self.store);
        if let Err(error) = blocking(move || store.insert_pending(&nullifier)).await {
            tracing::warn!(%error, "storing a Pending row failed");
            return Err(ErrorCode::Retry.into());
        }
        crash_point!("after_insert_pending");

        // 8: the slot opens.
        let now = Instant::now();
        let session =
            Session { id, spend, ledger: SessionLedger::new(budget), opened: now, last_active: now, turn: None };
        reservation.open(session);
        Ok(OpenSessionResponse::Open { session_id: id, budget, policy: self.policy })
    }

    /// The admission table (docs 05 section 5): what a spend may do given
    /// its nullifier's slot and store row.
    fn admit(
        &self,
        table: &SlotTable,
        nullifier: &Nullifier,
        spend: &SecretBlob,
        digest: &SpendDigest,
    ) -> Result<Admit, ErrorCode> {
        if let Some(slot) = table.slots.get(nullifier) {
            if !slot.digest.matches(digest) {
                return Err(ErrorCode::Spent);
            }
            return match &slot.phase {
                Phase::Open(session) => Ok(Admit::Existing(OpenSessionResponse::Open {
                    session_id: session.id,
                    budget: session.ledger.budget(),
                    policy: self.policy,
                })),
                Phase::Reserving | Phase::Closing(_) => Err(ErrorCode::Retry),
            };
        }
        match self.store.spend_status(nullifier) {
            Ok(SpendStatus::NoRow) => Ok(Admit::Proceed),
            // With no slot, a Pending row means the gate crashed.
            Ok(SpendStatus::Pending) => Err(ErrorCode::UseRecover),
            Ok(SpendStatus::Settled(sealed)) => match seal::unseal(spend.expose(), &sealed) {
                Some(_) => Err(ErrorCode::UseRecover),
                None => Err(ErrorCode::Spent),
            },
            Err(error) => {
                tracing::warn!(%error, "reading a spend row failed");
                Err(ErrorCode::Retry)
            }
        }
    }
}

/// A `Reserving` slot, removed again unless the session opens.
struct Reservation<'a> {
    sessions: &'a Sessions,
    nullifier: Nullifier,
    armed: bool,
}

impl Reservation<'_> {
    /// `Reserving` becomes `Open`.
    fn open(mut self, session: Session) {
        let mut table = self.sessions.table();
        table.sessions.insert(session.id, self.nullifier);
        if let Some(slot) = table.slots.get_mut(&self.nullifier) {
            slot.phase = Phase::Open(session);
        }
        self.armed = false;
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.sessions.table().remove(&self.nullifier);
        }
    }
}

/// 128 random bits; possession is the only authority over a session.
fn new_session_id() -> Result<SessionId, ErrorCode> {
    let mut bytes = [0u8; 16];
    rand::rngs::SysRng.try_fill_bytes(&mut bytes).map_err(|_| ErrorCode::Internal)?;
    Ok(SessionId::from_bytes(bytes))
}
