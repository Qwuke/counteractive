//! One test per rejection, including every attestation negative in SPEC section 10 step 2.

use manifest::{AttestationPolicy, BALANCE_WIDTH, CREDIT_SCHEME, Manifest, Platform, Release, Service};
use proto::Blob;
use serde_json::{Value, json};

use super::*;

const SPKI: [u8; 32] = [0x51; 32];
const NONCE: [u8; 32] = [0x4e; 32];

fn release_config() -> Value {
    json!({
        "schema": 1, "image_version": "0.1.0", "service_id": "counteractive-poc", "key_epoch": 1,
        "balance_width": 24, "operator_public_key": "RWQ", "k_sha256": "00".repeat(32),
        "products": [{"id": "usd-5", "usd_cents": 500, "credits": 5_000_000}],
        "storefront": {"port": 8443, "operator_token_sha256": "11".repeat(32)},
        "session_policy": {"maximum_spend": 500_000, "idle_seconds": 900, "max_turns": 64,
                           "max_age_seconds": 3600},
        "catalogue": [{"id": "fake-small", "engine": "fake", "usage_trust": "inside_attested_boundary",
                       "input_rate": 1, "output_rate": 3, "max_output": 4096, "context": 4096}]
    })
}

fn release(config: Value, dev_key: &DevAttestationKey) -> Release {
    Release {
        image_version: "0.1.0".into(),
        platform: "dev-insecure".into(),
        attestation_policy: AttestationPolicy {
            kind: "dev-insecure".into(),
            dev_public_key: Some(Blob::new(dev_key.public_key().to_vec())),
        },
        release_config_sha256: manifest::release_config_sha256(&config),
        release_config: config,
    }
}

fn manifest_with(releases: Vec<Release>) -> Manifest {
    Manifest {
        schema: 1,
        version: 7,
        service: Service {
            id: "counteractive-poc".into(),
            onion: None,
            credit_scheme: CREDIT_SCHEME.into(),
            issuer_public_key: Blob::new(vec![2; 33]),
            key_epoch: 1,
            balance_width: BALANCE_WIDTH,
        },
        releases,
    }
}

struct World {
    manifest: Manifest,
    source: DevInsecureSource,
}

fn world() -> World {
    let key = DevAttestationKey::generate().unwrap();
    let release = release(release_config(), &key);
    let sha = release.release_config_sha256;
    World { manifest: manifest_with(vec![release]), source: DevInsecureSource::new(key, sha) }
}

fn gate_binding() -> ReportBinding {
    ReportBinding::new(Role::Gate, SPKI, NONCE)
}

fn evidence_for(world: &World, binding: &ReportBinding) -> Evidence {
    world.source.evidence(binding).unwrap()
}

#[test]
fn matching_evidence_is_accepted() {
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    let verdict = DevInsecureVerifier.verify(&evidence, &gate_binding(), &w.manifest).unwrap();
    assert_eq!(verdict.role, Role::Gate);
    assert_eq!(verdict.platform, Platform::DevInsecure);
    assert!(verdict.is_insecure());
    assert_eq!(verdict.tls_spki_sha256, SPKI);
    assert_eq!(verdict.manifest_version, 7);
    assert_eq!(verdict.release_config.model("fake-small").unwrap().output_rate, 3);
    assert_eq!(evidence.report.len(), 64);
}

#[test]
fn storefront_evidence_cannot_stand_in_for_the_gate() {
    // SPEC 10 step 2: a verifier expecting role gate rejects the storefront's evidence.
    let w = world();
    let storefront = ReportBinding { role: Role::Storefront, ..gate_binding() };
    let evidence = evidence_for(&w, &storefront);
    assert_eq!(
        DevInsecureVerifier.verify(&evidence, &gate_binding(), &w.manifest),
        Err(Rejection::WrongRole { expected: Role::Gate, claimed: Role::Storefront })
    );
    // Relabelling the claimed role doesn't help: the role is inside the signed data.
    let relabelled = Evidence { role: Role::Gate, ..evidence };
    assert_eq!(DevInsecureVerifier.verify(&relabelled, &gate_binding(), &w.manifest), Err(Rejection::ReportMismatch));
}

#[test]
fn a_wrong_nonce_is_rejected() {
    // SPEC 10 step 2: a wrong nonce rejects (a replayed report).
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    let fresh = ReportBinding { wallet_nonce: [0x99; 32], ..gate_binding() };
    assert_eq!(DevInsecureVerifier.verify(&evidence, &fresh, &w.manifest), Err(Rejection::ReportMismatch));
}

#[test]
fn a_manifest_without_the_release_is_rejected() {
    // SPEC 10 step 2: a manifest without the matching release rejects.
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    let mut other_config = release_config();
    other_config["image_version"] = json!("0.2.0");
    let other = manifest_with(vec![release(other_config, &DevAttestationKey::generate().unwrap())]);
    assert_eq!(DevInsecureVerifier.verify(&evidence, &gate_binding(), &other), Err(Rejection::NoMatchingRelease));
}

#[test]
fn a_report_over_another_tls_key_is_rejected() {
    // A relay that terminates TLS itself presents a different SPKI.
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    let relayed = ReportBinding { tls_spki_sha256: [0x77; 32], ..gate_binding() };
    assert_eq!(DevInsecureVerifier.verify(&evidence, &relayed, &w.manifest), Err(Rejection::ReportMismatch));
}

#[test]
fn a_report_by_another_dev_key_is_rejected() {
    let w = world();
    let forger =
        DevInsecureSource::new(DevAttestationKey::generate().unwrap(), w.manifest.releases[0].release_config_sha256);
    let evidence = forger.evidence(&gate_binding()).unwrap();
    assert_eq!(DevInsecureVerifier.verify(&evidence, &gate_binding(), &w.manifest), Err(Rejection::ReportMismatch));
}

#[test]
fn policy_must_fit_the_platform() {
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    let mutations: [fn(&mut Release); 4] = [
        |r| r.attestation_policy.kind = "snp".into(),
        |r| r.attestation_policy.dev_public_key = None,
        |r| r.attestation_policy.dev_public_key = Some(Blob::new(vec![1; 31])),
        |r| r.platform = "snp-verda".into(),
    ];
    for mutate in mutations {
        let mut manifest = w.manifest.clone();
        mutate(&mut manifest.releases[0]);
        assert_eq!(DevInsecureVerifier.verify(&evidence, &gate_binding(), &manifest), Err(Rejection::PolicyMismatch));
    }
}

#[test]
fn gpu_evidence_must_match() {
    let w = world();
    let evidence = Evidence { gpu_evidence_sha256: [5; 32], ..evidence_for(&w, &gate_binding()) };
    assert_eq!(
        DevInsecureVerifier.verify(&evidence, &gate_binding(), &w.manifest),
        Err(Rejection::GpuEvidenceMismatch)
    );
}

#[test]
fn malformed_reports_and_platforms_are_rejected() {
    let w = world();
    let good = evidence_for(&w, &gate_binding());
    let short = Evidence { report: Blob::new(vec![0; 63]), ..good.clone() };
    assert_eq!(DevInsecureVerifier.verify(&short, &gate_binding(), &w.manifest), Err(Rejection::MalformedEvidence));
    let snp = Evidence { platform: "snp-verda".into(), ..good };
    assert_eq!(
        DevInsecureVerifier.verify(&snp, &gate_binding(), &w.manifest),
        Err(Rejection::WrongPlatform("snp-verda".into()))
    );
}

#[test]
fn an_invalid_release_config_is_rejected() {
    let key = DevAttestationKey::generate().unwrap();
    let mut config = release_config();
    config["catalogue"] = json!([]);
    let release = release(config, &key);
    let source = DevInsecureSource::new(key, release.release_config_sha256);
    let manifest = manifest_with(vec![release]);
    let evidence = source.evidence(&gate_binding()).unwrap();
    assert_eq!(DevInsecureVerifier.verify(&evidence, &gate_binding(), &manifest), Err(Rejection::ReleaseConfig));
}

#[test]
fn platform_dispatch() {
    let w = world();
    let evidence = evidence_for(&w, &gate_binding());
    PlatformVerifier::new(true).verify(&evidence, &gate_binding(), &w.manifest).unwrap();
    assert_eq!(
        PlatformVerifier::new(false).verify(&evidence, &gate_binding(), &w.manifest),
        Err(Rejection::DevAttestationNotAllowed)
    );
    let snp = Evidence { platform: "snp-verda".into(), ..evidence.clone() };
    assert_eq!(PlatformVerifier::new(true).verify(&snp, &gate_binding(), &w.manifest), Err(Rejection::Unsupported));
    assert_eq!(SnpVerifier.verify(&snp, &gate_binding(), &w.manifest), Err(Rejection::Unsupported));
    let unknown = Evidence { platform: "tdx".into(), ..evidence };
    assert_eq!(
        PlatformVerifier::new(true).verify(&unknown, &gate_binding(), &w.manifest),
        Err(Rejection::WrongPlatform("tdx".into()))
    );
}

#[test]
fn dev_keys_round_trip_and_stay_out_of_debug() {
    let key = DevAttestationKey::generate().unwrap();
    let again = DevAttestationKey::from_bytes(&key.to_bytes());
    assert_eq!(again.public_key(), key.public_key());
    assert_eq!(format!("{key:?}"), "DevAttestationKey([redacted])");
    let source = DevInsecureSource::new(key, [0; 32]);
    assert!(!format!("{source:?}").contains(&hex::encode(*again.to_bytes())));
    assert_eq!(source.platform(), Platform::DevInsecure);
}

#[test]
fn dev_key_files_read_raw_or_hex() {
    let seed = [0x5a; 32];
    let expected = DevAttestationKey::from_bytes(&seed).public_key();
    let read = |contents: &[u8]| DevAttestationKey::from_file_contents(contents).map(|key| key.public_key());
    assert_eq!(read(&seed), Ok(expected));
    assert_eq!(read(format!("{}\n", hex::encode(seed)).as_bytes()), Ok(expected));
    assert_eq!(read(hex::encode_upper(seed).as_bytes()), Ok(expected));
    assert_eq!(read(&[1; 31]), Err(DevKeyError));
    assert_eq!(read(hex::encode([1; 31]).as_bytes()), Err(DevKeyError));
    assert_eq!(read(&[b'z'; 64]), Err(DevKeyError));
}
