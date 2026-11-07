//! `release.json`: the per-image config the gate and storefront read, and
//! that a manifest release carries as `release_config` (docs 07 section 6).
//!
//! Prices, products and models live here, inside what's attested, so
//! "what am I being charged" is part of the release a wallet checked.

use std::collections::HashSet;

use proto::SessionPolicy;
use proto::encoding::hex_array;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ManifestError;
use crate::canonical::release_config_sha256;
use crate::types::BALANCE_WIDTH;

/// The `release.json` schema this build reads.
pub const RELEASE_SCHEMA: u32 = 1;

/// A whole `release.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseConfig {
    /// Must be [`RELEASE_SCHEMA`].
    pub schema: u32,
    /// The image version.
    pub image_version: String,
    /// The service id the contexts derive from.
    pub service_id: String,
    /// The issuer key epoch the contexts derive from.
    pub key_epoch: u32,
    /// Credential balance width; must be 24.
    pub balance_width: u8,
    /// The minisign key that signs manifests, in base64.
    pub operator_public_key: String,
    /// `SHA-256(K)`, which unlock compares in constant time.
    #[serde(with = "hex_array")]
    pub k_sha256: [u8; 32],
    /// What can be bought.
    pub products: Vec<Product>,
    /// The storefront's settings.
    pub storefront: StorefrontSettings,
    /// Session limits.
    pub session_policy: SessionPolicy,
    /// The models served, with their rates.
    pub catalogue: Vec<CatalogueEntry>,
}

/// One product: a price and the credits it buys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    /// Product id, such as `usd-5`.
    pub id: String,
    /// Price in US cents, sent to Stripe inline.
    pub usd_cents: u64,
    /// Credits issued for it.
    pub credits: u64,
}

/// The storefront's part of `release.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorefrontSettings {
    /// TCP port it listens on.
    pub port: u16,
    /// `SHA-256(token)` for `POST /v1/operator/configure`.
    #[serde(with = "hex_array")]
    pub operator_token_sha256: [u8; 32],
}

/// One model and its prices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueEntry {
    /// Model id clients ask for.
    pub id: String,
    /// The backend that serves it: `vllm`, `llamacpp` or `fake`.
    pub engine: String,
    /// How far its usage counts can be trusted, such as `inside_attested_boundary`.
    pub usage_trust: String,
    /// Credits per prompt token.
    pub input_rate: u64,
    /// Credits per output token; at least 1.
    pub output_rate: u64,
    /// Largest output a turn may ask for.
    pub max_output: u32,
    /// The model's context window in tokens.
    pub context: u32,
}

impl ReleaseConfig {
    /// Parses `release.json` bytes in any formatting, returning the config
    /// and the SHA-256 of its canonical form.
    pub fn parse(bytes: &[u8]) -> Result<(Self, [u8; 32]), ManifestError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| ManifestError::Malformed)?;
        let config = Self::from_value(&value)?;
        Ok((config, release_config_sha256(&value)))
    }

    /// Parses and validates a config from a JSON value.
    pub fn from_value(value: &Value) -> Result<Self, ManifestError> {
        let config = Self::deserialize(value).map_err(|_| ManifestError::Malformed)?;
        config.validate()?;
        Ok(config)
    }

    /// Checks the rules every service relies on.
    pub fn validate(&self) -> Result<(), ManifestError> {
        let invalid = ManifestError::InvalidReleaseConfig;
        let below_width = |amount: u64| amount < (1 << BALANCE_WIDTH);
        if self.schema != RELEASE_SCHEMA {
            return Err(ManifestError::UnknownSchema(self.schema));
        }
        if self.balance_width != BALANCE_WIDTH {
            return Err(invalid("balance_width must be 24"));
        }
        if self.service_id.is_empty() || self.service_id.len() > 255 {
            return Err(invalid("service_id must be 1 to 255 bytes"));
        }
        if self.products.is_empty() || !unique(self.products.iter().map(|p| p.id.as_str())) {
            return Err(invalid("products must be non-empty with unique ids"));
        }
        if self.products.iter().any(|p| p.usd_cents == 0 || p.credits == 0 || !below_width(p.credits)) {
            return Err(invalid("a product's price and credits must be positive and fit the balance width"));
        }
        let policy = &self.session_policy;
        if policy.maximum_spend == 0 || !below_width(policy.maximum_spend) || policy.max_turns == 0 {
            return Err(invalid("session_policy needs a positive maximum_spend within the width and max_turns"));
        }
        if self.catalogue.is_empty() || !unique(self.catalogue.iter().map(|m| m.id.as_str())) {
            return Err(invalid("catalogue must be non-empty with unique ids"));
        }
        if self.catalogue.iter().any(|m| m.output_rate == 0 || m.max_output == 0) {
            return Err(invalid("a model's output_rate and max_output must be at least 1"));
        }
        Ok(())
    }

    /// The product with this id.
    pub fn product(&self, id: &str) -> Option<&Product> {
        self.products.iter().find(|product| product.id == id)
    }

    /// Whether some product carries exactly these credits; the gate's issuance check.
    pub fn has_product_with_credits(&self, credits: u64) -> bool {
        self.products.iter().any(|product| product.credits == credits)
    }

    /// The catalogue entry for this model id.
    pub fn model(&self, id: &str) -> Option<&CatalogueEntry> {
        self.catalogue.iter().find(|model| model.id == id)
    }
}

fn unique<'a>(ids: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    ids.into_iter().all(|id| seen.insert(id))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// The dev release from SPEC section 9.
    pub(crate) fn sample_value() -> Value {
        json!({
            "schema": 1,
            "image_version": "0.1.0",
            "service_id": "counteractive-poc",
            "key_epoch": 1,
            "balance_width": 24,
            "operator_public_key": "RWQ",
            "k_sha256": "00".repeat(32),
            "products": [
                {"id": "usd-5", "usd_cents": 500, "credits": 5_000_000},
                {"id": "usd-1", "usd_cents": 100, "credits": 1_000_000}
            ],
            "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
            "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64,
                               "max_age_seconds": 3600},
            "catalogue": [
                {"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                 "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096},
                {"id": "fake-large", "engine": "fake", "usage_trust": "inside_attested_boundary",
                 "input_rate": 4, "output_rate": 12, "max_output": 4096, "context": 4096}
            ]
        })
    }

    #[test]
    fn parses_any_formatting_with_one_hash() {
        let value = sample_value();
        let pretty = serde_json::to_vec_pretty(&value).unwrap();
        let compact = serde_json::to_vec(&value).unwrap();
        let (config, hash) = ReleaseConfig::parse(&pretty).unwrap();
        assert_eq!(ReleaseConfig::parse(&compact).unwrap().1, hash);
        assert_eq!(hash, release_config_sha256(&value));
        assert_eq!(config.product("usd-5").unwrap().credits, 5_000_000);
        assert!(config.has_product_with_credits(1_000_000));
        assert!(!config.has_product_with_credits(999));
        assert_eq!(config.model("fake-large").unwrap().output_rate, 12);
        assert!(config.model("gpt").is_none());
    }

    #[test]
    fn validation_rejects_bad_configs() {
        let cases: [(&str, Value); 6] = [
            ("/balance_width", json!(32)),
            ("/products", json!([])),
            ("/products/0/credits", json!(16_777_216u64)),
            ("/catalogue/1/id", json!("fake-small")),
            ("/catalogue/0/output_rate", json!(0)),
            ("/session_policy/maximum_spend", json!(0)),
        ];
        for (pointer, bad) in cases {
            let mut value = sample_value();
            *value.pointer_mut(pointer).unwrap() = bad;
            assert!(
                matches!(ReleaseConfig::from_value(&value), Err(ManifestError::InvalidReleaseConfig(_))),
                "{pointer}"
            );
        }
        let mut value = sample_value();
        value["schema"] = json!(2);
        assert_eq!(ReleaseConfig::from_value(&value), Err(ManifestError::UnknownSchema(2)));
        assert_eq!(ReleaseConfig::parse(b"{"), Err(ManifestError::Malformed));
    }
}
