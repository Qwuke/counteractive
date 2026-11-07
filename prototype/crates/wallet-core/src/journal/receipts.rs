//! `receipt` and `settled_session` rows: the local usage record.

use proto::walletd::{SettledSessionView, SettlementOutcome};
use proto::{FinishReason, Receipt};
use rusqlite::Row;

use super::states::StateName as _;
use super::{ChainId, Journal, from_sql, to_sql};
use crate::WalletError;

impl Journal {
    /// Records a turn's receipt. A receipt already recorded for that turn
    /// is kept: the gate numbers turns, so a second one is a replay.
    pub fn insert_receipt(&mut self, chain: ChainId, spend_seq: u64, receipt: &Receipt) -> Result<(), WalletError> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT OR IGNORE INTO receipt
                   (chain_id, spend_seq, turn, model, prompt_tokens, completion_tokens, charge, remaining, finish)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    chain.0,
                    to_sql(spend_seq)?,
                    receipt.turn,
                    &receipt.model,
                    receipt.prompt_tokens,
                    receipt.completion_tokens,
                    to_sql(receipt.charge)?,
                    to_sql(receipt.remaining)?,
                    receipt.finish.as_str(),
                ),
            )?;
            Ok(())
        })
    }

    /// The receipts of one spend, by turn.
    pub fn receipts(&self, chain: ChainId, spend_seq: u64) -> Result<Vec<Receipt>, WalletError> {
        let mut statement = self.connection.prepare(
            "SELECT turn, model, prompt_tokens, completion_tokens, charge, remaining, finish FROM receipt
             WHERE chain_id = ?1 AND spend_seq = ?2 ORDER BY turn",
        )?;
        statement.query_and_then((chain.0, to_sql(spend_seq)?), read_receipt)?.collect()
    }

    /// Every settled session, oldest first.
    pub fn settled_sessions(&self) -> Result<Vec<SettledSessionView>, WalletError> {
        let mut statement = self.connection.prepare(
            "SELECT chain_id, spend_seq, s, t, receipts_sum, outcome FROM settled_session ORDER BY chain_id, spend_seq",
        )?;
        statement
            .query_and_then([], |row| {
                Ok(SettledSessionView {
                    chain_id: row.get(0)?,
                    spend_seq: from_sql(row.get(1)?, "spend_seq")?,
                    s: from_sql(row.get(2)?, "s")?,
                    t: from_sql(row.get(3)?, "t")?,
                    receipts_sum: from_sql(row.get(4)?, "receipts_sum")?,
                    outcome: SettlementOutcome::from_name(&row.get::<_, String>(5)?)?,
                })
            })?
            .collect()
    }
}

fn read_receipt(row: &Row<'_>) -> Result<Receipt, WalletError> {
    let finish: String = row.get(6)?;
    Ok(Receipt {
        turn: row.get(0)?,
        model: row.get(1)?,
        prompt_tokens: row.get(2)?,
        completion_tokens: row.get(3)?,
        charge: from_sql(row.get(4)?, "charge")?,
        remaining: from_sql(row.get(5)?, "remaining")?,
        finish: FinishReason::from_engine(&finish).ok_or(WalletError::Corrupt("finish reason"))?,
    })
}
