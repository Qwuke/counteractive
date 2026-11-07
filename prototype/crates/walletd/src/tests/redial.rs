//! The redial after a connection fails in transit (the tier 2 flake: a
//! Tor circuit that stalled during the TLS handshake). walletd dials once
//! more in a fresh scope, and nothing it sends goes out twice: the failed
//! connection carried nothing, and a journalled spend is resent as the
//! same bytes on the new one.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gate_conn::{ApiRequest, BoxedIo, DialError, DialScope, Dialer, HANDSHAKE_TIMEOUT, Target, UnixDialer};
use proto::gate::{RecoverRequest, RecoverResponse};
use proto::walletd::{ChainState, SettlementOutcome};
use tokio::io::DuplexStream;
use wallet_core::chain;

use super::testkit::{Policy, TestGate, TestWallet, USD_5};

/// Dials the gate's socket, except that the first `stalls` dials return a
/// stream on which nothing ever arrives, as over a stalled circuit, and
/// that it can refuse every dial instead. It counts its dials.
struct FlakyDialer {
    stalls: usize,
    refuse: bool,
    dials: Arc<AtomicUsize>,
    /// The far ends of the stalled streams, kept open so that they stall
    /// rather than close.
    stalled: std::sync::Mutex<Vec<DuplexStream>>,
}

impl FlakyDialer {
    /// The dialer, and its dial count.
    fn boxed(stalls: usize, refuse: bool) -> (Box<dyn Dialer>, Arc<AtomicUsize>) {
        let dials = Arc::new(AtomicUsize::new(0));
        let dialer = Self { stalls, refuse, dials: Arc::clone(&dials), stalled: std::sync::Mutex::default() };
        (Box::new(dialer), dials)
    }
}

#[async_trait]
impl Dialer for FlakyDialer {
    async fn dial(&self, target: &Target, scope: &DialScope) -> Result<BoxedIo, DialError> {
        let dial = self.dials.fetch_add(1, Ordering::SeqCst);
        if self.refuse {
            return Err(DialError::Io(std::io::ErrorKind::ConnectionRefused.into()));
        }
        if dial < self.stalls {
            let (near, far) = tokio::io::duplex(64 * 1024);
            self.stalled.lock().unwrap().push(far);
            return Ok(Box::new(near));
        }
        UnixDialer.dial(target, scope).await
    }
}

/// The flake itself: the first connection's TLS handshake gets no answer.
/// After the handshake timeout walletd redials once, opens the session
/// on the new connection and runs the turn, which the client sees as an
/// ordinary, slower answer. One spend, no unknown turn, and the session
/// settles exactly.
#[tokio::test]
async fn a_turn_whose_first_connection_stalls_is_redialled_once() {
    let gate = TestGate::start(Policy::default()).await;
    let (dialer, dials) = FlakyDialer::boxed(1, false);
    let wallet = TestWallet::new(&gate, &[USD_5], dialer).await;

    let started = tokio::time::Instant::now();
    let receipt = wallet.chat_ok("c", "fake-small", "hello there").await;
    assert!(started.elapsed() >= HANDSHAKE_TIMEOUT, "the first connection was waited out");
    assert_eq!(dials.load(Ordering::SeqCst), 2, "one stalled dial, one redial");
    assert_eq!(receipt.turn, 1);
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq, chain.unknown_turns), (ChainState::Open, 1, 0));

    let (_, closed) = wallet.post("/wallet/close", serde_json::json!({"conversation_id": "c"})).await;
    assert_eq!(closed["closed"], 1);
    let report = wallet.reconcile().await;
    let [session] = &report.sessions[..] else { panic!("one session: {:?}", report.sessions) };
    assert_eq!((session.outcome, session.receipts_sum), (SettlementOutcome::Exact, receipt.charge));
}

/// A spend journalled before a crash, settled by reconcile through a
/// connection that first stalls. The redial resends the journalled bytes
/// themselves: recovering with those exact bytes afterwards finds their
/// session settled, and the chain made no second spend.
#[tokio::test]
async fn reconcile_resends_the_journalled_spend_after_a_stall() {
    let gate = TestGate::start(Policy::default()).await;
    let (dialer, dials) = FlakyDialer::boxed(1, false);
    let wallet = TestWallet::new(&gate, &[USD_5], dialer).await;
    let context = wallet.walletd.context().unwrap();
    let tiers = wallet.walletd.pins.tier_ladder.clone();
    let spend = chain::prepare_spend(&mut wallet.walletd.journal(), &context.holder, &tiers, 500_000, 0)
        .unwrap()
        .expect("a Ready chain");

    let report = wallet.reconcile().await;
    assert_eq!((report.recovered, dials.load(Ordering::SeqCst)), (1, 2));
    let [session] = &report.sessions[..] else { panic!("one session: {:?}", report.sessions) };
    assert_eq!((session.outcome, session.s, session.t), (SettlementOutcome::Exact, 500_000, 500_000));
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq), (ChainState::Ready, 1), "settled with the journalled spend only");

    let recover = ApiRequest::post("/v1/sessions/recover").json(&RecoverRequest { spend: spend.message }).unwrap();
    let answer: RecoverResponse = gate.client().await.send_json(recover).await.unwrap();
    assert!(matches!(answer, RecoverResponse::Settled { .. }), "the gate settled these very bytes: {answer:?}");
    assert_eq!(wallet.balance().await.total_credits, USD_5);
}

/// The redial happens once, not in a loop: a gate that can't be reached
/// at all is dialled twice, the client gets `502 gate_unreachable`, and
/// since the connection comes before the spend, nothing is spent.
#[tokio::test]
async fn an_unreachable_gate_is_dialled_twice_and_nothing_is_spent() {
    let gate = TestGate::start(Policy::default()).await;
    let (dialer, dials) = FlakyDialer::boxed(0, true);
    let wallet = TestWallet::new(&gate, &[USD_5], dialer).await;

    let (status, _, body) = wallet.chat("c", "fake-small", "hello").await;
    assert_eq!((status, body["error"].as_str()), (502, Some("gate_unreachable")));
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq), (ChainState::Ready, 0));
}
