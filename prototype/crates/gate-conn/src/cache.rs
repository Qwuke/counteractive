//! Recent verdicts, so a new connection to the same server skips the
//! evidence round trip (SPEC section 4).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use attest::{Role, Verdict};

/// How long a verdict stays usable by default.
pub const DEFAULT_VERDICT_TTL: Duration = Duration::from_secs(60 * 60);

/// A verdict is reused only for the same role, the same TLS key, and the
/// same manifest version: a newer manifest may have dropped the release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    role: Role,
    spki_sha256: [u8; 32],
    manifest_version: u64,
}

/// Verdicts by server TLS key, each usable until its TTL runs out.
#[derive(Debug)]
pub struct VerdictCache {
    ttl: Duration,
    entries: Mutex<HashMap<Key, (Instant, Arc<Verdict>)>>,
}

impl VerdictCache {
    /// A cache keeping verdicts for [`DEFAULT_VERDICT_TTL`].
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_VERDICT_TTL)
    }

    /// A cache keeping verdicts for `ttl`.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self { ttl, entries: Mutex::new(HashMap::new()) }
    }

    pub(crate) fn get(&self, role: Role, spki_sha256: &[u8; 32], manifest_version: u64) -> Option<Arc<Verdict>> {
        let key = Key { role, spki_sha256: *spki_sha256, manifest_version };
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let (stored, verdict) = entries.get(&key)?;
        (stored.elapsed() < self.ttl).then(|| Arc::clone(verdict))
    }

    /// Stores a fresh verdict, dropping expired ones so the map stays small.
    pub(crate) fn insert(&self, verdict: Arc<Verdict>) {
        let key = Key {
            role: verdict.role,
            spki_sha256: verdict.tls_spki_sha256,
            manifest_version: verdict.manifest_version,
        };
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|_, (stored, _)| stored.elapsed() < self.ttl);
        entries.insert(key, (Instant::now(), verdict));
    }
}

impl Default for VerdictCache {
    fn default() -> Self {
        Self::new()
    }
}
