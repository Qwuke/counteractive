//! `POST /v1/sessions/recover` (docs 05 section 5, Recovering).
//!
//! The slot and the store row are read together under the slot lock.
//! Separate reads could find no slot, then a Pending row from an open in
//! progress, and pay `t = s` on a live session.

use std::sync::Arc;

use credit::Nullifier;
use proto::gate::RecoverResponse;
use proto::{Blob, ErrorBody, ErrorCode, SecretBlob};

use super::{Phase, Sessions, Settled, SpendDigest};
use crate::seal;
use crate::store::SpendStatus;

/// The recover table's verdict when it doesn't refuse.
enum Found {
    /// The settled refund, as stored.
    Answer(RecoverResponse),
    /// A Pending row with no slot: the gate crashed while it was live.
    Crashed,
}

impl Sessions {
    /// Returns a settled session's refund, a live session's id, or, after
    /// a crash, settles the session with a full refund.
    pub async fn recover(&self, spend: SecretBlob) -> Result<RecoverResponse, ErrorBody> {
        let spend = Arc::new(spend);
        let nullifier = self.moderator.peek_nullifier(spend.expose()).map_err(|_| ErrorCode::BadRequest)?;
        match self.recover_table(&nullifier, &spend)? {
            Found::Answer(response) => Ok(response),
            Found::Crashed => self.recover_crashed(nullifier, &spend).await,
        }
    }

    fn recover_table(&self, nullifier: &Nullifier, spend: &SecretBlob) -> Result<Found, ErrorCode> {
        let table = self.table();
        if let Some(slot) = table.slots.get(nullifier) {
            if !slot.digest.matches(&SpendDigest::of(spend.expose())) {
                return Err(ErrorCode::Spent);
            }
            return match &slot.phase {
                Phase::Open(session) => Ok(Found::Answer(RecoverResponse::Live { session_id: session.id })),
                Phase::Reserving | Phase::Closing(_) => Err(ErrorCode::Retry),
            };
        }
        match self.store.spend_status(nullifier) {
            Ok(SpendStatus::NoRow) => Err(ErrorCode::NoRow),
            Ok(SpendStatus::Pending) => Ok(Found::Crashed),
            Ok(SpendStatus::Settled(sealed)) => match seal::unseal(spend.expose(), &sealed) {
                Some(refund) => Ok(Found::Answer(RecoverResponse::Settled { refund: Blob::new(refund) })),
                None => Err(ErrorCode::Spent),
            },
            Err(error) => {
                tracing::warn!(%error, "reading a spend row failed");
                Err(ErrorCode::Retry)
            }
        }
    }

    /// The crashed case: verify under the semaphore, refund `t = s`, seal,
    /// compare-and-set. A different valid proof for the same nullifier may
    /// win instead; it gets a full refund of its own `s`, and this one `409 spent`.
    async fn recover_crashed(
        &self,
        nullifier: Nullifier,
        spend: &Arc<SecretBlob>,
    ) -> Result<RecoverResponse, ErrorBody> {
        let refund = {
            let _permit = self.verify.enter().await.ok_or(ErrorCode::Retry)?;
            let budget = self.verify_spend(spend).await?;
            self.refund(spend, budget).await.map_err(|_| ErrorCode::BadRequest)?
        };
        match self.settle(nullifier, spend, refund).await {
            Ok(Settled::Refund(refund)) => Ok(RecoverResponse::Settled { refund: Blob::new(refund) }),
            Ok(Settled::Spent) => Err(ErrorCode::Spent.into()),
            Err(error) => {
                tracing::warn!(%error, "settling a crashed session failed");
                Err(ErrorCode::Retry.into())
            }
        }
    }
}
