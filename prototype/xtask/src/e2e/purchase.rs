//! Steps 2 to 4: attestation negatives, purchase, issuance idempotency.

use anyhow::{Context as _, anyhow, bail, ensure};
use attest::{AttestationVerifier as _, PlatformVerifier, ReportBinding, Role};
use credit::{Holder, IssuerPublicKey, ServiceContext};
use gate_conn::{ApiRequest, AttestFailure, ConnError, DialScope, TcpDialer, UnverifiedConn};
use manifest::OperatorSecretKey;
use proto::storefront::{CollectRequest, CollectResponse, CreateOrderRequest, CreateOrderResponse};
use proto::{Blob, ErrorCode, SecretHex32};
use rand::TryRng as _;
use sha2::{Digest as _, Sha256};
use wallet_core::manifest_store::{self, Offered};
use wallet_core::{Journal, Pins};

use super::ok;
use super::world::World;
use crate::operator;
use crate::release::{KEY_EPOCH, SERVICE_ID, USD_5_CREDITS};

/// Step 2: evidence for the wrong role, an old nonce, a manifest without
/// the running release, and a manifest from another signer all fail.
pub async fn attestation_negatives(world: &World) -> anyhow::Result<()> {
    let verifier = PlatformVerifier::new(true);

    let conn = UnverifiedConn::connect(&TcpDialer, &world.storefront_target, Role::Gate, &DialScope::new()).await?;
    expect_rejected(conn.attest(&verifier, &world.manifest).await, "the storefront's evidence checked as the gate")?;

    let mut conn = UnverifiedConn::connect(
        &gate_conn::UnixDialer,
        &operator::gate_target(&world.tree),
        Role::Gate,
        &DialScope::new(),
    )
    .await?;
    let sent = random();
    let evidence = conn.fetch_evidence(sent).await?;
    let binding = |nonce| ReportBinding::new(Role::Gate, conn.spki_sha256(), nonce);
    verifier.verify(&evidence, &binding(sent), &world.manifest).map_err(|error| anyhow!("control failed: {error}"))?;
    match verifier.verify(&evidence, &binding(random()), &world.manifest) {
        Err(rejection) => ok(format!("gate evidence for one nonce checked against another: rejected ({rejection})")),
        Ok(_) => bail!("evidence for a different nonce was accepted"),
    }

    let running = release_sha256(&world.tree.release())?;
    let mut without = world.manifest.clone();
    without.releases.retain(|release| release.release_config_sha256 != running);
    ensure!(without.releases.len() == 1, "expected the idle release to remain");
    let conn = UnverifiedConn::connect(
        &gate_conn::UnixDialer,
        &operator::gate_target(&world.tree),
        Role::Gate,
        &DialScope::new(),
    )
    .await?;
    expect_rejected(conn.attest(&verifier, &without).await, "the gate against a manifest without its release")?;

    let pins = Pins::read(&world.tree.wallet())?;
    let scratch = world.tree.root().join("scratch-wallet");
    std::fs::create_dir(&scratch)?;
    let mut journal = Journal::create(&scratch)?;
    let stranger = OperatorSecretKey::generate()?;
    let forged = manifest::sign_manifest(&world.manifest, &stranger)?;
    match manifest_store::offer(&mut journal, &pins.accept_pins()?, &forged) {
        Err(error) => ok(format!("the wallet refuses the manifest signed by another key ({error})")),
        Ok(_) => bail!("the wallet accepted a manifest signed by another key"),
    }
    let genuine = operator::read_manifest(&world.tree)?.context("no manifest")?;
    ensure!(
        matches!(manifest_store::offer(&mut journal, &pins.accept_pins()?, &genuine)?, Offered::Pending(_)),
        "control: the operator's own manifest should be offered for approval"
    );
    ok("control: the same manifest signed by the pinned operator key is offered for approval");
    Ok(())
}

fn expect_rejected<T>(result: Result<T, AttestFailure>, what: &str) -> anyhow::Result<()> {
    match result {
        Err(AttestFailure::Rejected(rejection)) => {
            ok(format!("{what}: rejected ({rejection})"));
            Ok(())
        }
        Err(other) => bail!("{what}: failed for another reason: {other}"),
        Ok(_) => bail!("{what}: accepted"),
    }
}

fn release_sha256(path: &std::path::Path) -> anyhow::Result<[u8; 32]> {
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(manifest::release_config_sha256(&config))
}

/// Step 3: the wallet buys `usd-5`, can't collect before paying, pays
/// through fake-stripe and collects a Ready chain of 5,000,000 credits.
pub async fn purchase(world: &World) -> anyhow::Result<()> {
    world.cli_ok(&["init"]).await?;
    ok("wallet-cli init: journal and key created");
    let fetched = world.cli_ok(&["manifest", "fetch", "--approve"]).await?;
    ensure!(fetched.contains("accepted manifest 1"), "manifest fetch said: {fetched}");
    ok("wallet-cli manifest fetch --approve: manifest 1 accepted");

    let bought = world.cli_ok(&["buy", "--product", "usd-5", "--no-browser"]).await?;
    let order_id = line_value(&bought, "order_id: ").context("buy printed no order id")?;
    let checkout = line_value(&bought, "checkout_url: ").context("buy printed no checkout URL")?;
    ok(format!("wallet-cli buy --product usd-5 --no-browser: order {order_id}, checkout {checkout}"));

    let early = world.cli(&["collect"], None).await?;
    ensure!(early.code == Some(2), "collect before paying exited {:?}: {}", early.code, early.stderr);
    ensure!(early.stdout.contains("402 unpaid"), "collect before paying said: {}", early.stdout);
    ok("wallet-cli collect before paying: 402 unpaid, exit 2, order still awaiting payment");

    world.pay(&order_id).await?;
    ok("paid through fake-stripe's /test/pay");
    let collected = world.cli_ok(&["collect"]).await?;
    ensure!(collected.contains("chain 1 holds 5000000 credits (Ready)"), "collect after paying said: {collected}");
    ok("wallet-cli collect: chain 1 is Ready with 5,000,000 credits");
    Ok(())
}

/// The rest of the first line of `text` that starts with `prefix`.
pub fn line_value(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| line.strip_prefix(prefix)).map(|value| value.trim().to_owned())
}

/// Step 4: a second order, collected by xtask with its own holder: the
/// same request returns the same bytes, another request for the same
/// order gets `409 order_issued`, and a wrong secret gets `401`. The
/// credential it yields is step 7's.
pub async fn issuance_idempotency(world: &mut World) -> anyhow::Result<()> {
    let issuer = IssuerPublicKey::from_bytes(world.manifest.service.issuer_public_key.as_bytes())?;
    let holder = Holder::new(&ServiceContext::new(SERVICE_ID, KEY_EPOCH)?, &issuer)?;
    let mut storefront = world.storefront_conn().await?;

    let secret: [u8; 32] = random();
    let order: CreateOrderResponse = storefront
        .send_json(
            ApiRequest::post("/v1/orders")
                .json(&CreateOrderRequest { product: "usd-5".into(), secret_hash: Sha256::digest(secret).into() })?,
        )
        .await?;
    world.pay(&order.order_id).await?;
    let (state, request) = holder.issue_request()?;
    let collect = |secret: [u8; 32], request: &[u8]| {
        ApiRequest::post(format!("/v1/orders/{}/collect", order.order_id)).json(&CollectRequest {
            order_secret: SecretHex32::new(secret),
            issue_request: Blob::new(request.to_vec()),
        })
    };

    let first: CollectResponse = storefront.send_json(collect(secret, &request)?).await?;
    let again: CollectResponse = storefront.send_json(collect(secret, &request)?).await?;
    ensure!(first.issue_response == again.issue_response, "re-collecting returned different bytes");
    ensure!(first.credits == USD_5_CREDITS && again.credits == USD_5_CREDITS, "wrong credits");
    ok(format!("re-collecting order {} with the same request returned identical bytes", order.order_id));

    let (_, other_request) = holder.issue_request()?;
    expect_code(
        storefront.send_json::<CollectResponse>(collect(secret, &other_request)?).await,
        409,
        ErrorCode::OrderIssued,
    )?;
    ok("a different issue request for the same order: 409 order_issued");
    expect_code(
        storefront.send_json::<CollectResponse>(collect(random(), &request)?).await,
        401,
        ErrorCode::Unauthorized,
    )?;
    ok("the right request with a wrong order secret: 401");

    let credential = holder.finalize_issue(state, first.issue_response.as_bytes())?;
    ensure!(credential.balance() == USD_5_CREDITS, "the credential holds {}", credential.balance());
    ok("the response finalises to a credential of 5,000,000 credits (kept for step 7)");
    world.credential = Some(credential);
    Ok(())
}

/// Requires an API error with this status and code.
pub fn expect_code<T>(result: Result<T, ConnError>, status: u16, code: ErrorCode) -> anyhow::Result<()> {
    match result {
        Err(ConnError::Api { status: got, error: Some(body) }) if got == status && body.error == code => Ok(()),
        Err(other) => bail!("expected {status} {code}, got: {other}"),
        Ok(_) => bail!("expected {status} {code}, but the call succeeded"),
    }
}

/// Random bytes from the operating system.
pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    rand::rngs::SysRng.try_fill_bytes(&mut bytes).expect("the OS random number generator failed");
    bytes
}
