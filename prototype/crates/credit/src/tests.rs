//! Round trips through every byte encoding, and each rejection the gate relies on.

use secrecy::ExposeSecret as _;

use super::*;

struct Setup {
    seed: SecretSeed,
    ctx: ServiceContext,
    moderator: Moderator,
    holder: Holder,
}

fn setup() -> Setup {
    let seed = SecretSeed::generate().unwrap();
    let ctx = ServiceContext::new("counteractive-test", 1).unwrap();
    let moderator = Moderator::from_seed(&seed, &ctx).unwrap();
    let holder = Holder::new(&ctx, &moderator.public_key()).unwrap();
    Setup { seed, ctx, moderator, holder }
}

fn credential(setup: &Setup, credits: u64) -> Credential {
    let (state, request) = setup.holder.issue_request().unwrap();
    let response = setup.moderator.issue(credits, &request).unwrap();
    setup.holder.finalize_issue(state, &response).unwrap()
}

#[test]
fn full_cycle_through_byte_encodings() {
    let s = setup();

    // Issue, journalling the state in between.
    let (state, request) = s.holder.issue_request().unwrap();
    let state = s.holder.issue_state_from_bytes(state.to_bytes().expose_secret()).unwrap();
    let response = s.moderator.issue(5_000_000, &request).unwrap();
    let cred = s.holder.finalize_issue(state, &response).unwrap();
    let cred = s.holder.credential_from_bytes(cred.to_bytes().expose_secret()).unwrap();
    assert_eq!(cred.balance(), 5_000_000);

    // Spend, journalling the state and message.
    let (state, spend) = s.holder.prove_spend(cred, 500_000).unwrap();
    let state = s.holder.spend_state_from_bytes(state.to_bytes().expose_secret()).unwrap();
    assert_eq!(state.remainder(), 4_500_000);
    assert_eq!(s.holder.spend_amount(&spend).unwrap(), 500_000);

    // The gate's view.
    let peeked = s.moderator.peek_nullifier(&spend).unwrap();
    let facts = s.moderator.verify_spend(&spend).unwrap();
    assert_eq!(facts, SpendFacts { nullifier: peeked, budget: 500_000 });

    // Settle with 69 charged.
    let refund = s.moderator.refund(&spend, 499_931).unwrap();
    assert_eq!(s.holder.refund_amount(&refund).unwrap(), 499_931);
    let cred = s.holder.finalize_refund(state, &refund).unwrap();
    assert_eq!(cred.balance(), 5_000_000 - 69);

    // The next spend from the new credential has a different nullifier.
    let (_, next) = s.holder.prove_spend(cred, 500_000).unwrap();
    assert_ne!(s.moderator.peek_nullifier(&next).unwrap(), peeked);
}

#[test]
fn seed_round_trip_gives_the_same_issuer() {
    let s = setup();
    let again = SecretSeed::from_bytes(s.seed.expose()).unwrap();
    let moderator = Moderator::from_seed(&again, &s.ctx).unwrap();
    assert_eq!(moderator.public_key(), s.moderator.public_key());
    assert_eq!(SecretSeed::from_bytes(&[0; 47]).unwrap_err(), CreditError::Decode);
}

#[test]
fn public_keys_are_checked() {
    let s = setup();
    let key = s.moderator.public_key();
    assert_eq!(IssuerPublicKey::from_bytes(&key.to_bytes()).unwrap(), key);
    IssuerPublicKey::from_bytes(&[0; 33]).unwrap_err();
    IssuerPublicKey::from_bytes(&key.to_bytes()[..32]).unwrap_err();
}

#[test]
fn another_context_rejects_issuance_and_spends() {
    let s = setup();
    let other_ctx = ServiceContext::new("counteractive-test", 2).unwrap();

    let other_holder = Holder::new(&other_ctx, &s.moderator.public_key()).unwrap();
    let (state, request) = other_holder.issue_request().unwrap();
    let response = s.moderator.issue(1000, &request).unwrap();
    assert_eq!(other_holder.finalize_issue(state, &response).unwrap_err(), CreditError::Verify);

    let other_moderator = Moderator::from_seed(&s.seed, &other_ctx).unwrap();
    let (_, spend) = s.holder.prove_spend(credential(&s, 1000), 10).unwrap();
    assert_eq!(other_moderator.verify_spend(&spend).unwrap_err(), CreditError::Verify);
}

#[test]
fn another_issuer_key_rejects() {
    let s = setup();
    let stranger = Moderator::from_seed(&SecretSeed::generate().unwrap(), &s.ctx).unwrap();
    let (state, request) = s.holder.issue_request().unwrap();
    let response = stranger.issue(1000, &request).unwrap();
    assert_eq!(s.holder.finalize_issue(state, &response).unwrap_err(), CreditError::Verify);

    let (_, spend) = s.holder.prove_spend(credential(&s, 1000), 10).unwrap();
    assert_eq!(stranger.verify_spend(&spend).unwrap_err(), CreditError::Verify);
}

#[test]
fn nonzero_allowance_is_rejected() {
    // Holder always proves with a = 0, so build a hostile proof with act directly.
    let s = setup();
    let params: act::Params = act::Params::new(BALANCE_WIDTH).unwrap();
    let key: act::SecretKey = act::SecretKey::from_seed(s.seed.expose()).unwrap();
    let ctx_cred = s.ctx.ctx_cred();
    let (state, request) = act::issue_request(&params).unwrap();
    let verified = act::verify_issue_request(&params, &request).unwrap();
    let response = act::issue_response(&key, ctx_cred, 1000, verified).unwrap();
    let cred = act::finalize_issue(&params, &key.public_key(), ctx_cred, state, &response).unwrap();
    let (_, spend) = act::prove_spend(&params, cred, ctx_cred, 10, 5, s.ctx.ctx_spend()).unwrap();
    let spend = spend.to_bytes();

    assert_eq!(s.moderator.verify_spend(&spend).unwrap_err(), CreditError::Allowance);
    assert_eq!(s.moderator.refund(&spend, 0).unwrap_err(), CreditError::Allowance);
    // Peeking still works, so the gate can apply its admission table first.
    s.moderator.peek_nullifier(&spend).unwrap();
}

#[test]
fn refund_cannot_exceed_spend() {
    let s = setup();
    let (_, spend) = s.holder.prove_spend(credential(&s, 1000), 100).unwrap();
    assert_eq!(s.moderator.refund(&spend, 101).unwrap_err(), CreditError::Amount);
    s.moderator.refund(&spend, 100).unwrap();
}

#[test]
fn amounts_are_bounded_by_the_balance_width() {
    let s = setup();
    let (_, request) = s.holder.issue_request().unwrap();
    assert_eq!(s.moderator.issue(MAX_BALANCE + 1, &request).unwrap_err(), CreditError::Amount);
    s.moderator.issue(MAX_BALANCE, &request).unwrap();
    s.holder.prove_spend(credential(&s, 100), 101).unwrap_err();
}

#[test]
fn tampered_messages_are_rejected() {
    let s = setup();
    let (_, mut spend) = s.holder.prove_spend(credential(&s, 1000), 10).unwrap();
    let last = spend.len() - 1;
    spend[last] ^= 1;
    s.moderator.verify_spend(&spend).unwrap_err();
    s.moderator.verify_spend(&spend[..last]).unwrap_err();
    assert_eq!(s.moderator.peek_nullifier(b"junk").unwrap_err(), CreditError::Decode);

    let (state, request) = s.holder.issue_request().unwrap();
    let mut response = s.moderator.issue(1000, &request).unwrap();
    response[0] ^= 1;
    s.holder.finalize_issue(state, &response).unwrap_err();

    let (_, mut request) = s.holder.issue_request().unwrap();
    request[40] ^= 1;
    s.moderator.issue(1000, &request).unwrap_err();
}

#[test]
fn a_refund_only_finishes_its_own_spend() {
    let s = setup();
    let (state_a, _spend_a) = s.holder.prove_spend(credential(&s, 1000), 100).unwrap();
    let (_state_b, spend_b) = s.holder.prove_spend(credential(&s, 1000), 100).unwrap();
    let refund_b = s.moderator.refund(&spend_b, 50).unwrap();
    assert_eq!(s.holder.finalize_refund(state_a, &refund_b).unwrap_err(), CreditError::Verify);
}

#[test]
fn states_never_debug_their_contents() {
    let s = setup();
    let (issue_state, _) = s.holder.issue_request().unwrap();
    let cred = credential(&s, 1000);
    assert_eq!(format!("{issue_state:?}"), "IssueState([redacted])");
    assert_eq!(format!("{cred:?}"), "Credential([redacted])");
    assert_eq!(format!("{:?}", s.seed), "SecretSeed([redacted])");
    let (spend_state, spend) = s.holder.prove_spend(cred, 1).unwrap();
    assert_eq!(format!("{spend_state:?}"), "SpendState([redacted])");
    let facts = s.moderator.verify_spend(&spend).unwrap();
    assert!(!format!("{facts:?}").contains(&hex::encode(facts.nullifier.as_bytes())));
    assert!(format!("{:?}", s.moderator).contains("counteractive-test"));
}
