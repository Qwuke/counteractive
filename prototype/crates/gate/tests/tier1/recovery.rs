//! One refund per nullifier: replays, a second proof, a crash, and races.

use proto::ErrorCode;
use proto::gate::{GateState, RecoverResponse};
use secrecy::ExposeSecret as _;

use crate::harness::{Policy, USD_1, USD_5, World, chat_request, code, settled, stop, user};

#[tokio::test(flavor = "multi_thread")]
async fn a_settled_spend_is_recovered_never_reopened() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, 2_000).unwrap();
    assert_eq!(code(client.recover(&spend).await), (404, ErrorCode::NoRow), "never opened");

    let (id, _) = client.open_live(&spend).await;
    client.chat(id, &chat_request("fake-small", vec![user("hello")], None)).await.unwrap();
    let refund = client.close(id).await.unwrap().refund.into_vec();

    assert_eq!(code(client.open(&spend).await), (409, ErrorCode::UseRecover));
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund);
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund, "identical on every call");

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_proof_of_the_same_credential_is_spent() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let credential = wallet.buy(&gate, USD_5).await;
    let copy = wallet.holder.credential_from_bytes(credential.to_bytes().expose_secret()).unwrap();
    let (_, spend) = wallet.holder.prove_spend(credential, 3_000).unwrap();
    let (_, double) = wallet.holder.prove_spend(copy, 3_000).unwrap();
    assert_ne!(spend, double, "proofs are randomised");

    // Live: the slot belongs to the first proof.
    let (id, _) = client.open_live(&spend).await;
    assert_eq!(code(client.open(&double).await), (409, ErrorCode::Spent));
    assert_eq!(code(client.recover(&double).await), (409, ErrorCode::Spent));

    // Settled: only the first proof's bytes open the stored refund.
    let refund = client.close(id).await.unwrap().refund.into_vec();
    assert_eq!(code(client.open(&double).await), (409, ErrorCode::Spent));
    assert_eq!(code(client.recover(&double).await), (409, ErrorCode::Spent));
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund);

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_crash_recover_refunds_the_whole_spend() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let s = 7_777;
    let (spend_state, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, s).unwrap();
    let (id, _) = client.open_live(&spend).await;
    let turn = client.chat(id, &chat_request("fake-small", vec![user("hello")], None)).await.unwrap();
    assert!(turn.receipt.charge > 0);

    // Stopping without draining loses the session, as a crash would: its
    // row stays Pending and its debits are gone.
    drop(client);
    stop(gate).await;

    let gate = world.start().await;
    let mut client = world.attested(&gate).await;
    assert_eq!(client.unlock(world.k_token()).await.unwrap().state, GateState::Ready);
    assert_eq!(code(client.open(&spend).await), (409, ErrorCode::UseRecover));
    let refund = settled(client.recover(&spend).await.unwrap());
    assert_eq!(wallet.holder.refund_amount(&refund).unwrap(), s, "t = s");
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund);
    assert_eq!(code(client.open(&spend).await), (409, ErrorCode::UseRecover));
    assert_eq!(wallet.holder.finalize_refund(spend_state, &refund).unwrap().balance(), USD_5);

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_racing_recover_never_settles_a_live_session() {
    let world = World::new(Policy::default());
    let (gate, mut opener, wallet) = world.ready().await;
    let mut recoverer = world.attested(&gate).await;

    for _ in 0..8 {
        let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_1).await, 1_000).unwrap();
        let (opened, recovered) = tokio::join!(opener.open_live(&spend), recoverer.recover(&spend));
        let (id, _) = opened;
        match recovered {
            Ok(RecoverResponse::Live { session_id }) => assert_eq!(session_id, id),
            Ok(other) => panic!("recover settled a session being opened: {other:?}"),
            Err(error) => assert!(
                matches!(code::<()>(Err(error)), (404, ErrorCode::NoRow) | (503, ErrorCode::Retry)),
                "recover before or during the open"
            ),
        }
        assert_eq!(recoverer.recover(&spend).await.unwrap(), RecoverResponse::Live { session_id: id });
        let refund = opener.close(id).await.unwrap().refund;
        assert_eq!(wallet.holder.refund_amount(refund.as_bytes()).unwrap(), 1_000);
    }

    drop((opener, recoverer));
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn racing_closes_produce_one_refund() {
    let world = World::new(Policy::default());
    let (gate, mut first, wallet) = world.ready().await;
    let mut second = world.attested(&gate).await;

    for _ in 0..4 {
        let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_1).await, 1_000).unwrap();
        let (id, _) = first.open_live(&spend).await;
        let turn = first.chat(id, &chat_request("fake-small", vec![user("hello")], None)).await.unwrap();

        let (a, b) = tokio::join!(first.close(id), second.close(id));
        let refund = match (a, b) {
            (Ok(won), lost) | (lost, Ok(won)) => {
                assert!(
                    matches!(code(lost), (503, ErrorCode::Retry) | (404, ErrorCode::NotFound)),
                    "the other close sees closing under way, or nothing left"
                );
                won.refund.into_vec()
            }
            (a, b) => panic!("neither close succeeded: {a:?}, {b:?}"),
        };
        assert_eq!(wallet.holder.refund_amount(&refund).unwrap(), 1_000 - turn.receipt.charge);
        assert_eq!(settled(second.recover(&spend).await.unwrap()), refund);
    }

    drop((first, second));
    stop(gate).await;
}
