//! `issue.sock`: the same request gets the same bytes, a different one
//! `409 order_issued`, and only a listed product's credits are issued.

use proto::ErrorCode;

use crate::harness::{Policy, USD_1, USD_5, World, issue, issue_body, random, stop};

#[tokio::test]
async fn issuance_is_idempotent_by_order() {
    let world = World::new(Policy::default());
    let (gate, operator, wallet) = world.ready().await;
    let order: [u8; 32] = random();

    let (state, request) = wallet.holder.issue_request().unwrap();
    let first = issue(&gate, &issue_body(order, USD_5, request.clone())).await.unwrap();
    let again = issue(&gate, &issue_body(order, USD_5, request.clone())).await.unwrap();
    assert_eq!(first, again, "a repeated request gets identical bytes");
    let credential = wallet.holder.finalize_issue(state, first.issue_response.as_bytes()).unwrap();
    assert_eq!(credential.balance(), USD_5);

    // Another request for the same order is a different issuance.
    let (_, other) = wallet.holder.issue_request().unwrap();
    assert_eq!(issue(&gate, &issue_body(order, USD_5, other)).await.unwrap_err(), (409, ErrorCode::OrderIssued));
    assert_eq!(issue(&gate, &issue_body(order, USD_5, request)).await.unwrap(), first, "still answered");

    // Credits no product lists are refused, and nothing is recorded for them.
    let (state, request) = wallet.holder.issue_request().unwrap();
    let unlisted = random();
    assert_eq!(
        issue(&gate, &issue_body(unlisted, USD_1 + 1, request.clone())).await.unwrap_err(),
        (400, ErrorCode::BadRequest)
    );
    let listed = issue(&gate, &issue_body(unlisted, USD_1, request)).await.unwrap();
    assert_eq!(wallet.holder.finalize_issue(state, listed.issue_response.as_bytes()).unwrap().balance(), USD_1);

    // A request that isn't one is refused.
    assert_eq!(
        issue(&gate, &issue_body(random(), USD_1, vec![1, 2, 3])).await.unwrap_err(),
        (400, ErrorCode::BadRequest)
    );

    drop(operator);
    stop(gate).await;
}
