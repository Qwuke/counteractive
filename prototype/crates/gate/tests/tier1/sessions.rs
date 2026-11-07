//! Open, turns, close: the hold, the one debit rule, the receipt, the
//! refused turns, and the limits that close a session on their own.

use std::time::Instant;

use fake_engine::script::{self, Faults};
use proto::gate::{OpenSessionResponse, RecoverResponse};
use proto::{ErrorCode, FinishReason};

use crate::harness::{
    MAXIMUM_SPEND, Policy, TIMEOUT, USD_5, World, assistant, chat_request, code, error_body, settled, stop, user,
};

/// The fake model's text for `n` output tokens.
fn fake_text(n: u32) -> String {
    (0..n).map(script::token_text).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn two_turns_then_close_refund_exactly_what_is_left() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let credential = wallet.buy(&gate, USD_5).await;
    let s = 10_000;
    let (spend_state, spend) = wallet.holder.prove_spend(credential, s).unwrap();
    let (id, budget) = client.open_live(&spend).await;
    assert_eq!(budget, s);
    assert_eq!(client.open_live(&spend).await, (id, s), "the same bytes get the same session");
    assert_eq!(client.recover(&spend).await.unwrap(), RecoverResponse::Live { session_id: id });

    // fake-small charges 1 per prompt token and 3 per output token. One
    // message of 3 words is 4 + 3 = 7 tokens; the answer is 16 + 3 % 17 = 19.
    let first_messages = vec![user("hello there friend")];
    let first = client.chat(id, &chat_request("fake-small", first_messages.clone(), None)).await.unwrap();
    assert_eq!(script::prompt_tokens(&first_messages), 7);
    assert_eq!(first.text, fake_text(19));
    let receipt = &first.receipt;
    assert_eq!((receipt.turn, receipt.model.as_str()), (1, "fake-small"));
    assert_eq!((receipt.prompt_tokens, receipt.completion_tokens, receipt.finish), (7, 19, FinishReason::Stop));
    assert_eq!((receipt.charge, receipt.remaining), (7 + 19 * 3, s - 64));

    // fake-large charges 4 and 12. The client's limit of 5 binds, so the
    // engine stops by length, and the debit is input plus 5 outputs.
    let second_messages = vec![user("hello there friend"), assistant(&first.text), user("and once more please")];
    let prompt = script::prompt_tokens(&second_messages);
    assert_eq!(prompt, (4 + 3) + (4 + 19) + (4 + 4));
    assert_eq!(script::output_tokens(&second_messages, 5, &Faults::default()), 5);
    let second = client.chat(id, &chat_request("fake-large", second_messages, Some(5))).await.unwrap();
    assert_eq!(second.text, fake_text(5));
    let receipt = &second.receipt;
    assert_eq!((receipt.turn, receipt.model.as_str()), (2, "fake-large"));
    assert_eq!((receipt.prompt_tokens, receipt.completion_tokens, receipt.finish), (38, 5, FinishReason::Length));
    let second_charge = 38 * 4 + 5 * 12;
    assert_eq!((receipt.charge, receipt.remaining), (second_charge, s - 64 - second_charge));

    // t = s − debits, and the wallet's balance is its remainder plus t.
    let t = s - 64 - second_charge;
    let refund = client.close(id).await.unwrap().refund;
    assert_eq!(wallet.holder.refund_amount(refund.as_bytes()).unwrap(), t);
    let credential = wallet.holder.finalize_refund(spend_state, refund.as_bytes()).unwrap();
    assert_eq!(credential.balance(), USD_5 - s + t);

    // The session is gone; recover hands back the same refund.
    assert_eq!(code(client.close(id).await), (404, ErrorCode::NotFound));
    assert_eq!(
        code(client.chat(id, &chat_request("fake-small", vec![user("hi")], None)).await),
        (404, ErrorCode::NotFound)
    );
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund.into_vec());

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refused_turns_charge_nothing() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let s = 100;
    let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, s).unwrap();
    let (id, _) = client.open_live(&spend).await;

    // 30 words on fake-large: 34 prompt tokens, so one output token needs
    // 34 × 4 + 12 = 148 > s. No session of this size could take it.
    let words = vec!["word"; 30].join(" ");
    let refused = error_body(client.chat(id, &chat_request("fake-large", vec![user(&words)], None)).await);
    assert_eq!(
        (refused.http_status(), refused.error, refused.required),
        (402, ErrorCode::PromptExceedsBudget, Some(148))
    );

    let hi = || vec![user("hi")];
    assert_eq!(code(client.chat(id, &chat_request("fake-small", hi(), Some(0))).await), (400, ErrorCode::ZeroOutput));
    assert_eq!(code(client.chat(id, &chat_request("no-such-model", hi(), None)).await), (400, ErrorCode::Model));
    assert_eq!(code(client.chat(id, &chat_request("fake-small", vec![], None)).await), (400, ErrorCode::BadRequest));

    // `[[long]]` uses the whole hold: 5 prompt tokens leave (100 − 5) / 3 = 31 outputs.
    let long = client.chat(id, &chat_request("fake-small", vec![user("[[long]]")], None)).await.unwrap();
    let receipt = &long.receipt;
    assert_eq!((receipt.turn, receipt.prompt_tokens, receipt.completion_tokens), (1, 5, 31));
    assert_eq!((receipt.charge, receipt.remaining, receipt.finish), (98, 2, FinishReason::Length));

    // "hi" needs 5 + 3 = 8: within s, but not within the 2 left.
    assert_eq!(
        code(client.chat(id, &chat_request("fake-small", hi(), None)).await),
        (402, ErrorCode::SessionExhausted)
    );

    let refund = client.close(id).await.unwrap().refund;
    assert_eq!(wallet.holder.refund_amount(refund.as_bytes()).unwrap(), 2);

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn one_turn_at_a_time() {
    let world = World::new(Policy::default());
    let (gate, mut first, wallet) = world.ready().await;
    let mut second = world.attested(&gate).await;
    let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, 1_000).unwrap();
    let (id, _) = first.open_live(&spend).await;

    // The stall holds the first turn open while the second asks.
    let slow = chat_request("fake-small", vec![user("[[stall:500]] slowly")], None);
    let quick = chat_request("fake-small", vec![user("hi")], None);
    let (slow, quick) = tokio::join!(first.chat(id, &slow), async {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        second.chat(id, &quick).await
    });
    assert_eq!(slow.unwrap().receipt.turn, 1);
    assert_eq!(code(quick), (409, ErrorCode::TurnActive));

    drop((first, second));
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spend_over_the_maximum_settles_at_once_in_full() {
    let world = World::new(Policy::default());
    let (gate, mut client, wallet) = world.ready().await;
    let s = MAXIMUM_SPEND + 1;
    let (spend_state, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, s).unwrap();

    let OpenSessionResponse::Settled { refund } = client.open(&spend).await.unwrap() else {
        panic!("an out-of-policy spend opens no session");
    };
    assert_eq!(wallet.holder.refund_amount(refund.as_bytes()).unwrap(), s);
    assert_eq!(code(client.open(&spend).await), (409, ErrorCode::UseRecover));
    assert_eq!(settled(client.recover(&spend).await.unwrap()), refund.as_bytes());
    assert_eq!(wallet.holder.finalize_refund(spend_state, refund.as_bytes()).unwrap().balance(), USD_5);

    drop(client);
    stop(gate).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_and_turn_limits_close_through_the_same_path() {
    let world = World::new(Policy { idle_seconds: 1, max_turns: 1 });
    let (gate, mut client, wallet) = world.ready().await;

    // Idle: no turn for a second, so the gate closes it with t = s.
    let (_, idle_spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, 500).unwrap();
    client.open_live(&idle_spend).await;
    let refund = settle_eventually(&mut client, &idle_spend).await;
    assert_eq!(wallet.holder.refund_amount(&refund).unwrap(), 500);

    // Turn limit: the first turn is the last, and closing follows it.
    let (_, spend) = wallet.holder.prove_spend(wallet.buy(&gate, USD_5).await, 500).unwrap();
    let (id, _) = client.open_live(&spend).await;
    let turn = client.chat(id, &chat_request("fake-small", vec![user("hello there friend")], None)).await.unwrap();
    let refund = settle_eventually(&mut client, &spend).await;
    assert_eq!(wallet.holder.refund_amount(&refund).unwrap(), 500 - turn.receipt.charge);

    drop(client);
    stop(gate).await;
}

/// Recovers until the gate has settled the session on its own.
async fn settle_eventually(client: &mut crate::harness::Client, spend: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match client.recover(spend).await {
            Ok(RecoverResponse::Settled { refund }) => return refund.into_vec(),
            Ok(RecoverResponse::Live { .. }) => {}
            other => assert_eq!(code(other), (503, ErrorCode::Retry)),
        }
        assert!(Instant::now() < deadline, "the session never closed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
