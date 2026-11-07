//! `xtask keygen`: the operator's keys for one dev tree.
//!
//! K and the storefront token are bearer secrets: only their hashes go
//! into `release.json`, and they leave this process only as bearer tokens
//! over attested connections.

use anyhow::{Context as _, bail};
use attest::DevAttestationKey;
use manifest::{OperatorPublicKey, OperatorSecretKey};
use rand::TryRng as _;
use secrecy::{ExposeSecret as _, ExposeSecretMut as _, SecretBox, SecretString};
use zeroize::Zeroizing;

use crate::tree::{Tree, create_private_dir, write_secret};

/// Every key in `keys/`.
pub struct Keys {
    k: SecretBox<[u8; 32]>,
    operator: OperatorSecretKey,
    dev_attest: DevAttestationKey,
    storefront_token: SecretBox<[u8; 32]>,
}

impl Keys {
    /// Generates fresh keys into `keys/`. Refuses if any exist: a new K
    /// would lock the operator out of the money store.
    pub fn generate(tree: &Tree) -> anyhow::Result<Self> {
        if tree.k().exists() {
            bail!("{} already exists; delete the dev tree to start over", tree.k().display());
        }
        create_private_dir(&tree.keys())?;
        let keys = Self {
            k: random_secret()?,
            operator: OperatorSecretKey::generate()?,
            dev_attest: DevAttestationKey::generate().context("no randomness for the dev key")?,
            storefront_token: random_secret()?,
        };
        write_secret(&tree.k(), keys.k.expose_secret())?;
        write_secret(&tree.operator_key(), keys.operator.to_text()?.expose_secret().as_bytes())?;
        write_secret(&tree.dev_attest_key(), keys.dev_attest.to_bytes().as_ref())?;
        write_secret(&tree.storefront_token(), keys.storefront_token.expose_secret())?;
        let public = keys.operator_public_key()?;
        std::fs::write(tree.operator_pub(), format!("{}\n", public.as_base64()))
            .with_context(|| format!("writing {}", tree.operator_pub().display()))?;
        Ok(keys)
    }

    /// Reads the keys `keygen` wrote.
    pub fn read(tree: &Tree) -> anyhow::Result<Self> {
        let operator_text = Zeroizing::new(read(&tree.operator_key())?);
        let operator_text = String::from_utf8(operator_text.to_vec()).context("operator.key is not text")?;
        let dev_attest = DevAttestationKey::from_file_contents(&read(&tree.dev_attest_key())?)
            .with_context(|| format!("parsing {}", tree.dev_attest_key().display()))?;
        Ok(Self {
            k: read_32(&tree.k())?,
            operator: OperatorSecretKey::from_text(&Zeroizing::new(operator_text))?,
            dev_attest,
            storefront_token: read_32(&tree.storefront_token())?,
        })
    }

    /// The gate operator's bearer token: K in hex.
    pub fn k_token(&self) -> SecretString {
        SecretString::from(hex::encode(self.k.expose_secret()))
    }

    /// The storefront operator's bearer token, in hex.
    pub fn storefront_bearer(&self) -> SecretString {
        SecretString::from(hex::encode(self.storefront_token.expose_secret()))
    }

    /// `SHA-256(K)`, as `release.json` pins it.
    pub fn k_sha256(&self) -> [u8; 32] {
        sha256(self.k.expose_secret())
    }

    /// `SHA-256(storefront token)`, as `release.json` pins it.
    pub fn storefront_token_sha256(&self) -> [u8; 32] {
        sha256(self.storefront_token.expose_secret())
    }

    /// The manifest signing key.
    pub fn operator(&self) -> &OperatorSecretKey {
        &self.operator
    }

    /// The manifest signing key's public half.
    pub fn operator_public_key(&self) -> anyhow::Result<OperatorPublicKey> {
        Ok(self.operator.public_key()?)
    }

    /// The dev report key's public half, listed in the manifest.
    pub fn dev_public_key(&self) -> [u8; 32] {
        self.dev_attest.public_key()
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes).into()
}

fn random_secret() -> anyhow::Result<SecretBox<[u8; 32]>> {
    let mut bytes = Box::new([0u8; 32]);
    rand::rngs::SysRng.try_fill_bytes(bytes.as_mut()).context("no randomness for a key")?;
    Ok(SecretBox::new(bytes))
}

fn read(path: &std::path::Path) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    Ok(Zeroizing::new(std::fs::read(path).with_context(|| format!("reading {}: run `xtask keygen`", path.display()))?))
}

fn read_32(path: &std::path::Path) -> anyhow::Result<SecretBox<[u8; 32]>> {
    let bytes = read(path)?;
    if bytes.len() != 32 {
        bail!("{} is not 32 bytes", path.display());
    }
    let mut secret = SecretBox::new(Box::new([0u8; 32]));
    secret.expose_secret_mut().copy_from_slice(&bytes);
    Ok(secret)
}
