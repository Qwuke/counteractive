//! Locked, unlock, `AwaitingManifest`, manifest, Ready, and a restart.

use gate_conn::ApiRequest;
use proto::ErrorCode;
use proto::gate::{GateHealth, GateState, OperatorStateResponse};
use secrecy::SecretString;

use crate::harness::{Policy, World, code, connect, issue, issue_body, random, stop};

#[tokio::test]
async fn unlock_then_manifest_then_ready_then_restart() {
    let world = World::new(Policy::default());
    let gate = world.start().await;

    // Locked: health and attestation work, there's no manifest, nothing else is served.
    let mut plain = connect(&gate).await;
    assert_eq!(plain.health::<GateHealth>().await.unwrap().state, GateState::Locked);
    assert_eq!(code(plain.fetch_manifest().await), (503, ErrorCode::NoManifest));
    let mut operator = world.attested(&gate).await;
    assert_eq!(code(operator.open(b"spend").await), (503, ErrorCode::Locked));
    assert_eq!(code(operator.recover(b"spend").await), (503, ErrorCode::Locked));
    assert_eq!(issue(&gate, &issue_body(random(), 1, vec![1])).await.unwrap_err(), (503, ErrorCode::Locked));
    let placeholder = world.signed(&[2; 33], 1);
    assert_eq!(code(operator.post_manifest(world.k_token(), &placeholder).await), (503, ErrorCode::Locked));

    // A wrong or missing K is refused before anything else.
    let wrong = SecretString::from(hex::encode([7u8; 32]));
    assert_eq!(code(operator.unlock(wrong).await), (401, ErrorCode::Unauthorized));
    let unauthenticated = operator.conn.send_json::<OperatorStateResponse>(ApiRequest::get("/v1/operator/state")).await;
    assert_eq!(code(unauthenticated), (401, ErrorCode::Unauthorized));
    assert_eq!(operator_state(&mut operator, &world).await.state, GateState::Locked);

    // Unlock creates the store and its issuer key; repeating it returns the same key.
    let unlocked = operator.unlock(world.k_token()).await.unwrap();
    assert_eq!(unlocked.state, GateState::AwaitingManifest);
    assert_eq!(unlocked.issuer_public_key.len(), 33);
    assert_eq!(operator.unlock(world.k_token()).await.unwrap(), unlocked);
    assert_eq!(code(operator.open(b"spend").await), (503, ErrorCode::Locked));

    // A manifest for another issuer key, or signed by someone else, is refused.
    let issuer = unlocked.issuer_public_key.as_bytes();
    let mismatched = operator.post_manifest(world.k_token(), &placeholder).await;
    assert_eq!(code(mismatched), (409, ErrorCode::ManifestMismatch));
    let forged = operator.post_manifest(world.k_token(), &world.signed_by_stranger(issuer, 1)).await;
    assert_eq!(code(forged), (409, ErrorCode::ManifestMismatch));

    // The right one makes the gate Ready and is served from then on.
    let signed = world.signed(issuer, 1);
    assert_eq!(operator.post_manifest(world.k_token(), &signed).await.unwrap().state, GateState::Ready);
    assert_eq!(plain.health::<GateHealth>().await.unwrap().state, GateState::Ready);
    assert_eq!(plain.fetch_manifest().await.unwrap(), signed);
    assert_eq!(code(operator.unlock(world.k_token()).await), (503, ErrorCode::Locked));
    let state = operator_state(&mut operator, &world).await;
    assert_eq!((state.state, state.live_sessions), (GateState::Ready, 0));
    // An older version can't replace it.
    let stale = operator.post_manifest(world.k_token(), &world.signed(issuer, 0)).await;
    assert_eq!(code(stale), (409, ErrorCode::ManifestMismatch));

    drop((plain, operator));
    stop(gate).await;

    // A restart is Locked again but serves the stored manifest, and unlock
    // goes straight to Ready with the same issuer key.
    let gate = world.start().await;
    let mut plain = connect(&gate).await;
    assert_eq!(plain.health::<GateHealth>().await.unwrap().state, GateState::Locked);
    assert_eq!(plain.fetch_manifest().await.unwrap(), signed);
    let mut operator = world.attested(&gate).await;
    let reopened = operator.unlock(world.k_token()).await.unwrap();
    assert_eq!((reopened.state, reopened.issuer_public_key), (GateState::Ready, unlocked.issuer_public_key));
    drop((plain, operator));
    stop(gate).await;
}

async fn operator_state(operator: &mut crate::harness::Client, world: &World) -> OperatorStateResponse {
    operator.conn.send_json(ApiRequest::get("/v1/operator/state").bearer(world.k_token())).await.unwrap()
}
