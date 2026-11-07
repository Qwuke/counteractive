//! Steps 5 to 7: chat and metering through walletd, close and reconcile,
//! and replayed spends.

use std::path::Path;

use anyhow::{Context as _, bail, ensure};
use credit::{Holder, IssuerPublicKey, ServiceContext};
use gate_conn::ApiRequest;
use proto::gate::{CloseResponse, OpenSessionRequest, OpenSessionResponse, RecoverRequest, RecoverResponse};
use proto::openai::ModelList;
use proto::walletd::SettlementOutcome;
use proto::{ErrorCode, Receipt, SecretBlob};
use wallet_core::{Journal, Pins, manifest_store};

use super::ok;
use super::purchase::expect_code;
use super::world::{ChatResult, World};
use crate::release::{FAKE_LARGE, FAKE_SMALL, KEY_EPOCH, MAXIMUM_SPEND, Rates, SERVICE_ID, USD_5_CREDITS};

/// The release's largest answer.
const MAX_OUTPUT: u64 = 4096;
/// Prompt tokens the fake engine counts per message on top of its words.
const TOKENS_PER_MESSAGE: u64 = 4;

/// Step 5: two streamed turns on `fake-small` and one on `fake-large` in
/// another conversation, each receipt checked against the fake engine's
/// counts and the release's rates.
///
/// One purchase is one chain, and concurrent conversations need
/// different chains (docs 04 section 7), so the second conversation first
/// gets the documented clear error, and runs once the first is closed.
pub async fn chat_and_metering(world: &mut World) -> anyhow::Result<()> {
    let addr = world.start_walletd().await?;
    let gate = Pins::read(&world.tree.wallet())?.gate_target;
    ok(format!("walletd up on {addr}, reaching the gate at {gate}"));

    let first = "describe the attested gate in a few plain words please";
    let turn = world.chat("small", "fake-small", first).await?;
    let receipt = check_turn(world, &turn, first, "fake-small", FAKE_SMALL, 1, MAXIMUM_SPEND)?;
    let second = "and now the wallet";
    let turn = world.chat("small", "fake-small", second).await?;
    check_turn(world, &turn, second, "fake-small", FAKE_SMALL, 2, receipt.remaining)?;

    let busy = world.chat_error("large", "fake-large", "one more").await?;
    ensure!(busy.contains("no_spendable_chain"), "a second conversation on the only chain said: {busy}");
    ok("a second conversation while the only chain is in a session: no_spendable_chain");
    let closed = world.close(Some("small")).await?;
    ensure!(closed.closed == 1 && closed.recovered == 0, "closing `small`: {closed:?}");
    ok("closed conversation `small`");

    let third = "summarise the storefront for me";
    let turn = world.chat("large", "fake-large", third).await?;
    check_turn(world, &turn, third, "fake-large", FAKE_LARGE, 1, MAXIMUM_SPEND)?;

    let models: ModelList = serde_json::from_str(&world.cli_ok(&["--json", "models"]).await?)?;
    let ids: Vec<_> = models.data.iter().map(|model| model.id.as_str()).collect();
    ensure!(ids.contains(&"fake-small") && ids.contains(&"fake-large"), "models: {ids:?}");
    ok(format!("wallet-cli models: {}", ids.join(", ")));
    Ok(())
}

/// Checks one turn against the fake engine's formulas (SPEC section 8)
/// and the model's rates, and counts its charge.
pub fn check_turn(
    world: &mut World,
    turn: &ChatResult,
    prompt: &str,
    model: &str,
    rates: Rates,
    number: u32,
    remaining_before: u64,
) -> anyhow::Result<Receipt> {
    let receipt = turn.receipt()?.clone();
    let Expected { prompt_tokens, completion_tokens, charge } = expected_turn(prompt, rates, remaining_before);
    ensure!(receipt.model == model, "receipt model {}", receipt.model);
    ensure!(receipt.turn == number, "receipt turn {} != {number}", receipt.turn);
    ensure!(
        u64::from(receipt.prompt_tokens) == prompt_tokens,
        "prompt tokens {} != {prompt_tokens}",
        receipt.prompt_tokens
    );
    ensure!(
        u64::from(receipt.completion_tokens) == completion_tokens,
        "completion tokens {} != {completion_tokens}",
        receipt.completion_tokens
    );
    let streamed = turn.text.split_whitespace().count() as u64;
    ensure!(streamed == completion_tokens, "streamed {streamed} words for {completion_tokens} completion tokens");
    ensure!(receipt.charge == charge, "charge {} != {charge}", receipt.charge);
    ensure!(
        receipt.remaining == remaining_before - charge,
        "remaining {} != {}",
        receipt.remaining,
        remaining_before - charge
    );
    world.spent += charge;
    ok(format!(
        "{model} turn {number}: {prompt_tokens} prompt + {completion_tokens} completion tokens streamed; \
         charge {prompt_tokens}x{} + {completion_tokens}x{} = {charge}; remaining {}",
        rates.input, rates.output, receipt.remaining
    ));
    Ok(receipt)
}

/// What one turn should count and cost.
pub struct Expected {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub charge: u64,
}

/// The fake engine's counts for a one-message `prompt` (SPEC section 8),
/// and their charge at `rates` in a session with `remaining_before` left.
pub fn expected_turn(prompt: &str, rates: Rates, remaining_before: u64) -> Expected {
    let words = prompt.split_whitespace().count() as u64;
    let prompt_tokens = TOKENS_PER_MESSAGE + words;
    let fits = (remaining_before - rates.input * prompt_tokens) / rates.output;
    let completion_tokens = MAX_OUTPUT.min(fits).min(16 + words % 17);
    let charge = rates.input * prompt_tokens + rates.output * completion_tokens;
    Expected { prompt_tokens, completion_tokens, charge }
}

/// Step 6: close, then the balance is exact and every session reconciles.
pub async fn close_and_reconcile(world: &World) -> anyhow::Result<()> {
    let closed = world.close(None).await?;
    ensure!(closed.closed == 1 && closed.recovered == 0, "closing all: {closed:?}");
    ok("wallet-cli close: the open session closed and its refund finalised");
    let total = world.check_balance().await?;
    ok(format!("balance {total} = 5,000,000 - {} charged, exactly", world.spent));
    let report = world.reconcile().await?;
    ensure!(report.sessions.len() == 2, "expected 2 settled sessions, got {:?}", report.sessions);
    ensure!(
        report.sessions.iter().all(|session| session.outcome == SettlementOutcome::Exact
            && session.receipts_sum == session.s - session.t),
        "sessions: {:?}",
        report.sessions
    );
    ok("wallet-cli reconcile: both settled sessions are exact (receipts = s - t)");
    Ok(())
}

/// Step 7: a settled spend replayed to open gets `409 use_recover` and
/// recover returns the same refund; a second proof from the same
/// credential, made from an old copy of the journal, gets `409 spent`.
pub async fn replay(world: &mut World) -> anyhow::Result<()> {
    let credential = world.credential.take().context("step 4's credential is gone")?;
    let issuer = IssuerPublicKey::from_bytes(world.manifest.service.issuer_public_key.as_bytes())?;
    let holder = Holder::new(&ServiceContext::new(SERVICE_ID, KEY_EPOCH)?, &issuer)?;
    let (spend_state, spend) = holder.prove_spend(credential, 50_000)?;
    let mut gate = world.gate_conn().await?;
    let open = |spend: &[u8]| {
        ApiRequest::post("/v1/sessions").json(&OpenSessionRequest { spend: SecretBlob::new(spend.to_vec()) })
    };
    let OpenSessionResponse::Open { session_id, .. } = gate.send_json(open(&spend)?).await? else {
        bail!("the spend did not open a session");
    };
    let closed: CloseResponse =
        gate.send_json(ApiRequest::post(format!("/v1/sessions/{}/close", session_id.to_hex()))).await?;
    expect_code(gate.send_json::<OpenSessionResponse>(open(&spend)?).await, 409, ErrorCode::UseRecover)?;
    ok("a settled session's spend bytes sent to open again: 409 use_recover");
    let recover =
        ApiRequest::post("/v1/sessions/recover").json(&RecoverRequest { spend: SecretBlob::new(spend.clone()) })?;
    let RecoverResponse::Settled { refund } = gate.send_json(recover).await? else {
        bail!("recover did not report the session settled");
    };
    ensure!(refund.as_bytes() == closed.refund.as_bytes(), "recover returned a different refund");
    let restored = holder.finalize_refund(spend_state, refund.as_bytes())?;
    ensure!(restored.balance() == USD_5_CREDITS, "the refund restores {}", restored.balance());
    ok("recover returned the byte-identical refund, which finalises to the full 5,000,000");

    world.stop_walletd().await?;
    let copy = world.tree.root().join("wallet-copy");
    copy_files(&world.tree.wallet(), &copy)?;
    world.start_walletd().await?;
    ok("journal copied while chain 1 was Ready (walletd stopped for a consistent copy)");
    let prompt = "spend from the live journal";
    let turn = world.chat("replay", "fake-small", prompt).await?;
    check_turn(world, &turn, prompt, "fake-small", FAKE_SMALL, 1, MAXIMUM_SPEND)?;

    let pins = Pins::read(&copy)?;
    let mut journal = Journal::open(&copy)?;
    let manifest = manifest_store::require_accepted(&journal, &pins.accept_pins()?)?;
    let old_holder = manifest_store::holder(&pins, &manifest)?;
    let second = wallet_core::chain::prepare_spend(&mut journal, &old_holder, &pins.tier_ladder, MAXIMUM_SPEND, 0)?
        .context("the copied journal has no Ready chain")?;
    // A new connection: the first may have sat idle past the gate's header
    // read timeout while walletd restarted and the turn ran, which behind
    // Tor takes long enough for it.
    let mut gate = world.gate_conn().await?;
    expect_code(gate.send_json::<OpenSessionResponse>(open(second.message.expose())?).await, 409, ErrorCode::Spent)?;
    ok("a second proof from the same credential, made from the copied journal: 409 spent");
    let closed = world.close(Some("replay")).await?;
    ensure!(closed.closed == 1, "closing `replay`: {closed:?}");
    world.check_balance().await?;
    ok("the live wallet is unaffected: its session closed and the balance is exact");
    Ok(())
}

/// Copies the regular files of a wallet directory, keeping their modes.
fn copy_files(from: &Path, to: &Path) -> anyhow::Result<()> {
    crate::tree::create_private_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}
