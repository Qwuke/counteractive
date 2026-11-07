//! The journal and the state machine over it, against a real issuer.

use ::manifest::Product;
use credit::{Holder, Moderator, SecretSeed, ServiceContext};
use proto::walletd::{ChainState, PurchaseState, SettlementOutcome};
use proto::{FinishReason, Receipt, SessionId};
use secrecy::ExposeSecret as _;

use super::*;
use crate::{chain, purchase};

const TIERS: [u64; 3] = [500_000, 50_000, 5_000];

struct Fixture {
    _dir: tempfile::TempDir,
    journal: Journal,
    moderator: Moderator,
    holder: Holder,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::create(dir.path()).unwrap();
    let context = ServiceContext::new("counteractive-dev", 1).unwrap();
    let moderator = Moderator::from_seed(&SecretSeed::generate().unwrap(), &context).unwrap();
    let holder = Holder::new(&context, &moderator.public_key()).unwrap();
    Fixture { _dir: dir, journal, moderator, holder }
}

/// Buys and collects one product, returning the new chain.
fn collected_chain(f: &mut Fixture, credits: u64) -> ChainId {
    let product = Product { id: "usd-5".into(), usd_cents: 500, credits };
    let id = purchase::begin(&mut f.journal, &f.holder, &product).unwrap();
    f.journal.set_order_id(id, "cs_test_1").unwrap();
    let row = f.journal.purchases_in(&[PurchaseState::AwaitingPayment]).unwrap().pop().unwrap();
    let response = f.moderator.issue(credits, row.issue_request.as_ref().unwrap()).unwrap();
    purchase::complete(&mut f.journal, &f.holder, &TIERS, &row, &response).unwrap().chain
}

fn receipt(turn: u32, charge: u64, remaining: u64) -> Receipt {
    Receipt {
        turn,
        model: "fake-small".into(),
        prompt_tokens: 10,
        completion_tokens: 20,
        charge,
        remaining,
        finish: FinishReason::Stop,
    }
}

#[test]
fn reopens_with_its_key_and_refuses_a_second_init() {
    let dir = tempfile::tempdir().unwrap();
    drop(Journal::create(dir.path()).unwrap());
    assert!(matches!(Journal::create(dir.path()), Err(WalletError::AlreadyInitialised)));
    Journal::open(dir.path()).unwrap();

    // The database is encrypted: another key can't read it.
    std::fs::remove_file(dir.path().join(KEY_FILE)).unwrap();
    JournalKey::create(&dir.path().join(KEY_FILE)).unwrap();
    Journal::open(dir.path()).unwrap_err();
}

#[test]
fn collect_creates_a_ready_chain_and_clears_issuance_state() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);
    let purchase = f.journal.purchases_in(&[PurchaseState::Collected]).unwrap().pop().unwrap();
    assert_eq!(purchase.chain_id, Some(chain));
    assert!(purchase.issue_state.is_none() && purchase.issue_request.is_none());
    let row = f.journal.chain(chain).unwrap();
    assert_eq!(row.state, ChainState::Ready);
    let credential = f.holder.credential_from_bytes(row.credential.unwrap().expose_secret()).unwrap();
    assert_eq!(credential.balance(), 5_000_000);
}

#[test]
fn a_bad_issue_response_freezes_the_purchase() {
    let mut f = fixture();
    let product = Product { id: "usd-1".into(), usd_cents: 100, credits: 1_000_000 };
    let id = purchase::begin(&mut f.journal, &f.holder, &product).unwrap();
    f.journal.set_order_id(id, "cs_test_2").unwrap();
    let row = f.journal.purchases_in(&[PurchaseState::AwaitingPayment]).unwrap().pop().unwrap();
    purchase::complete(&mut f.journal, &f.holder, &TIERS, &row, b"garbage").unwrap_err();
    assert_eq!(f.journal.purchases_in(&[PurchaseState::Frozen]).unwrap().len(), 1);
    assert!(f.journal.chains().unwrap().is_empty());
}

#[test]
fn a_session_settles_exactly_and_the_balance_follows() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);

    let spend = chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    assert_eq!((spend.chain, spend.spend_seq, spend.budget), (chain, 1, 500_000));
    // Spending: the credential is gone, the stored spend is the one to resend.
    let row = f.journal.chain(chain).unwrap();
    assert_eq!(row.state, ChainState::Spending);
    assert!(row.credential.is_none());
    assert_eq!(chain::stored_spend(&row).unwrap().message.expose(), spend.message.expose());
    // A second spend from the same chain is refused while this one is unsettled.
    assert!(chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().is_none());

    f.journal.mark_open(chain, 1, &SessionId::from_bytes([7; 16])).unwrap();
    f.journal.insert_receipt(chain, 1, &receipt(1, 61, 499_939)).unwrap();
    f.journal.insert_receipt(chain, 1, &receipt(2, 100, 499_839)).unwrap();
    f.journal.set_session_state(chain, 1, ChainState::Closing).unwrap();

    let refund = f.moderator.refund(spend.message.expose(), 499_839).unwrap();
    let settled = chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 1, &refund).unwrap();
    assert_eq!(settled.outcome, SettlementOutcome::Exact);
    assert_eq!((settled.s, settled.t, settled.receipts_sum), (500_000, 499_839, 161));
    assert_eq!(f.journal.settled_sessions().unwrap(), vec![settled]);

    let row = f.journal.chain(chain).unwrap();
    assert_eq!(row.state, ChainState::Ready);
    assert!(row.spend_state.is_none() && row.spend_message.is_none());
    let credential = f.holder.credential_from_bytes(row.credential.unwrap().expose_secret()).unwrap();
    assert_eq!(credential.balance(), 5_000_000 - 161);
}

#[test]
fn a_full_refund_after_receipts_is_reported_as_such() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);
    let spend = chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    f.journal.mark_open(chain, 1, &SessionId::from_bytes([1; 16])).unwrap();
    f.journal.insert_receipt(chain, 1, &receipt(1, 61, 499_939)).unwrap();
    f.journal.set_session_state(chain, 1, ChainState::Recovering).unwrap();
    let refund = f.moderator.refund(spend.message.expose(), 500_000).unwrap();
    let settled = chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 1, &refund).unwrap();
    assert_eq!(settled.outcome, SettlementOutcome::FullRefund);
}

#[test]
fn a_refund_that_fails_to_verify_freezes_the_chain() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);
    chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 1, b"not a refund").unwrap_err();
    let row = f.journal.chain(chain).unwrap();
    assert_eq!(row.state, ChainState::Frozen);
    assert!(row.spend_state.is_some(), "the state is kept for investigation");
}

/// Opens a session on the first Ready chain, charges all of it but `t`
/// with one receipt, and settles. Returns `s` and the chain's new state.
fn spend_and_settle(f: &mut Fixture, chain: ChainId, t: u64) -> (u64, ChainState) {
    let spend = chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    f.journal.insert_receipt(chain, spend.spend_seq, &receipt(1, spend.budget - t, t)).unwrap();
    let refund = f.moderator.refund(spend.message.expose(), t).unwrap();
    let settled = chain::settle(&mut f.journal, &f.holder, &TIERS, chain, spend.spend_seq, &refund).unwrap();
    assert_eq!(settled.outcome, SettlementOutcome::Exact);
    (spend.budget, f.journal.chain(chain).unwrap().state)
}

#[test]
fn sizing_follows_the_ladder_and_a_low_balance_is_dust() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 60_000);
    // No 500,000 session fits, so a caller needing more than 50,000 gets none.
    assert!(chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 50_001).unwrap().is_none());
    assert_eq!(spend_and_settle(&mut f, chain, 0), (50_000, ChainState::Ready)); // 10,000 left
    assert_eq!(spend_and_settle(&mut f, chain, 0), (5_000, ChainState::Ready)); // 5,000 left
    assert_eq!(spend_and_settle(&mut f, chain, 1_000), (5_000, ChainState::Dust)); // 1,000 left
    assert!(chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().is_none());
}

#[test]
fn guarded_transitions_refuse_the_wrong_state() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);
    assert!(matches!(
        f.journal.mark_open(chain, 1, &SessionId::from_bytes([0; 16])),
        Err(WalletError::WrongState { .. })
    ));
    assert!(f.journal.set_session_state(chain, 1, ChainState::Closing).is_err());
    assert!(f.journal.add_unknown_turn(chain, 1).is_err());
    assert!(f.journal.freeze_chain(chain, 1).is_err(), "a Ready chain is never frozen");
}

/// A flow still holding spend 1 of a chain, after that spend settled and
/// spend 2 began, as two drives of one orphan once could: every step it
/// takes is refused, and spend 2 is untouched and settles normally.
#[test]
fn a_stale_spend_cannot_touch_the_next_one() {
    let mut f = fixture();
    let chain = collected_chain(&mut f, 5_000_000);
    let first = chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    f.journal.mark_open(chain, 1, &SessionId::from_bytes([1; 16])).unwrap();
    f.journal.set_session_state(chain, 1, ChainState::Closing).unwrap();
    let first_refund = f.moderator.refund(first.message.expose(), 500_000).unwrap();
    chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 1, &first_refund).unwrap();
    let second = chain::prepare_spend(&mut f.journal, &f.holder, &TIERS, 500_000, 0).unwrap().unwrap();
    assert_eq!(second.spend_seq, 2);

    // The stale flow, acting on what the gate says about spend 1.
    let stale = |result: Result<(), WalletError>| matches!(result, Err(WalletError::WrongState { .. }));
    assert!(stale(f.journal.set_session_state(chain, 1, ChainState::Recovering)));
    assert!(stale(f.journal.mark_open(chain, 1, &SessionId::from_bytes([1; 16]))));
    assert!(stale(f.journal.add_unknown_turn(chain, 1)));
    assert!(stale(chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 1, &first_refund).map(drop)));
    assert!(stale(chain::freeze(&mut f.journal, chain, 1, "stale")));
    let row = f.journal.chain(chain).unwrap();
    assert_eq!((row.state, row.spend_seq, row.unknown_turns), (ChainState::Spending, 2, 0));

    // Spend 2 carries on as if nothing happened.
    f.journal.mark_open(chain, 2, &SessionId::from_bytes([2; 16])).unwrap();
    f.journal.insert_receipt(chain, 2, &receipt(1, 100, 499_900)).unwrap();
    let refund = f.moderator.refund(second.message.expose(), 499_900).unwrap();
    let settled = chain::settle(&mut f.journal, &f.holder, &TIERS, chain, 2, &refund).unwrap();
    assert_eq!(settled.outcome, SettlementOutcome::Exact);
    let credential = f.journal.chain(chain).unwrap().credential.unwrap();
    assert_eq!(f.holder.credential_from_bytes(credential.expose_secret()).unwrap().balance(), 5_000_000 - 100);
}
