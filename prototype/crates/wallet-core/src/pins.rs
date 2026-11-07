//! The values a wallet release compiles in (docs 04 section 1), read from
//! `wallet.toml` in the prototype (SPEC section 7).
//!
//! Everything that decides which service a wallet trusts comes from here,
//! never from a fetched manifest: a hostile signer could set a manifest's
//! fields to anything. Reading them through this one type lets a release
//! build swap the file for constants.

use std::path::Path;

use credit::ServiceContext;
use manifest::{AcceptPins, OperatorPublicKey};
use serde::{Deserialize, Serialize};

use crate::WalletError;

/// The pins file's name inside the wallet directory.
pub const PINS_FILE: &str = "wallet.toml";

/// A wallet's pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    /// The service id the credential contexts derive from.
    pub service_id: String,
    /// The issuer key epoch the credential contexts derive from.
    pub key_epoch: u32,
    /// Where `walletd` dials the gate: `unix:/path` (tier 1) or
    /// `onion:host.onion:443` (tier 2).
    pub gate_target: String,
    /// The storefront's `host:port`, dialed by `wallet-cli` over TCP.
    pub storefront_addr: String,
    /// The minisign key that signs manifests, in base64.
    pub operator_public_key: String,
    /// Session sizes in credits, largest first.
    pub tier_ladder: Vec<u64>,
    /// Whether `dev-insecure` releases are acceptable. Never true in a real release.
    pub allow_dev_attestation: bool,
}

impl Pins {
    /// Parses and checks `wallet.toml` text.
    pub fn parse(text: &str) -> Result<Self, WalletError> {
        let pins: Self = toml::from_str(text).map_err(|error| WalletError::Pins(error.message().to_owned()))?;
        pins.check()?;
        Ok(pins)
    }

    /// Reads `wallet.toml` from a wallet directory.
    pub fn read(wallet_dir: &Path) -> Result<Self, WalletError> {
        let path = wallet_dir.join(PINS_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| WalletError::File { path, source })?;
        Self::parse(&text)
    }

    /// The pins as `wallet.toml` text.
    pub fn to_toml(&self) -> Result<String, WalletError> {
        toml::to_string(self).map_err(|error| WalletError::Pins(error.to_string()))
    }

    /// What a manifest must match to be accepted.
    pub fn accept_pins(&self) -> Result<AcceptPins, WalletError> {
        Ok(AcceptPins {
            operator_public_key: self.operator_key()?,
            service_id: self.service_id.clone(),
            key_epoch: self.key_epoch,
            onion: self.onion().map(str::to_owned),
            allow_dev_attestation: self.allow_dev_attestation,
        })
    }

    /// The credential contexts, derived from the pinned service id and epoch only.
    pub fn service_context(&self) -> Result<ServiceContext, WalletError> {
        Ok(ServiceContext::new(&self.service_id, self.key_epoch)?)
    }

    /// The onion host the gate target names, if it is an onion service.
    /// Tier 1 dials a Unix socket and pins no onion.
    pub fn onion(&self) -> Option<&str> {
        let rest = self.gate_target.strip_prefix("onion:")?;
        rest.rsplit_once(':').map(|(host, _port)| host)
    }

    fn operator_key(&self) -> Result<OperatorPublicKey, WalletError> {
        OperatorPublicKey::from_base64(&self.operator_public_key)
            .map_err(|_| WalletError::Pins("operator_public_key is not a minisign public key".into()))
    }

    fn check(&self) -> Result<(), WalletError> {
        self.operator_key()?;
        self.service_context()?;
        let ladder = &self.tier_ladder;
        if ladder.is_empty() || ladder.contains(&0) || ladder.windows(2).any(|pair| pair[0] <= pair[1]) {
            return Err(WalletError::Pins("tier_ladder must be positive and strictly descending".into()));
        }
        if self.storefront_addr.is_empty() || self.gate_target.is_empty() {
            return Err(WalletError::Pins("gate_target and storefront_addr are required".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A real minisign public key, so the pins parse.
    pub(crate) const OPERATOR_KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";

    pub(crate) fn pins(gate_target: &str) -> Pins {
        Pins {
            service_id: "counteractive-dev".into(),
            key_epoch: 1,
            gate_target: gate_target.into(),
            storefront_addr: "127.0.0.1:8443".into(),
            operator_public_key: OPERATOR_KEY.into(),
            tier_ladder: vec![500_000, 50_000, 5_000],
            allow_dev_attestation: true,
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let pins = pins("unix:/tmp/gate.sock");
        assert_eq!(Pins::parse(&pins.to_toml().unwrap()).unwrap(), pins);
    }

    #[test]
    fn onion_comes_from_the_gate_target() {
        assert_eq!(pins("unix:/tmp/gate.sock").onion(), None);
        assert_eq!(pins("onion:abc.onion:443").onion(), Some("abc.onion"));
        assert_eq!(pins("onion:abc.onion:443").accept_pins().unwrap().onion.as_deref(), Some("abc.onion"));
    }

    #[test]
    fn rejects_a_bad_ladder_and_unknown_fields() {
        let mut bad = pins("unix:/tmp/gate.sock");
        bad.tier_ladder = vec![5_000, 50_000];
        Pins::parse(&bad.to_toml().unwrap()).unwrap_err();
        let extra = format!("{}\nsurprise = 1\n", pins("unix:/g").to_toml().unwrap());
        Pins::parse(&extra).unwrap_err();
    }
}
