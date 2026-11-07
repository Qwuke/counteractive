//! Accepted and pending manifests (docs 04 section 6).
//!
//! A fetched manifest that passes the accept rule goes to `pending`; only
//! the user's approval moves it to `accepted`. That includes a new
//! wallet's first manifest, which has nothing to compare against.

use credit::{Holder, IssuerPublicKey};
use manifest::{AcceptPins, Manifest, Platform, ReleaseConfig, SignedManifest};
use proto::walletd::ManifestStatus;

use crate::journal::{Journal, ManifestSlot};
use crate::{Pins, WalletError};

/// What offering a fetched manifest did.
#[derive(Debug, Clone, PartialEq)]
pub enum Offered {
    /// It is the manifest already accepted; nothing to approve.
    AlreadyAccepted(Manifest),
    /// It passed the accept rule and waits for approval.
    Pending(Manifest),
}

/// The accepted manifest, checked again against the pins on every load so
/// a changed `wallet.toml` can't keep using a manifest it wouldn't accept.
pub fn accepted(journal: &Journal, pins: &AcceptPins) -> Result<Option<Manifest>, WalletError> {
    load(journal, ManifestSlot::Accepted, pins)
}

/// Like [`accepted`], but an error when none is.
pub fn require_accepted(journal: &Journal, pins: &AcceptPins) -> Result<Manifest, WalletError> {
    accepted(journal, pins)?.ok_or(WalletError::NoAcceptedManifest)
}

/// Runs the accept rule on a fetched manifest and, unless it is the one
/// already accepted, stores it as pending.
pub fn offer(journal: &mut Journal, pins: &AcceptPins, signed: &SignedManifest) -> Result<Offered, WalletError> {
    let current = journal.manifest(ManifestSlot::Accepted)?;
    let manifest = manifest::accept(signed, pins, current.as_ref().map(|stored| stored.version))?;
    if current.is_some_and(|stored| stored.signed.manifest.as_bytes() == signed.manifest.as_bytes()) {
        journal.discard_pending_manifest()?;
        return Ok(Offered::AlreadyAccepted(manifest));
    }
    journal.set_pending_manifest(manifest.version, signed)?;
    Ok(Offered::Pending(manifest))
}

/// The user's approval: re-checks the pending manifest against the pins
/// and the accepted version, then makes it the accepted one.
pub fn approve(journal: &mut Journal, pins: &AcceptPins) -> Result<Manifest, WalletError> {
    let pending = journal.manifest(ManifestSlot::Pending)?.ok_or(WalletError::NoPendingManifest)?;
    let highest = journal.manifest(ManifestSlot::Accepted)?.map(|stored| stored.version);
    let manifest = manifest::accept(&pending.signed, pins, highest)?;
    journal.approve_pending_manifest(manifest.version)?;
    Ok(manifest)
}

/// Both slots, summarised for display.
pub fn status(journal: &Journal, pins: &AcceptPins) -> Result<ManifestStatus, WalletError> {
    Ok(ManifestStatus {
        accepted: accepted(journal, pins)?.map(|manifest| manifest.summary()),
        pending: load(journal, ManifestSlot::Pending, pins)?.map(|manifest| manifest.summary()),
    })
}

/// The wallet's side of the credit scheme for this manifest's issuer key,
/// with contexts derived from the pins only.
pub fn holder(pins: &Pins, manifest: &Manifest) -> Result<Holder, WalletError> {
    let issuer = IssuerPublicKey::from_bytes(manifest.service.issuer_public_key.as_bytes())?;
    Ok(Holder::new(&pins.service_context()?, &issuer)?)
}

/// The configs of the releases this wallet can talk to: a known platform,
/// and `dev-insecure` only when the pins allow it.
pub fn usable_configs(manifest: &Manifest, allow_dev_attestation: bool) -> Vec<(Platform, ReleaseConfig)> {
    manifest
        .releases
        .iter()
        .filter_map(|release| {
            let platform = release.platform()?;
            if platform == Platform::DevInsecure && !allow_dev_attestation {
                return None;
            }
            Some((platform, release.config().ok()?))
        })
        .collect()
}

fn load(journal: &Journal, slot: ManifestSlot, pins: &AcceptPins) -> Result<Option<Manifest>, WalletError> {
    journal.manifest(slot)?.map(|stored| Ok(manifest::accept(&stored.signed, pins, None)?)).transpose()
}
