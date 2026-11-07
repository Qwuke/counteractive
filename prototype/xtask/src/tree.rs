//! The dev tree (SPEC section 9): where every key, config, socket, store
//! and log lives. `.dev/` by default; a temp dir for `e2e`.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use anyhow::Context as _;

/// Paths inside one dev tree.
#[derive(Debug, Clone)]
pub struct Tree {
    root: PathBuf,
}

impl Tree {
    /// A tree rooted at `root`. Nothing is created yet.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The root, which also holds `manifest.json` and `manifest.sig`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `keys/k.bin`: K, the gate's store key.
    pub fn k(&self) -> PathBuf {
        self.keys().join("k.bin")
    }

    /// `keys/operator.key`: the minisign key that signs manifests.
    pub fn operator_key(&self) -> PathBuf {
        self.keys().join("operator.key")
    }

    /// `keys/operator.pub`: its public half, in base64.
    pub fn operator_pub(&self) -> PathBuf {
        self.keys().join("operator.pub")
    }

    /// `keys/dev_attest.key`: the dev-insecure report key.
    pub fn dev_attest_key(&self) -> PathBuf {
        self.keys().join("dev_attest.key")
    }

    /// `keys/storefront.token`: the storefront operator token.
    pub fn storefront_token(&self) -> PathBuf {
        self.keys().join("storefront.token")
    }

    /// The keys directory.
    pub fn keys(&self) -> PathBuf {
        self.root.join("keys")
    }

    /// `release.json`, the release `up` runs.
    pub fn release(&self) -> PathBuf {
        self.root.join("release.json")
    }

    /// `release-idle.json`: the same release with a 2-second idle timer,
    /// for e2e step 9. Only `e2e` writes it.
    pub fn idle_release(&self) -> PathBuf {
        self.root.join("release-idle.json")
    }

    /// `manifest.json`: the canonical manifest bytes the operator signed.
    pub fn manifest(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    /// `manifest.sig`: the minisign signature text.
    pub fn manifest_sig(&self) -> PathBuf {
        self.root.join("manifest.sig")
    }

    /// `run/`: the gate's and engine's sockets.
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }

    /// `run/gate.sock`: inner TLS, where tor would forward.
    pub fn gate_socket(&self) -> PathBuf {
        self.run().join("gate.sock")
    }

    /// `run/issue.sock`: issuance, for the storefront only.
    pub fn issue_socket(&self) -> PathBuf {
        self.run().join("issue.sock")
    }

    /// `run/engine.sock`: fake-engine.
    pub fn engine_socket(&self) -> PathBuf {
        self.run().join("engine.sock")
    }

    /// `gate-data/`: the money store.
    pub fn gate_data(&self) -> PathBuf {
        self.root.join("gate-data")
    }

    /// `wallet/`: the wallet directory.
    pub fn wallet(&self) -> PathBuf {
        self.root.join("wallet")
    }

    /// `logs/`: one log per process.
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// `up.json`: what `up` started, for `unlock` and `down`.
    pub fn up_state(&self) -> PathBuf {
        self.root.join("up.json")
    }

    /// `tor/`: C tor's torrc, data and the gate's onion service (tier 2).
    pub fn tor(&self) -> PathBuf {
        self.root.join("tor")
    }

    /// `tor/onion/hostname`: the gate's onion address, written by C tor.
    pub fn onion_hostname(&self) -> PathBuf {
        self.tor().join("onion").join("hostname")
    }

    /// The gate's onion host, when C tor runs an onion service for it.
    /// The manifest and the wallet's pins name it; tier 1 has none.
    pub fn onion(&self) -> anyhow::Result<Option<String>> {
        match std::fs::read_to_string(self.onion_hostname()) {
            Ok(host) => Ok(Some(host.trim().to_owned())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("reading {}", self.onion_hostname().display())),
        }
    }
}

/// Writes a new secret file with mode 0600, refusing to replace one.
pub fn write_secret(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(bytes).with_context(|| format!("writing {}", path.display()))?;
    file.sync_all().with_context(|| format!("syncing {}", path.display()))
}

/// Creates a directory and its parents with mode 0700.
pub fn create_private_dir(path: &Path) -> anyhow::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .with_context(|| format!("creating {}", path.display()))
}
