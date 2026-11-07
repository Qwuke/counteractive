//! `chain` rows: one credential chain each, moving through the states of
//! ARCHITECTURE section 11. Every transition is a guarded update, so a
//! row in an unexpected state is refused rather than overwritten. Those
//! within a spend also name its `spend_seq`: a flow still holding an
//! earlier spend's bytes is refused rather than acting on the next spend.

use proto::SessionId;
use proto::walletd::{ChainState, SettledSessionView};
use rusqlite::Row;
use secrecy::{ExposeSecret as _, SecretSlice};

use super::purchase::expect_one;
use super::states::StateName as _;
use super::{ChainId, Journal, from_sql, to_sql};
use crate::WalletError;

/// One chain.
pub struct ChainRow {
    /// Local id.
    pub id: ChainId,
    /// Where it is in the state machine.
    pub state: ChainState,
    /// The credential, in Ready and Dust.
    pub credential: Option<SecretSlice<u8>>,
    /// Spends made from this chain so far; the current one's number.
    pub spend_seq: u64,
    /// The client spend state, from Spending until the refund is final.
    pub spend_state: Option<SecretSlice<u8>>,
    /// The spend bytes, resent unchanged on every retry.
    pub spend_message: Option<SecretSlice<u8>>,
    /// The live session, once open.
    pub session_id: Option<SessionId>,
    /// `s` of the current spend.
    pub budget: Option<u64>,
    /// Turns whose stream ended before a receipt.
    pub unknown_turns: u32,
}

impl std::fmt::Debug for ChainRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainRow")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("spend_seq", &self.spend_seq)
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

impl ChainRow {
    /// Whether a spend from this chain is journalled but not yet settled.
    pub fn is_unsettled(&self) -> bool {
        IN_SESSION.contains(&self.state)
    }
}

const COLUMNS: &str =
    "chain_id, state, credential, spend_seq, spend_state, spend_message, session_id, budget, unknown_turns";

/// The states a chain is in while a spend is unsettled.
const IN_SESSION: [ChainState; 4] =
    [ChainState::Spending, ChainState::Open, ChainState::Closing, ChainState::Recovering];

impl Journal {
    /// One chain.
    pub fn chain(&self, id: ChainId) -> Result<ChainRow, WalletError> {
        let mut statement = self.connection.prepare(&format!("SELECT {COLUMNS} FROM chain WHERE chain_id = ?1"))?;
        statement.query_and_then([id.0], read_row)?.next().unwrap_or(Err(WalletError::Corrupt("no such chain")))
    }

    /// Every chain, oldest first.
    pub fn chains(&self) -> Result<Vec<ChainRow>, WalletError> {
        let mut statement = self.connection.prepare(&format!("SELECT {COLUMNS} FROM chain ORDER BY chain_id"))?;
        statement.query_and_then([], read_row)?.collect()
    }

    /// `Ready` to `Spending`: replaces the credential with the spend state
    /// and message, raises `spend_seq`, and records `s`. This commits before
    /// the message is sent (docs 04 invariant 2). Returns the new `spend_seq`.
    pub fn begin_spend(
        &mut self,
        id: ChainId,
        spend_state: &SecretSlice<u8>,
        spend_message: &[u8],
        budget: u64,
    ) -> Result<u64, WalletError> {
        self.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE chain SET state = ?2, credential = NULL, spend_seq = spend_seq + 1, spend_state = ?3,
                    spend_message = ?4, session_id = NULL, budget = ?5, unknown_turns = 0
                 WHERE chain_id = ?1 AND state = ?6",
                (
                    id.0,
                    ChainState::Spending.name(),
                    spend_state.expose_secret(),
                    spend_message,
                    to_sql(budget)?,
                    ChainState::Ready.name(),
                ),
            )?;
            expect_one(changed, id.0, "not Ready")?;
            let seq: i64 =
                transaction.query_row("SELECT spend_seq FROM chain WHERE chain_id = ?1", [id.0], |row| row.get(0))?;
            from_sql(seq, "spend_seq")
        })
    }

    /// `Spending` or `Recovering` to `Open`, with the session's id.
    pub fn mark_open(&mut self, id: ChainId, spend_seq: u64, session_id: &SessionId) -> Result<(), WalletError> {
        self.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE chain SET state = ?3, session_id = ?4
                 WHERE chain_id = ?1 AND spend_seq = ?2 AND state IN (?5, ?6)",
                (
                    id.0,
                    to_sql(spend_seq)?,
                    ChainState::Open.name(),
                    session_id.as_bytes().as_slice(),
                    ChainState::Spending.name(),
                    ChainState::Recovering.name(),
                ),
            )?;
            expect_one(changed, id.0, "not Spending or Recovering in this spend")
        })
    }

    /// Moves an unsettled chain between in-session states, such as `Open`
    /// to `Closing` or `Recovering` back to `Spending`.
    pub fn set_session_state(&mut self, id: ChainId, spend_seq: u64, to: ChainState) -> Result<(), WalletError> {
        if !IN_SESSION.contains(&to) {
            return Err(WalletError::WrongState { id: id.0, found: format!("cannot move to {to:?} here") });
        }
        self.update_in_session(id, spend_seq, "state = ?3", to.name())
    }

    /// Counts a turn whose stream ended without a receipt.
    pub fn add_unknown_turn(&mut self, id: ChainId, spend_seq: u64) -> Result<(), WalletError> {
        self.update_in_session(id, spend_seq, "unknown_turns = unknown_turns + ?3", 1)
    }

    /// The `finalize_refund` transaction: stores the new credential, drops
    /// the spend state and message, and records the classified session,
    /// all at once (docs 04 section 5).
    pub fn settle_chain(
        &mut self,
        id: ChainId,
        credential: &SecretSlice<u8>,
        state: ChainState,
        settled: &SettledSessionView,
    ) -> Result<(), WalletError> {
        if !matches!(state, ChainState::Ready | ChainState::Dust) {
            return Err(WalletError::WrongState { id: id.0, found: format!("cannot settle into {state:?}") });
        }
        self.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE chain SET state = ?3, credential = ?4, spend_state = NULL, spend_message = NULL,
                    session_id = NULL, budget = NULL, unknown_turns = 0
                 WHERE chain_id = ?1 AND spend_seq = ?2 AND state IN (?5, ?6, ?7, ?8)",
                (
                    id.0,
                    to_sql(settled.spend_seq)?,
                    state.name(),
                    credential.expose_secret(),
                    IN_SESSION[0].name(),
                    IN_SESSION[1].name(),
                    IN_SESSION[2].name(),
                    IN_SESSION[3].name(),
                ),
            )?;
            expect_one(changed, id.0, "not in this session")?;
            transaction.execute(
                "INSERT INTO settled_session (chain_id, spend_seq, s, t, receipts_sum, outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    id.0,
                    to_sql(settled.spend_seq)?,
                    to_sql(settled.s)?,
                    to_sql(settled.t)?,
                    to_sql(settled.receipts_sum)?,
                    settled.outcome.name(),
                ),
            )?;
            Ok(())
        })
    }

    /// Moves a chain whose spend the gate or the refund refused to
    /// `Frozen`, keeping every column for investigation.
    pub fn freeze_chain(&mut self, id: ChainId, spend_seq: u64) -> Result<(), WalletError> {
        self.update_in_session(id, spend_seq, "state = ?3", ChainState::Frozen.name())
    }

    /// Applies `set` (whose one value is `?3`) to the chain while it is in
    /// a session of spend `spend_seq`; anything else is refused.
    fn update_in_session(
        &mut self,
        id: ChainId,
        spend_seq: u64,
        set: &'static str,
        value: impl rusqlite::ToSql,
    ) -> Result<(), WalletError> {
        self.write(|transaction| {
            let changed = transaction.execute(
                &format!("UPDATE chain SET {set} WHERE chain_id = ?1 AND spend_seq = ?2 AND state IN (?4, ?5, ?6, ?7)"),
                (
                    id.0,
                    to_sql(spend_seq)?,
                    value,
                    IN_SESSION[0].name(),
                    IN_SESSION[1].name(),
                    IN_SESSION[2].name(),
                    IN_SESSION[3].name(),
                ),
            )?;
            expect_one(changed, id.0, "not in this session")
        })
    }
}

fn read_row(row: &Row<'_>) -> Result<ChainRow, WalletError> {
    let session_id = match row.get::<_, Option<Vec<u8>>>(6)? {
        Some(bytes) => {
            let bytes: [u8; 16] = bytes.as_slice().try_into().map_err(|_| WalletError::Corrupt("session id"))?;
            Some(SessionId::from_bytes(bytes))
        }
        None => None,
    };
    Ok(ChainRow {
        id: ChainId(row.get(0)?),
        state: ChainState::from_name(&row.get::<_, String>(1)?)?,
        credential: row.get::<_, Option<Vec<u8>>>(2)?.map(SecretSlice::from),
        spend_seq: from_sql(row.get(3)?, "spend_seq")?,
        spend_state: row.get::<_, Option<Vec<u8>>>(4)?.map(SecretSlice::from),
        spend_message: row.get::<_, Option<Vec<u8>>>(5)?.map(SecretSlice::from),
        session_id,
        budget: row.get::<_, Option<i64>>(7)?.map(|budget| from_sql(budget, "budget")).transpose()?,
        unknown_turns: from_sql(row.get(8)?, "unknown_turns")?,
    })
}
