//! walletd's local API under `/wallet` (SPEC section 7, docs 04 sections 3, 7 and 10).
//!
//! The `/v1` routes are the OpenAI subset in [`crate::openai`]. These types
//! are what `wallet-cli` and the embedded page read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Receipt;

/// Header naming a conversation, so its turns share one session and circuit.
pub const CONVERSATION_HEADER: &str = "x-conversation-id";

/// Response header set when walletd ended the conversation's session and
/// opened one new session for this request. Its value is why: the gate's
/// error code, such as `session_exhausted`.
pub const ROLLOVER_HEADER: &str = "x-counteractive-rollover";

/// Response header set when walletd found the conversation's session gone
/// and recovered its spend. Its value is the settled session's outcome,
/// such as `full_refund` after a gate crash.
pub const RECOVERED_HEADER: &str = "x-counteractive-recovered";

/// A walletd chat answer, streamed chunk or whole completion, with the
/// gate's receipt for the turn on the last one. OpenAI clients ignore the
/// extra field; the page and `wallet-cli` show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithReceipt<T> {
    /// The OpenAI-shaped body.
    #[serde(flatten)]
    pub body: T,
    /// The turn's receipt, when this is the last part and one arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Receipt>,
}

/// An error from walletd's local API.
///
/// Errors the gate sent keep the gate's code and `required`. Errors of
/// walletd's own, such as having no spendable chain, use codes outside the
/// gate's set, so this carries the code as text and a message for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalErrorBody {
    /// A stable code.
    pub error: String,
    /// What happened, for a person to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// For `prompt_exceeds_budget`: the credits a session would need.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<u64>,
}

/// A purchase before it becomes a chain (docs 04 section 4, `purchase.state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PurchaseState {
    /// Journalled; the order isn't created yet.
    Ordered,
    /// Created at the storefront; waiting for payment.
    AwaitingPayment,
    /// Paid and issued; a chain exists.
    Collected,
    /// Needs investigation by hand.
    Frozen,
    /// The user gave up.
    Abandoned,
}

/// A chain's state (ARCHITECTURE section 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainState {
    /// Holds a credential; spendable.
    Ready,
    /// Holds a credential below the smallest tier; not spendable.
    Dust,
    /// A proof is journalled; the open hasn't been answered.
    Spending,
    /// A session is live.
    Open,
    /// Close requested; the refund isn't finalised.
    Closing,
    /// Asking the gate what happened to a spend.
    Recovering,
    /// Needs investigation by hand.
    Frozen,
}

impl ChainState {
    /// Whether the chain's value is known exactly (Ready, Dust or Open).
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::Ready | Self::Dust | Self::Open)
    }
}

/// An output-token upper bound for one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenEstimate {
    /// Credits divided by the model's output rate; input also costs, so this is a ceiling.
    pub output_tokens_at_most: u64,
}

/// One chain in the balance view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainView {
    /// Local chain id.
    pub id: i64,
    /// Its state.
    pub state: ChainState,
    /// Its contribution, per the table in docs 04 section 7.
    pub credits: u64,
    /// Whether `credits` is exact.
    pub resolved: bool,
}

/// An order not yet paid, shown apart from the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOrderView {
    /// Product id.
    pub product: String,
    /// The credits it will carry.
    pub credits: u64,
}

/// `GET /wallet/balance` (docs 04 section 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceView {
    /// Ready and in-session credits; excludes orders, dust and frozen chains.
    pub total_credits: u64,
    /// `total_credits` in dollars, as a decimal string with six places.
    pub total_usd: String,
    /// Per model id.
    pub estimated_tokens_remaining: BTreeMap<String, TokenEstimate>,
    /// Every chain.
    pub chains: Vec<ChainView>,
    /// Orders awaiting payment.
    pub orders_awaiting_payment: Vec<PendingOrderView>,
    /// Purchases needing investigation.
    pub frozen_orders: u64,
    /// Chains needing investigation.
    pub frozen_chains: u64,
    /// Credits stranded in Dust chains.
    pub dust_credits: u64,
    /// Whether the accepted release runs on a platform without full verification.
    pub experimental_platform: bool,
    /// The accepted manifest's version, if one is accepted.
    pub manifest_version: Option<u64>,
}

/// How a settled session's receipts compare with `s − t` (docs 04 section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementOutcome {
    /// `receipts_sum == s − t`.
    Exact,
    /// `t == s` though receipts were present, as after a gate crash.
    FullRefund,
    /// Less was receipted than charged, and some turns ended without a receipt.
    ReceiptGap,
    /// Any other difference; reported loudly.
    Mismatch,
}

impl SettlementOutcome {
    /// The outcome as it appears on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::FullRefund => "full_refund",
            Self::ReceiptGap => "receipt_gap",
            Self::Mismatch => "mismatch",
        }
    }
}

/// One row of `settled_session`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettledSessionView {
    /// Local chain id.
    pub chain_id: i64,
    /// Which spend of that chain.
    pub spend_seq: u64,
    /// Credits spent into the session.
    pub s: u64,
    /// Credits refunded.
    pub t: u64,
    /// Sum of receipt charges.
    pub receipts_sum: u64,
    /// The classification.
    pub outcome: SettlementOutcome,
}

/// `POST /wallet/reconcile`: recovers unsettled sessions, then reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Sessions recovered by this call.
    pub recovered: u64,
    /// Every settled session.
    pub sessions: Vec<SettledSessionView>,
    /// Chains needing investigation.
    pub frozen_chains: u64,
}

/// `POST /wallet/close`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WalletCloseRequest {
    /// The conversation to close; all open sessions when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

/// The answer to `POST /wallet/close`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletCloseResponse {
    /// Sessions closed and settled.
    pub closed: u64,
    /// Of those, sessions the gate had already ended, such as by its idle
    /// timer, so the close got `404` and the refund came from recover.
    pub recovered: u64,
}

/// A manifest's facts as shown for approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSummary {
    /// Manifest version.
    pub version: u64,
    /// Image versions of its releases.
    pub image_versions: Vec<String>,
    /// Platforms of its releases, including ones this wallet can't verify.
    pub platforms: Vec<String>,
}

/// `GET /wallet/manifest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestStatus {
    /// The accepted manifest, if any.
    pub accepted: Option<ManifestSummary>,
    /// A fetched manifest waiting for approval, if any.
    pub pending: Option<ManifestSummary>,
}

/// What `POST /wallet/manifest` should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestAction {
    /// Fetch the gate's manifest over Tor into `pending`.
    Fetch,
    /// Accept the pending manifest.
    Approve,
}

/// `POST /wallet/manifest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestActionRequest {
    /// The action.
    pub action: ManifestAction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn balance_view_matches_the_docs_example() {
        let view = BalanceView {
            total_credits: 4_998_980,
            total_usd: "4.998980".into(),
            estimated_tokens_remaining: BTreeMap::from([(
                "fake-small".to_owned(),
                TokenEstimate { output_tokens_at_most: 1_666_326 },
            )]),
            chains: vec![ChainView { id: 1, state: ChainState::Ready, credits: 4_998_980, resolved: true }],
            orders_awaiting_payment: vec![],
            frozen_orders: 0,
            frozen_chains: 0,
            dust_credits: 0,
            experimental_platform: true,
            manifest_version: Some(7),
        };
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["chains"][0]["state"], "Ready");
        assert_eq!(value["estimated_tokens_remaining"]["fake-small"]["output_tokens_at_most"], 1_666_326);
        assert_eq!(serde_json::from_value::<BalanceView>(value).unwrap(), view);
    }

    #[test]
    fn outcomes_and_actions_are_snake_case() {
        for outcome in [
            SettlementOutcome::Exact,
            SettlementOutcome::FullRefund,
            SettlementOutcome::ReceiptGap,
            SettlementOutcome::Mismatch,
        ] {
            assert_eq!(serde_json::to_value(outcome).unwrap(), json!(outcome.as_str()));
        }
        let request: ManifestActionRequest = serde_json::from_value(json!({"action": "approve"})).unwrap();
        assert_eq!(request.action, ManifestAction::Approve);
    }

    #[test]
    fn resolved_states() {
        assert!(ChainState::Open.is_resolved());
        assert!(!ChainState::Recovering.is_resolved());
    }
}
