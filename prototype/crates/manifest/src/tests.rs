//! Signing, verification and both accept rules.

use proto::Blob;
use serde_json::json;

use super::*;
use crate::release::tests::sample_value;

fn dev_release(config: serde_json::Value) -> Release {
    Release {
        image_version: "0.1.0".into(),
        platform: "dev-insecure".into(),
        attestation_policy: AttestationPolicy {
            kind: "dev-insecure".into(),
            dev_public_key: Some(Blob::new(vec![7; 32])),
        },
        release_config_sha256: release_config_sha256(&config),
        release_config: config,
    }
}

fn manifest(version: u64) -> Manifest {
    Manifest {
        schema: SCHEMA,
        version,
        service: Service {
            id: "counteractive-poc".into(),
            onion: None,
            credit_scheme: CREDIT_SCHEME.into(),
            issuer_public_key: Blob::new(vec![2; 33]),
            key_epoch: 1,
            balance_width: BALANCE_WIDTH,
        },
        releases: vec![dev_release(sample_value())],
    }
}

fn pins(key: &OperatorSecretKey) -> AcceptPins {
    AcceptPins {
        operator_public_key: key.public_key().unwrap(),
        service_id: "counteractive-poc".into(),
        key_epoch: 1,
        onion: None,
        allow_dev_attestation: true,
    }
}

#[test]
fn signed_manifests_verify_and_round_trip() {
    let key = OperatorSecretKey::generate().unwrap();
    let original = manifest(3);
    let signed = sign_manifest(&original, &key).unwrap();
    assert!(is_canonical(signed.manifest.as_bytes()));
    assert!(signed.signature.contains("trusted comment: counteractive manifest"));
    assert_eq!(verify_signed(&signed, &key.public_key().unwrap()).unwrap(), original);
}

#[test]
fn keys_round_trip_through_text_and_never_debug() {
    let key = OperatorSecretKey::generate().unwrap();
    let text = key.to_text().unwrap();
    let again = OperatorSecretKey::from_text(secrecy::ExposeSecret::expose_secret(&text)).unwrap();
    assert_eq!(again.public_key().unwrap(), key.public_key().unwrap());
    assert_eq!(format!("{key:?}"), "OperatorSecretKey([redacted])");
    let public = key.public_key().unwrap();
    assert_eq!(OperatorPublicKey::from_base64(public.as_base64()).unwrap(), public);
    assert_eq!(OperatorPublicKey::from_base64("nonsense").unwrap_err(), ManifestError::OperatorKey);
}

#[test]
fn signature_checks_reject() {
    let key = OperatorSecretKey::generate().unwrap();
    let other = OperatorSecretKey::generate().unwrap();
    let signed = sign_manifest(&manifest(1), &key).unwrap();

    let other_key = other.public_key().unwrap();
    assert_eq!(verify_signed(&signed, &other_key).unwrap_err(), ManifestError::Signature);

    let mut tampered = signed.clone();
    let mut bytes = tampered.manifest.into_vec();
    let at = bytes.iter().position(|&b| b == b'3').unwrap();
    bytes[at] = b'4';
    tampered.manifest = Blob::new(bytes);
    assert_eq!(verify_signed(&tampered, &key.public_key().unwrap()).unwrap_err(), ManifestError::Signature);

    let garbled = SignedManifest { signature: "not minisign".into(), ..signed };
    assert_eq!(verify_signed(&garbled, &key.public_key().unwrap()).unwrap_err(), ManifestError::Signature);
}

#[test]
fn non_canonical_bytes_are_refused_even_when_signed() {
    let key = OperatorSecretKey::generate().unwrap();
    let pretty = serde_json::to_vec_pretty(&manifest(1)).unwrap();
    let signed = SignedManifest { signature: key.sign_bytes(&pretty, "pretty").unwrap(), manifest: Blob::new(pretty) };
    assert_eq!(verify_signed(&signed, &key.public_key().unwrap()).unwrap_err(), ManifestError::NotCanonical);
}

#[test]
fn unknown_schema_is_refused() {
    let key = OperatorSecretKey::generate().unwrap();
    let mut value = serde_json::to_value(manifest(1)).unwrap();
    value["schema"] = json!(2);
    let bytes = canonical_json(&value);
    let signed = SignedManifest { signature: key.sign_bytes(&bytes, "v2").unwrap(), manifest: Blob::new(bytes) };
    assert_eq!(verify_signed(&signed, &key.public_key().unwrap()).unwrap_err(), ManifestError::UnknownSchema(2));
}

#[test]
fn wallet_accepts_a_matching_manifest() {
    let key = OperatorSecretKey::generate().unwrap();
    let signed = sign_manifest(&manifest(5), &key).unwrap();
    let accepted = accept(&signed, &pins(&key), Some(5)).unwrap();
    assert_eq!(accepted.version, 5);
    let summary = accepted.summary();
    assert_eq!(summary.platforms, vec!["dev-insecure".to_owned()]);
    let release = &accepted.releases[0];
    assert_eq!(accepted.release_by_config_sha256(&release.release_config_sha256), Some(release));
    assert_eq!(release.config().unwrap().service_id, "counteractive-poc");
}

#[test]
fn wallet_rejections() {
    let key = OperatorSecretKey::generate().unwrap();
    let pins = pins(&key);
    let check = |m: Manifest, pins: &AcceptPins, highest: Option<u64>| {
        accept(&sign_manifest(&m, &key).unwrap(), pins, highest).unwrap_err()
    };

    assert_eq!(check(manifest(4), &pins, Some(5)), ManifestError::VersionRollback { accepted: 5, offered: 4 });

    let mut m = manifest(1);
    m.service.id = "elsewhere".into();
    assert_eq!(check(m, &pins, None), ManifestError::ServiceMismatch);

    let mut m = manifest(1);
    m.service.key_epoch = 2;
    assert_eq!(check(m, &pins, None), ManifestError::EpochMismatch);

    let onion_pins = AcceptPins { onion: Some("abc.onion".into()), ..pins.clone() };
    assert_eq!(check(manifest(1), &onion_pins, None), ManifestError::OnionMismatch);
    let mut m = manifest(1);
    m.service.onion = Some("abc.onion".into());
    accept(&sign_manifest(&m, &key).unwrap(), &onion_pins, None).unwrap();

    let mut m = manifest(1);
    m.service.credit_scheme = "blind-rsa".into();
    assert_eq!(check(m, &pins, None), ManifestError::UnknownCreditScheme);

    let mut m = manifest(1);
    m.service.issuer_public_key = Blob::default();
    assert_eq!(check(m, &pins, None), ManifestError::MissingIssuerKey);

    let mut m = manifest(1);
    m.releases[0].release_config["image_version"] = json!("0.2.0");
    assert_eq!(check(m, &pins, None), ManifestError::ConfigHashMismatch);

    let mut m = manifest(1);
    m.releases[0].attestation_policy.kind = "snp".into();
    assert_eq!(check(m, &pins, None), ManifestError::PolicyMismatch);

    let strict = AcceptPins { allow_dev_attestation: false, ..pins.clone() };
    assert_eq!(check(manifest(1), &strict, None), ManifestError::NoUsableRelease);

    let other = OperatorSecretKey::generate().unwrap();
    let foreign = sign_manifest(&manifest(1), &other).unwrap();
    assert_eq!(accept(&foreign, &pins, None).unwrap_err(), ManifestError::Signature);
}

#[test]
fn unknown_platforms_are_skipped_not_fatal() {
    let key = OperatorSecretKey::generate().unwrap();
    let mut m = manifest(1);
    let mut future = dev_release(json!({"anything": true}));
    future.platform = "tdx-somewhere".into();
    future.release_config_sha256 = [0; 32];
    m.releases.insert(0, future);
    accept(&sign_manifest(&m, &key).unwrap(), &pins(&key), None).unwrap();

    m.releases.truncate(1);
    assert_eq!(
        accept(&sign_manifest(&m, &key).unwrap(), &pins(&key), None).unwrap_err(),
        ManifestError::NoUsableRelease
    );
}

#[test]
fn gate_rule() {
    let key = OperatorSecretKey::generate().unwrap();
    let public = key.public_key().unwrap();
    let own_sha = release_config_sha256(&sample_value());
    let issuer = vec![2; 33];
    let pins = GatePins {
        operator_public_key: &public,
        service_id: "counteractive-poc",
        issuer_public_key: &issuer,
        key_epoch: 1,
        stored_version: 2,
        release_config_sha256: &own_sha,
    };
    let signed = sign_manifest(&manifest(2), &key).unwrap();
    assert_eq!(gate_accept(&signed, &pins).unwrap().version, 2);

    let older = sign_manifest(&manifest(1), &key).unwrap();
    assert!(matches!(gate_accept(&older, &pins), Err(ManifestError::VersionRollback { .. })));

    let other_issuer = vec![3; 33];
    let wrong_issuer = GatePins { issuer_public_key: &other_issuer, ..pins.clone() };
    assert_eq!(gate_accept(&signed, &wrong_issuer).unwrap_err(), ManifestError::IssuerKeyMismatch);

    let wrong_epoch = GatePins { key_epoch: 2, ..pins.clone() };
    assert_eq!(gate_accept(&signed, &wrong_epoch).unwrap_err(), ManifestError::EpochMismatch);

    let elsewhere = [9; 32];
    let wrong_release = GatePins { release_config_sha256: &elsewhere, ..pins.clone() };
    assert_eq!(gate_accept(&signed, &wrong_release).unwrap_err(), ManifestError::NoMatchingRelease);

    let stranger = OperatorSecretKey::generate().unwrap().public_key().unwrap();
    let wrong_key = GatePins { operator_public_key: &stranger, ..pins };
    assert_eq!(gate_accept(&signed, &wrong_key).unwrap_err(), ManifestError::Signature);
}
