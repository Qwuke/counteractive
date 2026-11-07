//! What tier 1 can't reach through the sockets: a store write that fails.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use credit::{Holder, Moderator, Nullifier, ServiceContext};
use proto::gate::{OpenSessionResponse, RecoverResponse};
use proto::{ErrorCode, SecretBlob, SessionPolicy};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::Sessions;
use crate::custody::OperatorDelivered;
use crate::models::Models;
use crate::state::Unlocked;
use crate::store::{
    IssueOutcome, IssuedRow, IssuerRow, MoneyStore, OrderRef, SettleOutcome, SpendStatus, SqlCipherStore, StoreError,
};

/// A real store whose first settlements fail, as a full disk would.
struct FlakyStore {
    inner: SqlCipherStore,
    failures_left: AtomicU32,
    settle_attempts: AtomicU32,
}

impl MoneyStore for FlakyStore {
    fn issuer(&self) -> Result<IssuerRow, StoreError> {
        self.inner.issuer()
    }

    fn set_manifest_version(&self, version: u64) -> Result<(), StoreError> {
        self.inner.set_manifest_version(version)
    }

    fn spend_status(&self, nullifier: &Nullifier) -> Result<SpendStatus, StoreError> {
        self.inner.spend_status(nullifier)
    }

    fn insert_pending(&self, nullifier: &Nullifier) -> Result<(), StoreError> {
        self.inner.insert_pending(nullifier)
    }

    fn settle(&self, nullifier: &Nullifier, sealed_refund: &[u8]) -> Result<SettleOutcome, StoreError> {
        self.settle_attempts.fetch_add(1, Ordering::SeqCst);
        let failing = self.failures_left.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1));
        if failing.is_ok() {
            return Err(StoreError::Io(std::io::Error::other("no space left on device")));
        }
        self.inner.settle(nullifier, sealed_refund)
    }

    fn issued(&self, order_ref: &OrderRef) -> Result<Option<IssuedRow>, StoreError> {
        self.inner.issued(order_ref)
    }

    fn record_issue(
        &self,
        order_ref: &OrderRef,
        request_sha256: &[u8; 32],
        response: &[u8],
    ) -> Result<IssueOutcome, StoreError> {
        self.inner.record_issue(order_ref, request_sha256, response)
    }
}

#[tokio::test]
async fn closing_retries_its_write_until_it_lands() {
    let dir = tempfile::tempdir().unwrap();
    let k = [9u8; 32];
    let key = OperatorDelivered::new(Sha256::digest(k).into()).accept(&hex::encode(k)).unwrap();
    let (inner, _) = SqlCipherStore::open_or_create(dir.path(), &key, 1).unwrap();
    let context = ServiceContext::new("flaky", 1).unwrap();
    let moderator = Moderator::from_seed(&inner.issuer().unwrap().seed, &context).unwrap();
    let store = Arc::new(FlakyStore { inner, failures_left: AtomicU32::new(2), settle_attempts: AtomicU32::new(0) });
    let unlocked = Unlocked {
        store: Arc::<FlakyStore>::clone(&store),
        issuer_public_key: moderator.public_key(),
        moderator: Arc::new(moderator),
        key_epoch: 1,
    };
    let policy = SessionPolicy { maximum_spend: 10_000, idle_seconds: 900, max_turns: 64, max_age_seconds: 3600 };
    // No turns run here, so no engine is needed.
    let models = Arc::new(Models::from_catalogue(&[], Path::new("unused.sock")).unwrap());
    let (shutdown, tasks) = (CancellationToken::new(), TaskTracker::new());
    let sessions = Sessions::start(&unlocked, policy, models, shutdown.clone(), tasks.clone());

    let holder = Holder::new(&context, &unlocked.issuer_public_key).unwrap();
    let (state, request) = holder.issue_request().unwrap();
    let credential = holder.finalize_issue(state, &unlocked.moderator.issue(1_000, &request).unwrap()).unwrap();
    let (_, spend) = holder.prove_spend(credential, 400).unwrap();
    let OpenSessionResponse::Open { session_id, .. } = sessions.open(SecretBlob::new(spend.clone())).await.unwrap()
    else {
        panic!("a spend within policy opens a session");
    };

    // The first write fails: the wallet hears `retry`, and the closing task keeps going.
    assert_eq!(sessions.close(session_id).await.unwrap_err().error, ErrorCode::Retry);
    let deadline = Instant::now() + Duration::from_secs(10);
    let refund = loop {
        match sessions.recover(SecretBlob::new(spend.clone())).await {
            Ok(RecoverResponse::Settled { refund }) => break refund,
            Err(body) => assert_eq!(body.error, ErrorCode::Retry, "closing is still under way"),
            Ok(live) => panic!("a closing session isn't live: {live:?}"),
        }
        assert!(Instant::now() < deadline, "the refund was never stored");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(holder.refund_amount(refund.as_bytes()).unwrap(), 400);
    assert_eq!(store.settle_attempts.load(Ordering::SeqCst), 3);
    assert_eq!(sessions.live_sessions(), 0);

    shutdown.cancel();
    tasks.close();
    tasks.wait().await;
}
