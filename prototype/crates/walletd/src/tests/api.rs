//! The local API (SPEC section 7, docs 04 section 3) and the chat rows of
//! the failure table (docs 04 section 8), through walletd's router.

use axum::body::Body;
use gate_conn::ApiRequest;
use proto::gate::{CloseResponse, OpenSessionRequest, OpenSessionResponse};
use proto::walletd::{ChainState, RECOVERED_HEADER, ROLLOVER_HEADER, SettlementOutcome};
use serde_json::json;
use wallet_core::chain;

use super::testkit::{HOST, Policy, TestGate, TestWallet, USD_5};

/// The fake engine's charge for a one-message prompt of `words` words in
/// a session with room to spare: `4 + words` prompt tokens and
/// `16 + words % 17` completion tokens (SPEC section 8), at the model's rates.
fn charge(words: u64, input_rate: u64, output_rate: u64) -> u64 {
    (4 + words) * input_rate + (16 + words % 17) * output_rate
}

/// Every route, the page and `/paid` included, answers only to a `Host`
/// naming this walletd, so a page on another origin that rebinds its DNS
/// name to 127.0.0.1 can't reach the API.
#[tokio::test]
async fn the_host_header_must_name_this_walletd() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[]).await;
    for path in ["/", "/paid", "/wallet/balance", "/v1/models"] {
        for host in [Some("evil.example"), Some("127.0.0.1:7778"), None] {
            let mut request = wallet.request("GET", path);
            // The builder already names the right host; replace or drop it.
            request.headers_mut().unwrap().remove(hyper::header::HOST);
            if let Some(host) = host {
                request = request.header(hyper::header::HOST, host);
            }
            let (status, _, body) = wallet.send(request.body(Body::empty()).unwrap()).await;
            assert_eq!((status, body["error"].as_str()), (403, Some("bad_host")), "{path} with Host {host:?}");
        }
    }
    let (status, _) = wallet.get("/paid").await;
    assert_eq!(status, 200, "the right host is served");
}

/// The bearer token guards every route but the page and `/paid`: missing,
/// wrong or malformed, it gets `401` before any handler runs.
#[tokio::test]
async fn every_route_but_the_page_needs_the_bearer_token() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let wrong = format!("Bearer {}", "0".repeat(64));
    for (method, path) in [("GET", "/wallet/balance"), ("GET", "/v1/models"), ("POST", "/wallet/reconcile")] {
        for authorization in [None, Some(wrong.as_str()), Some("Bearer"), Some("Basic dXNlcjpwYXNz")] {
            let mut request = hyper::Request::builder().method(method).uri(path).header(hyper::header::HOST, HOST);
            if let Some(value) = authorization {
                request = request.header(hyper::header::AUTHORIZATION, value);
            }
            let (status, _, body) = wallet.send(request.body(Body::empty()).unwrap()).await;
            assert_eq!(
                (status, body["error"].as_str()),
                (401, Some("unauthorized")),
                "{method} {path} with {authorization:?}"
            );
        }
    }
    for path in ["/", "/paid"] {
        let request = hyper::Request::get(path).header(hyper::header::HOST, HOST).body(Body::empty()).unwrap();
        assert_eq!(wallet.send(request).await.0, 200, "{path} needs no token");
    }
    // Nothing was spent by the refused requests.
    assert_eq!(wallet.chain(1).state, ChainState::Ready);
}

/// One conversation from first turn to reconcile: the receipt carries the
/// fake engine's counts at the release's rates, the balance is exact after
/// close, and reconcile reports the session `exact`. A second close of the
/// same conversation finds nothing to close.
#[tokio::test]
async fn a_conversation_is_metered_closed_and_reconciled_exactly() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;

    let (status, models) = wallet.get("/v1/models").await;
    assert_eq!(status, 200);
    let ids: Vec<_> = models["data"].as_array().unwrap().iter().map(|model| model["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["fake-large", "fake-small"]);

    let first = wallet.chat_ok("c", "fake-small", "hello there").await;
    assert_eq!((first.turn, first.charge, first.remaining), (1, charge(2, 1, 3), 500_000 - charge(2, 1, 3)));
    let second = wallet.chat_ok("c", "fake-large", "and again").await;
    assert_eq!((second.turn, second.charge), (2, charge(2, 4, 12)), "the same session takes the next turn");
    assert_eq!(second.remaining, first.remaining - second.charge);
    let open = wallet.chain(1);
    assert_eq!((open.state, open.spend_seq), (ChainState::Open, 1));

    let (status, closed) = wallet.post("/wallet/close", json!({"conversation_id": "c"})).await;
    assert_eq!((status, closed), (200, json!({"closed": 1, "recovered": 0})));
    let (_, again) = wallet.post("/wallet/close", json!({"conversation_id": "c"})).await;
    assert_eq!(again, json!({"closed": 0, "recovered": 0}), "the conversation is gone once closed");

    let view = wallet.balance().await;
    assert_eq!(view.total_credits, USD_5 - first.charge - second.charge);
    assert!(view.chains.iter().all(|chain| chain.resolved && chain.state == ChainState::Ready), "{view:?}");
    let report = wallet.reconcile().await;
    assert_eq!((report.recovered, report.frozen_chains, report.sessions.len()), (0, 0, 1));
    let session = &report.sessions[0];
    assert_eq!(session.outcome, SettlementOutcome::Exact);
    assert_eq!((session.s, session.t, session.receipts_sum), (500_000, second.remaining, 500_000 - second.remaining));
}

/// A model no accepted release serves is refused before walletd spends
/// anything, so the chain stays `Ready` on its first spend.
#[tokio::test]
async fn an_unknown_model_is_refused_before_any_spend() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let (status, _, body) = wallet.chat("c", "no-such-model", "hello").await;
    assert_eq!((status, body["error"].as_str()), (400, Some("model")));
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq), (ChainState::Ready, 0));
}

/// `402 session_exhausted` closes the session and rolls the turn over to
/// one new session, which the response announces. A prompt larger than
/// any session this wallet can open gets `402 prompt_exceeds_budget`
/// after at most one attempt: the session it was refused in is settled,
/// and no further spend is made for it.
#[tokio::test]
async fn an_exhausted_session_rolls_over_once() {
    // Sessions of 5,000 credits: a 1,000-word prompt on fake-large costs
    // 1,004 x 4 + 30 x 12 = 4,376, and the next needs 4,028 of the 624 left.
    let gate = TestGate::start(Policy { maximum_spend: 5_000, ..Policy::default() }).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let prompt = "w ".repeat(1_000);
    let first = wallet.chat_ok("c", "fake-large", &prompt).await;
    assert_eq!((first.charge, first.remaining), (4_376, 624));

    let (status, headers, body) = wallet.chat("c", "fake-large", &prompt).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get(ROLLOVER_HEADER).unwrap(), "session_exhausted");
    let rolled: proto::Receipt = serde_json::from_value(body["receipt"].clone()).unwrap();
    assert_eq!((rolled.turn, rolled.remaining), (1, 624), "the turn ran as the first of a new session");
    assert_eq!(wallet.chain(1).spend_seq, 2, "exactly one new spend");

    // 2,004 x 4 + 12 = 8,028 credits needed, more than any 5,000 session.
    let (status, _, body) = wallet.chat("c", "fake-large", &"w ".repeat(2_000)).await;
    assert_eq!(
        (status, body["error"].as_str(), body["required"].as_u64()),
        (402, Some("prompt_exceeds_budget"), Some(8_028))
    );
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq), (ChainState::Ready, 2), "settled, and nothing more spent");

    let report = wallet.reconcile().await;
    assert_eq!(report.sessions.len(), 2);
    for session in &report.sessions {
        assert_eq!((session.outcome, session.s, session.t), (SettlementOutcome::Exact, 5_000, 624), "{session:?}");
    }
    assert_eq!(wallet.balance().await.total_credits, USD_5 - 2 * 4_376);
}

/// The gate forgets a live session when it stops (without draining, a
/// stop is a crash). The conversation's next turn gets `404`, walletd
/// recovers the spend (`t = s`, a full refund, so the lost session costs
/// nothing), says so in a header, and runs the turn in a new session.
#[tokio::test]
async fn a_session_the_gate_lost_is_recovered_and_the_turn_continues() {
    let mut gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let lost = wallet.chat_ok("c", "fake-small", "before the crash").await;
    gate.restart().await;

    let (status, headers, body) = wallet.chat("c", "fake-small", "after the crash").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get(RECOVERED_HEADER).unwrap(), "full_refund");
    assert!(headers.get(ROLLOVER_HEADER).is_none());
    let next: proto::Receipt = serde_json::from_value(body["receipt"].clone()).unwrap();
    assert_eq!((next.turn, next.charge), (1, charge(3, 1, 3)));
    let (_, closed) = wallet.post("/wallet/close", json!({"conversation_id": "c"})).await;
    assert_eq!(closed["closed"], 1);

    let report = wallet.reconcile().await;
    let [crashed, after] = &report.sessions[..] else { panic!("two sessions: {:?}", report.sessions) };
    assert_eq!(
        (crashed.outcome, crashed.t, crashed.receipts_sum),
        (SettlementOutcome::FullRefund, 500_000, lost.charge)
    );
    assert_eq!(after.outcome, SettlementOutcome::Exact);
    assert_eq!(wallet.balance().await.total_credits, USD_5 - next.charge, "only the turn after the crash is paid");
}

/// A spend walletd journalled, whose open reached the gate and whose
/// session the gate then settled without walletd hearing of it (a crash
/// after the open). Reconcile resends the stored bytes, gets
/// `409 use_recover`, recovers the gate's refund, and the chain is whole.
#[tokio::test]
async fn a_spend_the_gate_settled_unseen_is_recovered_through_use_recover() {
    let gate = TestGate::start(Policy::default()).await;
    let wallet = TestWallet::unix(&gate, &[USD_5]).await;
    let context = wallet.walletd.context().unwrap();
    let tiers = wallet.walletd.pins.tier_ladder.clone();
    let spend = chain::prepare_spend(&mut wallet.walletd.journal(), &context.holder, &tiers, 500_000, 0)
        .unwrap()
        .expect("a Ready chain");

    let mut client = gate.client().await;
    let open = ApiRequest::post("/v1/sessions").json(&OpenSessionRequest { spend: spend.message.clone() }).unwrap();
    let OpenSessionResponse::Open { session_id, .. } = client.send_json(open).await.unwrap() else {
        panic!("the journalled spend should open a session");
    };
    let _: CloseResponse =
        client.send_json(ApiRequest::post(format!("/v1/sessions/{}/close", session_id.to_hex()))).await.unwrap();
    assert_eq!(wallet.chain(1).state, ChainState::Spending, "walletd never saw the open");

    let report = wallet.reconcile().await;
    assert_eq!(report.recovered, 1);
    let [session] = &report.sessions[..] else { panic!("one session: {:?}", report.sessions) };
    assert_eq!(
        (session.outcome, session.s, session.t, session.receipts_sum),
        (SettlementOutcome::Exact, 500_000, 500_000, 0)
    );
    let chain = wallet.chain(1);
    assert_eq!((chain.state, chain.spend_seq), (ChainState::Ready, 1), "settled with the journalled spend, no new one");
    assert_eq!(wallet.balance().await.total_credits, USD_5);
}
