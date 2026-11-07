//! Sessions: the slot table, its timers and the four flows (docs 05
//! section 5).
//!
//! A Pending row with no live session means "the gate crashed", so nothing
//! may create that appearance during normal work. The gate keeps one slot
//! per nullifier, and every session transition goes through this table
//! under one std mutex. The lock is never held across an await, a proof
//! verification, a disk write or inference. A store *read* under it is
//! allowed: opening and recovering must see slot and row together.

mod chat;
mod close;
mod open;
mod recover;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crash_point::crash_point;
use credit::{Moderator, Nullifier};
use metering::SessionLedger;
use proto::{ErrorCode, SecretBlob, SessionId, SessionPolicy};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use zeroize::Zeroizing;

use crate::admission::Admission;
use crate::blocking;
use crate::models::Models;
use crate::seal;
use crate::state::Unlocked;
use crate::store::{MoneyStore, SettleOutcome, StoreError};

/// Proof verifications at once (docs 05 section 7).
const VERIFICATIONS_AT_ONCE: usize = 18;
/// Verifications allowed to wait; more get `503 retry`.
const VERIFICATIONS_WAITING: usize = 64;
/// How often idle and age limits are checked.
const TIMER_TICK: Duration = Duration::from_millis(250);

/// The live sessions and what they need: the store, the issuer, the models.
pub struct Sessions {
    slots: Mutex<SlotTable>,
    store: Arc<dyn MoneyStore>,
    moderator: Arc<Moderator>,
    verify: Admission,
    policy: SessionPolicy,
    models: Arc<Models>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

/// Slots by nullifier, and the open or closing ones by session id.
#[derive(Default)]
struct SlotTable {
    slots: HashMap<Nullifier, Slot>,
    sessions: HashMap<SessionId, Nullifier>,
}

/// One nullifier's slot (docs 05 section 5, Session slots).
struct Slot {
    digest: SpendDigest,
    phase: Phase,
}

enum Phase {
    /// Verified; the Pending row is being committed, or an out-of-policy spend settled.
    Reserving,
    /// The session is live.
    Open(Session),
    /// No more turns; the refund is being produced and stored.
    Closing(Session),
}

/// A live session's state. Dropping it drops the spend bytes, when no
/// task still holds them.
struct Session {
    id: SessionId,
    spend: Arc<SecretBlob>,
    ledger: SessionLedger,
    opened: Instant,
    last_active: Instant,
    turn: Option<ActiveTurn>,
}

/// A running turn, as its slot sees it.
#[derive(Clone)]
struct ActiveTurn {
    /// Cancelled to stop the turn early, as closing does after its grace period.
    abort: CancellationToken,
    /// Cancelled once the turn has debited and left the slot.
    finished: CancellationToken,
}

impl ActiveTurn {
    fn new() -> Self {
        Self { abort: CancellationToken::new(), finished: CancellationToken::new() }
    }
}

/// SHA-256 of a spend's bytes: how a slot tells a retry of the same bytes
/// from a different proof. It's also the seal key's input, so it is zeroed
/// on drop and compared in constant time.
struct SpendDigest(Zeroizing<[u8; 32]>);

impl SpendDigest {
    fn of(spend: &[u8]) -> Self {
        Self(Zeroizing::new(Sha256::digest(spend).into()))
    }

    fn matches(&self, other: &Self) -> bool {
        self.0.ct_eq(&*other.0).into()
    }
}

/// What a settlement gives the wallet.
enum Settled {
    /// The nullifier's one refund: ours, or a winner's that our bytes unseal.
    Refund(Vec<u8>),
    /// Another proof settled this nullifier first.
    Spent,
}

/// Why a settlement didn't happen. Either way nothing was stored, and the
/// same settlement may be tried again.
#[derive(Debug, thiserror::Error)]
enum SettleFailure {
    #[error(transparent)]
    Seal(#[from] seal::SealError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl SlotTable {
    /// The session in a nullifier's slot, and whether it's still open.
    fn session_mut(&mut self, nullifier: &Nullifier) -> Option<(&mut Session, bool)> {
        match &mut self.slots.get_mut(nullifier)?.phase {
            Phase::Open(session) => Some((session, true)),
            Phase::Closing(session) => Some((session, false)),
            Phase::Reserving => None,
        }
    }

    /// Drops a slot and its session-id entry.
    fn remove(&mut self, nullifier: &Nullifier) {
        if let Some(slot) = self.slots.remove(nullifier)
            && let Phase::Open(session) | Phase::Closing(session) = slot.phase
        {
            self.sessions.remove(&session.id);
        }
    }
}

impl Sessions {
    /// The session service for an unlocked store, with its timers running
    /// until `shutdown`.
    pub fn start(
        unlocked: &Unlocked,
        policy: SessionPolicy,
        models: Arc<Models>,
        shutdown: CancellationToken,
        tasks: TaskTracker,
    ) -> Arc<Self> {
        let sessions = Arc::new(Self {
            slots: Mutex::new(SlotTable::default()),
            store: Arc::clone(&unlocked.store),
            moderator: Arc::clone(&unlocked.moderator),
            verify: Admission::new(VERIFICATIONS_AT_ONCE, VERIFICATIONS_WAITING),
            policy,
            models,
            shutdown,
            tasks,
        });
        sessions.tasks.spawn(Arc::clone(&sessions).run_timers());
        sessions
    }

    /// Slots currently held, in any phase.
    pub fn live_sessions(&self) -> u64 {
        self.table().slots.len() as u64
    }

    fn table(&self) -> MutexGuard<'_, SlotTable> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Seals `refund` under `spend` and settles the nullifier by
    /// compare-and-set (docs 05 section 5). A loser unseals the winner's
    /// row with its own bytes: the same bytes get that identical refund,
    /// different ones get [`Settled::Spent`].
    async fn settle(
        &self,
        nullifier: Nullifier,
        spend: &Arc<SecretBlob>,
        refund: Vec<u8>,
    ) -> Result<Settled, SettleFailure> {
        let sealed = seal::seal(spend.expose(), &refund)?;
        crash_point!("before_settle");
        let store = Arc::clone(&self.store);
        let outcome = blocking(move || store.settle(&nullifier, &sealed)).await?;
        crash_point!("after_settle");
        Ok(match outcome {
            SettleOutcome::Won => Settled::Refund(refund),
            SettleOutcome::Lost { sealed_refund } => match seal::unseal(spend.expose(), &sealed_refund) {
                Some(winner) => Settled::Refund(winner),
                None => Settled::Spent,
            },
        })
    }

    /// Verifies a spend off the async workers, returning `s`. Any failure,
    /// including a nonzero allowance, is `400`.
    async fn verify_spend(&self, spend: &Arc<SecretBlob>) -> Result<u64, ErrorCode> {
        let (moderator, spend) = (Arc::clone(&self.moderator), Arc::clone(spend));
        let facts = blocking(move || moderator.verify_spend(spend.expose())).await;
        facts.map(|facts| facts.budget).map_err(|_| ErrorCode::BadRequest)
    }

    /// The refund message for `t` of a spend, made off the async workers.
    /// It verifies the spend again: act-rs's verified value can't be kept.
    async fn refund(&self, spend: &Arc<SecretBlob>, t: u64) -> Result<Vec<u8>, credit::CreditError> {
        let (moderator, spend) = (Arc::clone(&self.moderator), Arc::clone(spend));
        blocking(move || moderator.refund(spend.expose(), t)).await
    }

    /// Closes sessions past the idle or age limit, through the same path
    /// as a wallet's close, until shutdown.
    async fn run_timers(self: Arc<Self>) {
        let mut tick = tokio::time::interval(TIMER_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = self.shutdown.cancelled() => return,
                _ = tick.tick() => {}
            }
            for id in self.expired(Instant::now()) {
                // Already closing or gone is fine: someone else got there.
                let _ = self.begin_close(id);
            }
        }
    }

    /// Open sessions idle with no turn running, or past their maximum age.
    fn expired(&self, now: Instant) -> Vec<SessionId> {
        let idle = Duration::from_secs(self.policy.idle_seconds);
        let max_age = Duration::from_secs(self.policy.max_age_seconds);
        let table = self.table();
        table
            .slots
            .values()
            .filter_map(|slot| match &slot.phase {
                Phase::Open(session)
                    if (session.turn.is_none() && now.duration_since(session.last_active) >= idle)
                        || now.duration_since(session.opened) >= max_age =>
                {
                    Some(session.id)
                }
                _ => None,
            })
            .collect()
    }
}
