//! walletd's own idle close (SPEC section 7: walletd closes a session idle
//! for 5 minutes), run with an idle time short enough for a test.

use std::sync::Arc;
use std::time::Duration;

use proto::walletd::{ChainState, SettlementOutcome};

use super::testkit::{Policy, TestGate, TestWallet, USD_5, eventually};
use crate::conversation::close_idle;

/// The idle time the tests run [`close_idle`] with.
const IDLE: Duration = Duration::from_millis(300);

/// A conversation left alone for the idle time has its session closed by
/// walletd itself: the refund is finalised into the chain, which is
/// `Ready` again, the session settles exactly, the balance is the full
/// purchase less the one turn, and the conversation is forgotten.
#[tokio::test]
async fn an_idle_session_is_closed_and_its_refund_finalised() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let receipt = wallet.chat_ok("c", "fake-small", "hello there").await;
    assert_eq!(wallet.chain(1).state, ChainState::Open);

    let idle = tokio::spawn(close_idle(Arc::clone(&wallet.walletd), IDLE));
    eventually("walletd closes the idle session", || {
        wallet.chain(1).state == ChainState::Ready && wallet.walletd.conversations.get("c").is_none()
    })
    .await;
    idle.abort();

    let report = wallet.reconcile().await;
    assert_eq!(report.recovered, 0, "the close settled it; nothing was left for reconcile");
    let [session] = &report.sessions[..] else { panic!("one session: {:?}", report.sessions) };
    assert_eq!((session.outcome, session.s, session.t), (SettlementOutcome::Exact, 500_000, receipt.remaining));
    let view = wallet.balance().await;
    assert_eq!(view.total_credits, USD_5 - receipt.charge);
    assert!(view.chains.iter().all(|chain| chain.resolved), "{view:?}");
}

/// The idle time [`a_conversation_in_the_middle_of_a_turn_is_not_closed`]
/// runs with: long enough, with its 250 ms check interval, to tell a
/// session closed on time from one closed early without a tight race.
const TURN_IDLE: Duration = Duration::from_secs(1);

/// A turn that outlasts the idle time is not cut off: the conversation
/// is busy for the whole turn, however long ago it was last used, so its
/// session stays open and the turn ends with its full answer and receipt.
/// The idle time then counts from the turn's end, and only after that is
/// the session closed.
#[tokio::test]
async fn a_conversation_in_the_middle_of_a_turn_is_not_closed() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let idle = tokio::spawn(close_idle(Arc::clone(&wallet.walletd), TURN_IDLE));

    // The engine stalls 3 s before its first token, three idle times.
    let prompt = "[[stall:3000]] a slow answer";
    let mid_turn = async {
        eventually("the turn's session is open", || wallet.chain(1).state == ChainState::Open).await;
        tokio::time::sleep(2 * TURN_IDLE).await;
        let chain = wallet.chain(1);
        assert_eq!((chain.state, chain.spend_seq), (ChainState::Open, 1), "still in the turn's session");
        assert!(wallet.walletd.conversations.get("c").is_some(), "the busy conversation is kept");
    };
    let (receipt, ()) = tokio::join!(wallet.chat_ok("c", "fake-small", prompt), mid_turn);
    // 4 words, the hook among them: 8 prompt tokens, and all 16 + 4
    // completion tokens the fake engine gives such a prompt.
    assert_eq!((receipt.turn, receipt.prompt_tokens, receipt.completion_tokens), (1, 8, 20));
    assert_eq!(receipt.charge, 8 + 20 * 3);
    // The turn's end counts as use, so the session outlives it by the idle
    // time: two checks later it is still open. Counting from the turn's
    // start instead, or not waiting at all, would have closed it by now.
    tokio::time::sleep(TURN_IDLE / 2).await;
    assert!(wallet.walletd.conversations.get("c").is_some(), "closed before the idle time after its turn");
    assert_eq!(wallet.chain(1).state, ChainState::Open);

    eventually("walletd closes the session once idle", || {
        wallet.chain(1).state == ChainState::Ready && wallet.walletd.conversations.get("c").is_none()
    })
    .await;
    idle.abort();
    let report = wallet.reconcile().await;
    let [session] = &report.sessions[..] else { panic!("one session: {:?}", report.sessions) };
    assert_eq!((session.outcome, session.receipts_sum), (SettlementOutcome::Exact, receipt.charge));
}
