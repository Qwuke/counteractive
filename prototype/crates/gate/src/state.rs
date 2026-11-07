//! The boot states and their transitions (docs 05 section 2).
//!
//! ```text
//! Locked --unlock, new store or no manifest yet--> AwaitingManifest --signed manifest--> Ready
//! Locked --unlock, store with an accepted manifest-------------------------------------> Ready
//! ```
//!
//! A process restart is a fresh start: the gate comes back Locked with a
//! new TLS key and no sessions. Draining and export aren't built yet.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use credit::{IssuerPublicKey, Moderator, ServiceContext};
use manifest::{GatePins, gate_accept};
use proto::gate::{GateState, StateResponse, UnlockResponse};
use proto::{Blob, ErrorCode, SignedManifest};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::attestation::Attestor;
use crate::blocking;
use crate::config::Release;
use crate::custody::{OperatorDelivered, StoreKey};
use crate::models::Models;
use crate::sessions::Sessions;
use crate::store::{self, MoneyStore, Opened, SqlCipherStore};

/// What an opened store gives: the store and the issuer it holds.
pub struct Unlocked {
    /// The money store.
    pub store: Arc<dyn MoneyStore>,
    /// The issuer key, derived from the stored seed.
    pub moderator: Arc<Moderator>,
    /// Its public half, which a manifest must name.
    pub issuer_public_key: IssuerPublicKey,
    /// The store's key epoch, which a manifest must name.
    pub key_epoch: u32,
}

/// Serving: the store, plus live sessions.
pub struct Ready {
    /// The opened store and issuer.
    pub unlocked: Arc<Unlocked>,
    /// The slot table and session flows.
    pub sessions: Arc<Sessions>,
}

#[derive(Clone)]
enum Boot {
    Locked,
    AwaitingManifest(Arc<Unlocked>),
    Ready(Arc<Ready>),
}

/// The whole gate, as its handlers see it.
pub struct Gate {
    /// The measured `release.json`.
    pub release: Release,
    /// This boot's TLS identity and evidence.
    pub attestor: Attestor,
    /// How operator calls authenticate.
    pub custody: OperatorDelivered,
    data: PathBuf,
    models: Arc<Models>,
    boot: RwLock<Boot>,
    /// Serialises unlock and manifest acceptance, which await store work.
    transitions: tokio::sync::Mutex<()>,
    /// The latest signed manifest, served even while Locked.
    manifest: RwLock<Option<SignedManifest>>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

impl Gate {
    /// A Locked gate. The stored manifest, if any, is served from the start.
    pub fn new(
        release: Release,
        attestor: Attestor,
        models: Arc<Models>,
        data: PathBuf,
        shutdown: CancellationToken,
        tasks: TaskTracker,
    ) -> Self {
        let manifest = store::read_manifest(&data);
        Self {
            custody: OperatorDelivered::new(release.config.k_sha256),
            release,
            attestor,
            data,
            models,
            boot: RwLock::new(Boot::Locked),
            transitions: tokio::sync::Mutex::new(()),
            manifest: RwLock::new(manifest),
            shutdown,
            tasks,
        }
    }

    /// The boot state.
    pub fn state(&self) -> GateState {
        match self.boot() {
            Boot::Locked => GateState::Locked,
            Boot::AwaitingManifest(_) => GateState::AwaitingManifest,
            Boot::Ready(_) => GateState::Ready,
        }
    }

    /// The serving state, or `503 locked` before Ready.
    pub fn ready(&self) -> Result<Arc<Ready>, ErrorCode> {
        match self.boot() {
            Boot::Ready(ready) => Ok(ready),
            _ => Err(ErrorCode::Locked),
        }
    }

    /// Session slots held, 0 before Ready.
    pub fn live_sessions(&self) -> u64 {
        self.ready().map_or(0, |ready| ready.sessions.live_sessions())
    }

    /// The latest signed manifest, if one is stored.
    pub fn served_manifest(&self) -> Option<SignedManifest> {
        self.manifest.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Tracks work that must finish even if its client goes, so shutdown waits for it.
    pub fn tasks(&self) -> &TaskTracker {
        &self.tasks
    }

    /// `POST /v1/operator/unlock` with a K that matched the pin.
    ///
    /// With no store, creates one with a fresh seed and `release.json`'s
    /// key epoch. With a store, opens it: Ready if a manifest was accepted
    /// before, else `AwaitingManifest`. Repeating it in `AwaitingManifest`
    /// returns the same key, which covers a lost response. Ready doesn't
    /// serve it (docs 05 section 2).
    pub async fn unlock(&self, key: StoreKey) -> Result<UnlockResponse, ErrorCode> {
        let _transition = self.transitions.lock().await;
        match self.boot() {
            Boot::Locked => {}
            Boot::AwaitingManifest(unlocked) => return Ok(unlock_response(GateState::AwaitingManifest, &unlocked)),
            Boot::Ready(_) => return Err(ErrorCode::Locked),
        }

        let (data, key_epoch) = (self.data.clone(), self.release.config.key_epoch);
        let opened = blocking(move || {
            let (store, opened) = SqlCipherStore::open_or_create(&data, &key, key_epoch)?;
            let issuer = store.issuer()?;
            Ok::<_, store::StoreError>((store, opened, issuer))
        })
        .await;
        let (store, opened, issuer) = opened.map_err(|error| {
            tracing::error!(%error, "opening the money store failed");
            ErrorCode::Internal
        })?;
        if issuer.key_epoch != key_epoch {
            tracing::error!(
                store = issuer.key_epoch,
                release = key_epoch,
                "the store's key epoch differs from release.json"
            );
            return Err(ErrorCode::ManifestMismatch);
        }
        let moderator = ServiceContext::new(&self.release.config.service_id, issuer.key_epoch)
            .and_then(|context| Moderator::from_seed(&issuer.seed, &context))
            .map_err(|error| {
                tracing::error!(%error, "deriving the issuer key failed");
                ErrorCode::Internal
            })?;
        let unlocked = Arc::new(Unlocked {
            store: Arc::new(store),
            issuer_public_key: moderator.public_key(),
            moderator: Arc::new(moderator),
            key_epoch: issuer.key_epoch,
        });

        let (next, state) = if issuer.manifest_version > 0 {
            (Boot::Ready(self.ready_with(Arc::clone(&unlocked))), GateState::Ready)
        } else {
            (Boot::AwaitingManifest(Arc::clone(&unlocked)), GateState::AwaitingManifest)
        };
        *self.boot.write().unwrap_or_else(PoisonError::into_inner) = next;
        tracing::info!(?state, created = opened == Opened::Created, "unlocked");
        Ok(unlock_response(state, &unlocked))
    }

    /// `POST /v1/operator/manifest`: accepts a signed manifest for this
    /// store and release (SPEC section 5, Manifest), then serves it and is Ready.
    pub async fn accept_manifest(&self, signed: SignedManifest) -> Result<StateResponse, ErrorCode> {
        let _transition = self.transitions.lock().await;
        let unlocked = match self.boot() {
            Boot::Locked => return Err(ErrorCode::Locked),
            Boot::AwaitingManifest(unlocked) => unlocked,
            Boot::Ready(ready) => Arc::clone(&ready.unlocked),
        };
        let issuer = unlocked.store.issuer().map_err(|error| {
            tracing::error!(%error, "reading the issuer row failed");
            ErrorCode::Internal
        })?;
        let pins = GatePins {
            operator_public_key: &self.release.operator_key,
            service_id: &self.release.config.service_id,
            issuer_public_key: &unlocked.issuer_public_key.to_bytes(),
            key_epoch: unlocked.key_epoch,
            stored_version: issuer.manifest_version,
            release_config_sha256: &self.release.sha256,
        };
        let manifest = gate_accept(&signed, &pins).map_err(|error| {
            tracing::warn!(%error, "refused a manifest");
            ErrorCode::ManifestMismatch
        })?;
        // The store's 0 means "none accepted", so version 0 couldn't be told apart from no manifest.
        if manifest.version == 0 {
            tracing::warn!("refused a manifest with version 0");
            return Err(ErrorCode::ManifestMismatch);
        }

        let (data, store, files, version) =
            (self.data.clone(), Arc::clone(&unlocked.store), signed.clone(), manifest.version);
        let stored = blocking(move || {
            store::write_manifest(&data, &files)?;
            store.set_manifest_version(version)
        })
        .await;
        stored.map_err(|error| {
            tracing::error!(%error, "storing the manifest failed");
            ErrorCode::Internal
        })?;

        if let Boot::AwaitingManifest(unlocked) = self.boot() {
            *self.boot.write().unwrap_or_else(PoisonError::into_inner) = Boot::Ready(self.ready_with(unlocked));
        }
        *self.manifest.write().unwrap_or_else(PoisonError::into_inner) = Some(signed);
        tracing::info!(version, "manifest accepted; ready");
        Ok(StateResponse { state: GateState::Ready })
    }

    fn boot(&self) -> Boot {
        self.boot.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn ready_with(&self, unlocked: Arc<Unlocked>) -> Arc<Ready> {
        let sessions = Sessions::start(
            &unlocked,
            self.release.config.session_policy,
            Arc::clone(&self.models),
            self.shutdown.clone(),
            self.tasks.clone(),
        );
        Arc::new(Ready { unlocked, sessions })
    }
}

fn unlock_response(state: GateState, unlocked: &Unlocked) -> UnlockResponse {
    UnlockResponse { state, issuer_public_key: Blob::new(unlocked.issuer_public_key.to_bytes().to_vec()) }
}
