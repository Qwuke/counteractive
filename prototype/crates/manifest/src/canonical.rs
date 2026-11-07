//! Canonical JSON: sorted object keys, no whitespace (SPEC section 2).
//!
//! Signatures and `release_config_sha256` are over these bytes. Keys are
//! sorted here explicitly rather than by relying on `serde_json`'s default
//! map, so the result stays canonical even if another crate in the graph
//! turns on `serde_json/preserve_order`.

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

/// The canonical bytes of a JSON value.
pub fn canonical_json(value: &Value) -> Vec<u8> {
    serde_json::to_vec(&sorted(value)).expect("a JSON value always serialises: keys are strings and numbers are finite")
}

/// The canonical bytes of any serialisable value.
pub fn to_canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    Ok(canonical_json(&serde_json::to_value(value)?))
}

/// Whether `bytes` are exactly the canonical encoding of the JSON they hold.
pub fn is_canonical(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| canonical_json(&value) == bytes)
}

/// SHA-256 of a release config's canonical bytes: the value a report binds
/// and a manifest release is selected by.
pub fn release_config_sha256(config: &Value) -> [u8; 32] {
    Sha256::digest(canonical_json(config)).into()
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), sorted(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_sort_recursively_without_whitespace() {
        let value = json!({"b": 1, "a": {"z": [3, {"y": true, "x": null}], "c": "s"}});
        assert_eq!(
            String::from_utf8(canonical_json(&value)).unwrap(),
            r#"{"a":{"c":"s","z":[3,{"x":null,"y":true}]},"b":1}"#
        );
    }

    #[test]
    fn canonical_check_rejects_spacing_and_order() {
        assert!(is_canonical(br#"{"a":1,"b":[1,2]}"#));
        assert!(!is_canonical(br#"{"b":1,"a":2}"#));
        assert!(!is_canonical(br#"{"a": 1}"#));
        assert!(!is_canonical(b"{\"a\":1}\n"));
        assert!(!is_canonical(b"not json"));
    }

    #[test]
    fn hash_ignores_formatting_of_the_input() {
        let pretty: Value = serde_json::from_str("{\n  \"y\": 2,\n  \"x\": 1\n}").unwrap();
        let compact: Value = serde_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
        assert_eq!(release_config_sha256(&pretty), release_config_sha256(&compact));
        assert_eq!(
            hex::encode(release_config_sha256(&compact)),
            // SHA-256 of the 13 bytes {"x":1,"y":2}
            hex::encode(Sha256::digest(br#"{"x":1,"y":2}"#))
        );
    }
}
