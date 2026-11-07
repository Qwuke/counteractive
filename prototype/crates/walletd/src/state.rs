//! What walletd holds: the pins, the journal, the gate client, the live
//! conversations, the chains being driven, and the accepted manifest with
//! its credit holder.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use credit::Holder;
use manifest::{AcceptPins, Manifest};
use wallet_core::{ChainId, Journal, Pins, WalletError, manifest_store};

use crate::conversation::Conversations;
use crate::gate::GateClient;
use crate::token::Token;

/// The accepted manifest and the holder for its issuer key.
pub struct Context {
    /// The accepted manifest.
    pub manifest: Manifest,
    /// The wallet's side of the credit scheme.
    pub holder: Holder,
}

/// The daemon's shared state.
pub struct Walletd {
    /// The pins from `wallet.toml`.
    pub pins: Pins,
    accept_pins: AcceptPins,
    journal: Mutex<Journal>,
    /// The gate, over the Unix or Arti dialer.
    pub gate: GateClient,
    /// Live conversations, by id.
    pub conversations: Conversations,
    /// Chains some flow is driving, which `reconcile` leaves alone.
    pub attached: Attached,
    context: Mutex<Option<Arc<Context>>>,
    /// The local API's bearer token.
    pub token: Token,
    /// `Host` header values the local API answers to.
    pub allowed_hosts: Vec<String>,
}

impl Walletd {
    /// Assembles the state.
    pub fn new(
        pins: Pins,
        journal: Journal,
        gate: GateClient,
        token: Token,
        allowed_hosts: Vec<String>,
    ) -> Result<Self, WalletError> {
        Ok(Self {
            accept_pins: pins.accept_pins()?,
            pins,
            journal: Mutex::new(journal),
            gate,
            conversations: Conversations::default(),
            attached: Attached::default(),
            context: Mutex::new(None),
            token,
            allowed_hosts,
        })
    }

    /// The journal. Held only for synchronous steps, never across an await.
    pub fn journal(&self) -> MutexGuard<'_, Journal> {
        lock(&self.journal)
    }

    /// The manifest pins.
    pub fn accept_pins(&self) -> &AcceptPins {
        &self.accept_pins
    }

    /// The accepted manifest and its holder, rebuilt when the user has
    /// approved a different manifest since the last call.
    pub fn context(&self) -> Result<Arc<Context>, WalletError> {
        let manifest = manifest_store::require_accepted(&self.journal(), &self.accept_pins)?;
        let mut cached = lock(&self.context);
        if let Some(context) = cached.as_ref().filter(|context| context.manifest == manifest) {
            return Ok(Arc::clone(context));
        }
        let holder = manifest_store::holder(&self.pins, &manifest)?;
        let context = Arc::new(Context { manifest, holder });
        *cached = Some(Arc::clone(&context));
        Ok(context)
    }
}

/// The chains some flow is driving, with how many [`Attachment`]s hold each.
#[derive(Default)]
pub struct Attached(Arc<Mutex<HashMap<ChainId, usize>>>);

impl Attached {
    /// Marks a chain as driven by the caller until the [`Attachment`] drops.
    ///
    /// Call it with the journal held, in the same critical section that
    /// read or wrote the chain's state: `reconcile` picks its orphans under
    /// the journal too, so it never sees that state without the mark.
    pub fn attach(&self, chain: ChainId) -> Attachment {
        *lock(&self.0).entry(chain).or_default() += 1;
        Attachment { chain, attached: Arc::clone(&self.0) }
    }

    /// Whether some flow is driving a chain.
    pub fn contains(&self, chain: ChainId) -> bool {
        lock(&self.0).contains_key(&chain)
    }
}

/// A chain marked as driven by one flow: a live conversation's session, an
/// open in progress, or an orphan `reconcile` is settling. Reconcile leaves
/// such chains alone, so two flows never drive one spend at once.
///
/// Dropping it releases the mark however the flow ended, so a session lost
/// to an error goes back to `reconcile` instead of looking held forever.
/// Holders are counted, not just recorded: a conversation may open a chain
/// that another flow settled a moment ago and has yet to let go of.
pub struct Attachment {
    chain: ChainId,
    attached: Arc<Mutex<HashMap<ChainId, usize>>>,
}

impl Drop for Attachment {
    fn drop(&mut self) {
        let mut attached = lock(&self.attached);
        if let Some(count) = attached.get_mut(&self.chain) {
            *count -= 1;
            if *count == 0 {
                attached.remove(&self.chain);
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_stays_attached_until_its_last_holder_drops() {
        let attached = Attached::default();
        let (one, other) = (ChainId(1), ChainId(2));
        let settling = attached.attach(one);
        let opening = attached.attach(one);
        assert!(attached.contains(one) && !attached.contains(other));
        drop(settling);
        assert!(attached.contains(one), "the newer holder still drives it");
        drop(opening);
        assert!(!attached.contains(one), "with no holder left, reconcile may take it");
    }
}
