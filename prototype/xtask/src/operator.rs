//! `xtask unlock`: the operator's side of a boot (docs 07 section 7).
//!
//! The gate is unlocked with K over an attested connection. On its first
//! unlock it answers with the issuer key it just generated, which the
//! operator signs into the manifest and posts back. Then the storefront
//! gets its Stripe key, also over an attested connection.

use anyhow::{Context as _, bail};
use attest::{PlatformVerifier, Role};
use gate_conn::{ApiRequest, AttestedConn, DialScope, Target, TcpDialer, UnixDialer, UnverifiedConn};
use manifest::{Manifest, SignedManifest};
use proto::gate::{GateHealth, GateState, StateResponse, UnlockResponse};
use proto::storefront::{ConfigureRequest, ConfigureResponse, StorefrontHealth, StorefrontState};
use secrecy::SecretString;

use crate::keys::Keys;
use crate::release;
use crate::tree::Tree;

/// An issuer key for attesting the gate before its first unlock. Evidence
/// doesn't depend on the issuer key, which the operator doesn't know yet.
const PLACEHOLDER_ISSUER_KEY: [u8; 33] = [2; 33];

/// The gate's state from `GET /v1/health`, over an unattested connection.
pub async fn gate_health(tree: &Tree) -> anyhow::Result<GateState> {
    let mut conn = UnverifiedConn::connect(&UnixDialer, &gate_target(tree), Role::Gate, &DialScope::new()).await?;
    Ok(conn.health::<GateHealth>().await?.state)
}

/// The storefront's state from `GET /v1/health`.
pub async fn storefront_health(storefront: &Target) -> anyhow::Result<StorefrontState> {
    let mut conn = UnverifiedConn::connect(&TcpDialer, storefront, Role::Storefront, &DialScope::new()).await?;
    Ok(conn.health::<StorefrontHealth>().await?.state)
}

/// The gate's dial target.
pub fn gate_target(tree: &Tree) -> Target {
    Target::Unix(tree.gate_socket())
}

/// An attested connection to the gate, checked against `manifest`.
pub async fn attest_gate(tree: &Tree, manifest: &Manifest) -> anyhow::Result<AttestedConn> {
    let conn = UnverifiedConn::connect(&UnixDialer, &gate_target(tree), Role::Gate, &DialScope::new()).await?;
    Ok(conn.attest(&PlatformVerifier::new(true), manifest).await?)
}

/// Unlocks the gate if it is locked and, on a first unlock, signs and
/// posts the manifest. Returns the manifest the gate is serving under.
pub async fn unlock_gate(tree: &Tree, keys: &Keys) -> anyhow::Result<SignedManifest> {
    if gate_health(tree).await? == GateState::Ready {
        return read_manifest(tree)?.context("the gate is ready but the tree has no manifest");
    }
    let bootstrap = release::manifest(tree, keys, &PLACEHOLDER_ISSUER_KEY, 1)?;
    let mut gate = attest_gate(tree, &bootstrap).await.context("attesting the gate")?;
    let unlocked: UnlockResponse = gate
        .send_json(ApiRequest::post("/v1/operator/unlock").bearer(keys.k_token()))
        .await
        .context("unlocking the gate")?;
    let issuer_public_key = unlocked.issuer_public_key.as_bytes();
    match unlocked.state {
        GateState::Ready => {
            // A restart: the store already records an accepted manifest.
            read_manifest(tree)?.context("the gate is ready but the tree has no manifest")
        }
        GateState::AwaitingManifest => {
            let signed = manifest_for(tree, keys, issuer_public_key)?;
            let request = ApiRequest::post("/v1/operator/manifest").json(&signed)?.bearer(keys.k_token());
            let posted: StateResponse = gate.send_json(request).await.context("posting the manifest")?;
            if posted.state != GateState::Ready {
                bail!("the gate is {:?} after taking the manifest", posted.state);
            }
            Ok(signed)
        }
        GateState::Locked => bail!("the gate is still locked after unlocking"),
    }
}

/// The manifest to post for this issuer key: the tree's own if it already
/// names this key and these releases, otherwise a new version, signed.
fn manifest_for(tree: &Tree, keys: &Keys, issuer_public_key: &[u8]) -> anyhow::Result<SignedManifest> {
    let operator = keys.operator_public_key()?;
    let previous = read_manifest(tree)?
        .map(|signed| manifest::verify_signed(&signed, &operator).map(|manifest| (signed, manifest)))
        .transpose()?;
    let wanted = release::manifest(tree, keys, issuer_public_key, 1)?;
    match previous {
        Some((signed, manifest)) if Manifest { version: 1, ..manifest.clone() } == wanted => Ok(signed),
        Some((_, manifest)) => {
            release::sign_and_write(tree, keys, &Manifest { version: manifest.version + 1, ..wanted })
        }
        None => release::sign_and_write(tree, keys, &wanted),
    }
}

/// The signed manifest in the tree.
pub fn read_manifest(tree: &Tree) -> anyhow::Result<Option<SignedManifest>> {
    release::read_signed(tree)
}

/// Gives the storefront its Stripe key over an attested connection.
pub async fn configure_storefront(
    tree: &Tree,
    keys: &Keys,
    storefront: &Target,
    stripe_key: SecretString,
) -> anyhow::Result<()> {
    let signed = read_manifest(tree)?.context("no manifest yet: unlock the gate first")?;
    let manifest = manifest::verify_signed(&signed, &keys.operator_public_key()?)?;
    let conn = UnverifiedConn::connect(&TcpDialer, storefront, Role::Storefront, &DialScope::new()).await?;
    let mut conn = conn.attest(&PlatformVerifier::new(true), &manifest).await.context("attesting the storefront")?;
    let request = ApiRequest::post("/v1/operator/configure")
        .json(&ConfigureRequest { stripe_key })?
        .bearer(keys.storefront_bearer());
    let configured: ConfigureResponse = conn.send_json(request).await.context("configuring the storefront")?;
    if configured.state != StorefrontState::Ready {
        bail!("the storefront is {:?} after configuring", configured.state);
    }
    Ok(())
}
