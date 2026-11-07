//! Step 13: kill tests (docs 03 section 5.6).
//!
//! Each `crash_point!` in the gate, wallet-cli and walletd is armed in
//! turn, in crash-point builds of the real binaries. The process must die
//! there by SIGABRT. After a restart, the invariants must hold:
//!
//! - every retry resends the journalled bytes, never fresh ones;
//! - the gate answers those bytes as its recover table says (docs 05
//!   section 9), the same bytes every time;
//! - each chain ends Ready, and reconcile classifies its session;
//! - the balance is exactly the purchase minus the charges the gate
//!   committed, counted from the receipts.
//!
//! The crash-point builds have a target directory and a stack of their
//! own, so steps 1 to 12 never run a binary with a crash point; this step
//! first checks that they don't. Each test buys into a fresh wallet, so
//! each starts from a new order and a new chain.
//!
//! The wallet commits a collected credential with its purchase, and a
//! settled credential with its classified session, in one journal
//! transaction each. So there is no point between finalising and deleting
//! state to crash at.

use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use credit::{Holder, IssuerPublicKey, ServiceContext};
use gate_conn::{ApiRequest, AttestedConn};
use proto::gate::{GateState, OperatorStateResponse, RecoverRequest, RecoverResponse};
use proto::storefront::{CollectRequest, CollectResponse};
use proto::walletd::{BalanceView, ChainState, PurchaseState, ReconcileReport, SettledSessionView, SettlementOutcome};
use proto::{Blob, ErrorCode, SecretBlob, SecretHex32};
use secrecy::ExposeSecret as _;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt as _;
use wallet_core::pins::PINS_FILE;
use wallet_core::{ChainId, Journal, PurchaseRow};

use super::ok;
use super::purchase::{expect_code, line_value};
use super::sessions::expected_turn;
use super::toolchain::tree;
use super::world::{ChatResult, World};
use crate::process::{Binaries, SERVICE_CRASH_POINTS, SERVICES, Service, WALLETD_CRASH_POINTS};
use crate::release::{FAKE_SMALL, KEY_EPOCH, MAXIMUM_SPEND, SERVICE_ID, USD_5_CREDITS};
use crate::tree::{Tree, create_private_dir};
use crate::{operator, stack};

/// The signal `std::process::abort` raises: SIGABRT, 6 on every Unix.
const SIGABRT: i32 = 6;
/// How long an armed process may take to die. On macOS the crash reporter
/// holds a dying process while it writes its report.
const ABORT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a restarted walletd may take to settle what a crash left.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the balance is read while waiting for that.
const SETTLE_POLL: Duration = Duration::from_millis(250);
/// The model, conversation and prompt of a test's one turn.
const MODEL: &str = "fake-small";
const CONVERSATION: &str = "kill";
const PROMPT: &str = "one turn before the crash";
/// What `cargo tree -e features` shows when crash points are compiled in.
const ENABLED: &str = "crash-point feature \"enabled\"";
/// How many lines of a dead process's log an error quotes.
const LOG_TAIL: usize = 5;

/// Step 13: checks the binaries the other steps run carry no crash point,
/// builds the crash-point binaries, brings up a stack of them, and runs one
/// kill test per crash point.
pub async fn run(binaries: &Binaries) -> anyhow::Result<()> {
    let crash = Binaries::build_with_crash_points()?;
    no_crash_points_in(binaries, &crash)?;
    let world = World::bring_up(&crash, None).await.context("bringing up the crash-point stack")?;
    let issuer = IssuerPublicKey::from_bytes(world.manifest.service.issuer_public_key.as_bytes())?;
    let holder = Holder::new(&ServiceContext::new(SERVICE_ID, KEY_EPOCH)?, &issuer)?;
    let mut kill = Kill { binaries: crash, world, holder };
    let result = kill.run_all().await;
    if let Some(dir) = kill.world.finish(result.is_ok()).await {
        println!("\nkill-test logs and state kept in {}", dir.display());
    }
    result
}

/// Requires the binaries steps 1 to 12 run to carry no crash point. The
/// crash-point builds are the control for both checks:
///
/// - cargo resolves `crash-point` without `enabled` for the packages
///   [`Binaries::build`] builds together, and for walletd on its own;
/// - the built gate, walletd and wallet-cli don't contain the name of the
///   variable that arms a crash point, which only `enabled` code reads.
fn no_crash_points_in(normal: &Binaries, crash: &Binaries) -> anyhow::Result<()> {
    let services: Vec<&str> = SERVICES.iter().flat_map(|package| ["-p", package]).collect();
    let builds =
        [(services, SERVICE_CRASH_POINTS.as_slice()), (vec!["-p", "walletd"], WALLETD_CRASH_POINTS.as_slice())];
    for (packages, features) in builds {
        let features = features.join(",");
        let mut args = packages;
        args.extend(["-e", "features", "-i", "crash-point"]);
        ensure!(!tree(&args)?.contains(ENABLED), "cargo tree {}: crash points are compiled in", args.join(" "));
        let normal = args.join(" ");
        args.extend(["--features", &features]);
        ensure!(tree(&args)?.contains(ENABLED), "control: cargo tree {} shows no {ENABLED}", args.join(" "));
        ok(format!("cargo tree {normal}: no {ENABLED} (control: --features {features} shows it)"));
    }
    for name in ["gate", "walletd", "wallet-cli"] {
        ensure!(!holds(&normal.path(name), crash_point::ENV_VAR)?, "{name} can read {}", crash_point::ENV_VAR);
        ensure!(holds(&crash.path(name), crash_point::ENV_VAR)?, "control: the crash-point {name} never reads it");
    }
    ok(format!(
        "the gate, walletd and wallet-cli that steps 1 to 12 run don't contain {}; their crash-point builds do",
        crash_point::ENV_VAR
    ));
    Ok(())
}

/// Whether the file at `path` contains `text`.
fn holds(path: &Path, text: &str) -> anyhow::Result<bool> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(bytes.windows(text.len()).any(|window| window == text.as_bytes()))
}

/// The crash-point stack, and a holder to read the refunds the gate returns.
struct Kill {
    binaries: Binaries,
    world: World,
    holder: Holder,
}

impl Kill {
    /// Every kill test, in order; the first failure ends the run.
    async fn run_all(&mut self) -> anyhow::Result<()> {
        after_issue_commit(self).await.context("after_issue_commit")?;
        after_insert_pending(self).await.context("after_insert_pending")?;
        before_settle(self).await.context("before_settle")?;
        after_settle(self).await.context("after_settle")?;
        lost_order(self, "after_journal_purchase").await.context("after_journal_purchase")?;
        lost_order(self, "after_create_order").await.context("after_create_order")?;
        after_finalize_issue(self).await.context("after_finalize_issue")?;
        spend_in_flight(self, "after_journal_spend", 0).await.context("after_journal_spend")?;
        spend_in_flight(self, "after_open_response", 1).await.context("after_open_response")?;
        before_finalize_refund(self).await.context("before_finalize_refund")?;
        Ok(())
    }

    /// Restarts the gate on its data dir with `point` armed, and unlocks it.
    async fn arm_gate(&mut self, point: &'static str) -> anyhow::Result<Service> {
        self.world.stop_gate(false).await?;
        let (tree, release) = (&self.world.tree, self.world.tree.release());
        let gate = stack::gate(&self.binaries.arming("gate", point), tree, &release).await?;
        operator::unlock_gate(tree, &self.world.keys).await?;
        ensure!(operator::gate_health(tree).await? == GateState::Ready, "the armed gate is not ready");
        Ok(gate)
    }

    /// Requires the armed gate to die at `point`, then starts it again,
    /// unarmed, on the same data dir and unlocks it.
    async fn gate_died(&mut self, gate: Service, point: &str) -> anyhow::Result<()> {
        let (status, log) = gate.exited(ABORT_TIMEOUT).await?;
        aborted_at("the gate", status, &log, point)?;
        let release = self.world.tree.release();
        self.world.start_gate(&release).await?;
        check(point, "the gate died by SIGABRT there; restarted unarmed on the same data dir and unlocked: Ready");
        Ok(())
    }

    /// Recovers `spend` from the gate twice, as a wallet that lost the
    /// answer would, and requires the same settled refund both times.
    /// Returns it with its `t`.
    async fn recover(&self, spend: &SecretBlob) -> anyhow::Result<(Vec<u8>, u64)> {
        let mut gate = self.world.gate_conn().await?;
        let first = recover_once(&mut gate, spend).await?;
        let second = recover_once(&mut gate, spend).await?;
        ensure!(first == second, "recover returned different refunds for the same spend");
        let t = self.holder.refund_amount(&first)?;
        Ok((first, t))
    }

    /// Sessions the gate holds a slot for.
    async fn live_sessions(&self) -> anyhow::Result<u64> {
        let mut gate = self.world.gate_conn().await?;
        let request = ApiRequest::get("/v1/operator/state").bearer(self.world.keys.k_token());
        let state: OperatorStateResponse = gate.send_json(request).await?;
        Ok(state.live_sessions)
    }
}

async fn recover_once(gate: &mut AttestedConn, spend: &SecretBlob) -> anyhow::Result<Vec<u8>> {
    let request = ApiRequest::post("/v1/sessions/recover").json(&RecoverRequest { spend: spend.clone() })?;
    match gate.send_json(request).await? {
        RecoverResponse::Settled { refund } => Ok(refund.as_bytes().to_vec()),
        RecoverResponse::Live { .. } => bail!("the gate still holds a live session for the spend"),
    }
}

/// Gate, `after_issue_commit`: the issuance is committed but its answer
/// never leaves. After a restart, the journalled request re-collects
/// identical bytes, another request gets `409 order_issued` (the commit
/// survived), and the wallet's retry ends with exactly one chain.
async fn after_issue_commit(kill: &mut Kill) -> anyhow::Result<()> {
    let point = "after_issue_commit";
    let mut wallet = Wallet::new(kill, point).await?;
    let order_id = wallet.buy().await?;
    kill.world.pay(&order_id).await?;
    let gate = kill.arm_gate(point).await?;
    let lost = wallet.cli(&["collect"], None, false).await?;
    ensure!(
        lost.status.code() == Some(2)
            && lost.stdout.contains("503")
            && lost.stdout.contains("will retry with the same request"),
        "collect while the gate died ended with {}: {}{}",
        lost.status,
        lost.stdout,
        lost.stderr
    );
    check(point, "wallet-cli collect: the storefront answered 503; the order waits (exit 2) to retry the same request");
    kill.gate_died(gate, point).await?;

    let row = wallet.purchase()?;
    ensure!(row.state == PurchaseState::AwaitingPayment, "the purchase is {row:?}");
    ensure!(wallet.journal()?.chains()?.is_empty(), "a chain exists though no issuance reached the wallet");
    let request = row.issue_request.clone().context("the purchase has no journalled request")?;
    let mut storefront = kill.world.storefront_conn().await?;
    let collect = |request: &[u8]| {
        ApiRequest::post(format!("/v1/orders/{order_id}/collect")).json(&CollectRequest {
            order_secret: SecretHex32::new(*row.order_secret.expose_secret()),
            issue_request: Blob::new(request.to_vec()),
        })
    };
    let first: CollectResponse = storefront.send_json(collect(&request)?).await?;
    let again: CollectResponse = storefront.send_json(collect(&request)?).await?;
    ensure!(first.issue_response == again.issue_response, "re-collecting returned different bytes");
    let (_, other) = kill.holder.issue_request()?;
    expect_code(storefront.send_json::<CollectResponse>(collect(&other)?).await, 409, ErrorCode::OrderIssued)?;
    check(
        point,
        "the journalled request re-collects identical bytes twice; another request gets 409 order_issued: \
         the commit survived",
    );
    wallet.collect_one_chain().await?;
    wallet.start_walletd(false).await?;
    wallet.check_balance(USD_5_CREDITS, "one order, one chain").await?;
    wallet.stop().await
}

/// Gate, `after_insert_pending`: the Pending row is durable but the
/// session never opened, so the wallet's chain stays Spending. After a
/// restart reconcile settles it with `t = s` (open answers `use_recover`,
/// and recover refunds a Pending row with no slot in full): the crash
/// costs nothing.
async fn after_insert_pending(kill: &mut Kill) -> anyhow::Result<()> {
    let point = "after_insert_pending";
    let mut wallet = Wallet::funded(kill, point).await?;
    wallet.start_walletd(false).await?;
    let gate = kill.arm_gate(point).await?;
    wallet.chat_fails().await?;
    check(point, "wallet-cli chat failed (exit 1): the gate died opening the session");
    kill.gate_died(gate, point).await?;

    let spend = wallet.unsettled_spend(ChainState::Spending)?;
    ensure!(kill.live_sessions().await? == 0, "the restarted gate holds a live session");
    check(point, "chain 1 is journalled Spending with its spend; the restarted gate holds no session");
    let report = wallet.reconcile().await?;
    ensure!(report.recovered == 1, "reconcile recovered {} sessions", report.recovered);
    expect_session(&report, MAXIMUM_SPEND, 0, SettlementOutcome::Exact)?;
    check(point, format!("wallet-cli reconcile settled it: t = s = {MAXIMUM_SPEND} with no receipts, exact"));
    let (_, t) = kill.recover(&spend).await?;
    ensure!(t == MAXIMUM_SPEND, "recover refunds {t}, not s");
    check(point, "recover with the journalled spend returns the identical refund twice, of t = s");
    wallet.check_balance(USD_5_CREDITS, "the crashed open cost nothing").await?;
    wallet.stop().await
}

/// Gate, `before_settle`: the refund is sealed in memory but never
/// committed, so the row stays Pending. After a restart the wallet's
/// reconcile gets `t = s`, a full refund of the turn too, recover agrees
/// byte for byte, and the balance is the whole purchase.
async fn before_settle(kill: &mut Kill) -> anyhow::Result<()> {
    let point = "before_settle";
    let (wallet, spend, charge) = close_into_gate_crash(kill, point).await?;
    let report = wallet.reconcile().await?;
    ensure!(report.recovered == 1, "reconcile recovered {} sessions", report.recovered);
    expect_session(&report, MAXIMUM_SPEND, charge, SettlementOutcome::FullRefund)?;
    check(
        point,
        format!("wallet-cli reconcile settled it: t = s = {MAXIMUM_SPEND} against receipts of {charge}, full_refund"),
    );
    let (_, t) = kill.recover(&spend).await?;
    ensure!(t == MAXIMUM_SPEND, "recover refunds {t}, not s");
    check(point, "recover with the journalled spend returns the identical refund twice, of t = s");
    wallet.check_balance(USD_5_CREDITS, "the uncommitted session cost nothing, its turn included").await?;
    wallet.stop().await
}

/// Gate, `after_settle`: the settle committed but the close's answer is
/// lost. Recover returns the committed refund, the same bytes before and
/// after the wallet settles from it; the wallet's reconcile classifies it
/// exact, and the balance drops by exactly the receipt.
async fn after_settle(kill: &mut Kill) -> anyhow::Result<()> {
    let point = "after_settle";
    let (wallet, spend, charge) = close_into_gate_crash(kill, point).await?;
    let (committed, t) = kill.recover(&spend).await?;
    ensure!(t == MAXIMUM_SPEND - charge, "recover refunds {t}, not s - {charge}");
    check(point, format!("recover returns the committed refund twice, identical: t = s - {charge} = {t}"));
    let report = wallet.reconcile().await?;
    ensure!(report.recovered == 1, "reconcile recovered {} sessions", report.recovered);
    expect_session(&report, t, charge, SettlementOutcome::Exact)?;
    check(point, format!("wallet-cli reconcile settled it: t = {t} with receipts of {charge}, exact"));
    let (again, _) = kill.recover(&spend).await?;
    ensure!(again == committed, "recover returned different bytes after the wallet settled");
    check(point, "after the wallet settled, recover still returns the same bytes");
    wallet.check_balance(USD_5_CREDITS - charge, &format!("the purchase minus the receipt's {charge}")).await?;
    wallet.stop().await
}

/// A funded wallet runs one turn through walletd and asks it to close the
/// session while the gate is armed at `point`, in its settle. Returns the
/// wallet, chain 1's journalled spend and the turn's charge.
async fn close_into_gate_crash(kill: &mut Kill, point: &'static str) -> anyhow::Result<(Wallet, SecretBlob, u64)> {
    let mut wallet = Wallet::funded(kill, point).await?;
    wallet.start_walletd(false).await?;
    let gate = kill.arm_gate(point).await?;
    let charge = wallet.chat().await?;
    wallet.close_fails().await?;
    check(point, "wallet-cli close failed (exit 1): the gate died settling the session");
    kill.gate_died(gate, point).await?;
    let spend = wallet.unsettled_spend(ChainState::Closing)?;
    ensure!(wallet.receipts()? == [charge], "the journal holds receipts {:?}, not {charge}", wallet.receipts()?);
    check(point, format!("chain 1 is journalled Closing with its spend and the turn's receipt of {charge}"));
    Ok((wallet, spend, charge))
}

/// wallet-cli, `after_journal_purchase` and `after_create_order`: buy dies
/// after journalling the purchase, before or after the storefront creates
/// the order, so no order id is journalled and no checkout URL shown.
/// `collect` sends the order again from the same journalled purchase
/// (same secret, same issue request), and paying and collecting then
/// yield exactly one chain. An order the crash left at the storefront was
/// never shown, so it can't be paid.
async fn lost_order(kill: &Kill, point: &'static str) -> anyhow::Result<()> {
    let mut wallet = Wallet::new(kill, point).await?;
    let crashed = wallet.cli_crash(&["buy", "--product", "usd-5", "--no-browser"]).await?;
    ensure!(!crashed.stdout.contains("checkout_url"), "buy showed a checkout URL before dying: {}", crashed.stdout);
    check(point, "wallet-cli buy died by SIGABRT there, before showing a checkout URL");
    let before = wallet.purchase()?;
    ensure!(before.state == PurchaseState::Ordered && before.order_id.is_none(), "the purchase is {before:?}");
    check(point, "the journal holds the purchase Ordered, with no order id");

    let resent = wallet.cli(&["collect"], None, false).await?;
    ensure!(resent.status.code() == Some(2), "collect ended with {}: {}", resent.status, resent.stderr);
    let order_id = line_value(&resent.stdout, "order_id: ").context("collect printed no order id")?;
    ensure!(line_value(&resent.stdout, "checkout_url: ").is_some(), "collect printed no checkout URL");
    let after = wallet.purchase()?;
    ensure!(
        after.id == before.id
            && after.state == PurchaseState::AwaitingPayment
            && after.order_id.as_deref() == Some(order_id.as_str()),
        "after collect the purchase is {after:?}"
    );
    ensure!(
        after.order_secret.expose_secret() == before.order_secret.expose_secret()
            && after.issue_request == before.issue_request,
        "the retry made a new order secret or issue request"
    );
    check(
        point,
        format!(
            "wallet-cli collect sent the order again from the same purchase, secret and request: order {order_id}, \
             exit 2 until paid"
        ),
    );
    kill.world.pay(&order_id).await?;
    wallet.collect_one_chain().await?;
    wallet.start_walletd(false).await?;
    wallet.check_balance(USD_5_CREDITS, "one purchase, one chain").await?;
    wallet.stop().await
}

/// wallet-cli, `after_finalize_issue`: collect dies with the credential
/// finalised in memory only. The purchase is still `AwaitingPayment` with
/// its journalled request, and there is no chain. The retry must send
/// that request, since another would get `409` and freeze the purchase,
/// and it ends with exactly one chain.
async fn after_finalize_issue(kill: &Kill) -> anyhow::Result<()> {
    let point = "after_finalize_issue";
    let mut wallet = Wallet::new(kill, point).await?;
    let order_id = wallet.buy().await?;
    kill.world.pay(&order_id).await?;
    let before = wallet.purchase()?;
    let crashed = wallet.cli_crash(&["collect"]).await?;
    ensure!(!crashed.stdout.contains("collected"), "collect reported a chain before dying: {}", crashed.stdout);
    check(point, "wallet-cli collect died by SIGABRT there, with the credential finalised in memory");
    let after = wallet.purchase()?;
    ensure!(
        after.state == PurchaseState::AwaitingPayment
            && after.issue_state.is_some()
            && after.issue_request == before.issue_request,
        "after the crash the purchase is {after:?}"
    );
    ensure!(wallet.journal()?.chains()?.is_empty(), "a chain was journalled before the crash");
    check(point, "the journal still holds the purchase AwaitingPayment with the same request and state, and no chain");
    wallet.collect_one_chain().await?;
    wallet.start_walletd(false).await?;
    wallet.check_balance(USD_5_CREDITS, "one purchase, one chain").await?;
    wallet.stop().await
}

/// walletd, `after_journal_spend` and `after_open_response`: walletd dies
/// with the spend journalled, either before sending it or before
/// journalling the session the gate opened for it (`gate_holds` is how
/// many sessions the gate then holds). After a restart its startup
/// reconcile resends the journalled bytes; the gate opens them, or returns
/// the session it already holds for them. walletd closes it and settles
/// `t = s`. Recover with the bytes journalled before the crash returns
/// that refund, so the gate settled those bytes, not a new proof.
async fn spend_in_flight(kill: &Kill, point: &'static str, gate_holds: u64) -> anyhow::Result<()> {
    let mut wallet = Wallet::funded(kill, point).await?;
    wallet.start_walletd(true).await?;
    wallet.chat_fails().await?;
    wallet.walletd_died().await?;
    let spend = wallet.unsettled_spend(ChainState::Spending)?;
    let live = kill.live_sessions().await?;
    ensure!(live == gate_holds, "the gate holds {live} live sessions, not {gate_holds}");
    check(point, format!("chain 1 is journalled Spending with its spend; the gate holds {live} session(s)"));

    wallet.start_walletd(false).await?;
    wallet.wait_ready().await?;
    ensure!(kill.live_sessions().await? == 0, "the gate still holds a session");
    check(point, "walletd restarted unarmed; its startup reconcile settled chain 1, and the gate holds no session");
    let report = wallet.reconcile().await?;
    expect_session(&report, MAXIMUM_SPEND, 0, SettlementOutcome::Exact)?;
    check(point, format!("wallet-cli reconcile: t = s = {MAXIMUM_SPEND} with no receipts, exact"));
    let (_, t) = kill.recover(&spend).await?;
    ensure!(t == MAXIMUM_SPEND, "recover refunds {t}, not s");
    check(
        point,
        "recover with the spend journalled before the crash returns the identical refund twice, of t = s: \
         walletd resent those bytes",
    );
    wallet.check_balance(USD_5_CREDITS, "the crashed session cost nothing").await?;
    wallet.stop().await
}

/// walletd, `before_finalize_refund`: walletd dies holding the gate's
/// refund in memory, after the gate committed it. Recover returns that
/// refund, the same bytes before and after the restarted walletd settles
/// from it; the session classifies exact, and the balance drops by
/// exactly the turn's receipt.
async fn before_finalize_refund(kill: &Kill) -> anyhow::Result<()> {
    let point = "before_finalize_refund";
    let mut wallet = Wallet::funded(kill, point).await?;
    wallet.start_walletd(true).await?;
    let charge = wallet.chat().await?;
    wallet.close_fails().await?;
    wallet.walletd_died().await?;
    let spend = wallet.unsettled_spend(ChainState::Closing)?;
    ensure!(wallet.receipts()? == [charge], "the journal holds receipts {:?}, not {charge}", wallet.receipts()?);
    check(point, format!("chain 1 is journalled Closing with its spend and the turn's receipt of {charge}"));
    let (committed, t) = kill.recover(&spend).await?;
    ensure!(t == MAXIMUM_SPEND - charge, "recover refunds {t}, not s - {charge}");
    check(point, format!("the gate committed the refund walletd lost: recover returns it twice, identical, t = {t}"));

    wallet.start_walletd(false).await?;
    wallet.wait_ready().await?;
    check(point, "walletd restarted unarmed; its startup reconcile settled chain 1");
    let report = wallet.reconcile().await?;
    expect_session(&report, t, charge, SettlementOutcome::Exact)?;
    check(point, format!("wallet-cli reconcile: t = {t} with receipts of {charge}, exact"));
    let (again, _) = kill.recover(&spend).await?;
    ensure!(again == committed, "recover returned different bytes after the wallet settled");
    check(point, "after the wallet settled, recover still returns the same bytes");
    wallet.check_balance(USD_5_CREDITS - charge, &format!("the purchase minus the receipt's {charge}")).await?;
    wallet.stop().await
}

/// Requires the wallet's only settled session to be spend 1 of chain 1,
/// of `s` = 500,000, with this `t`, receipts and outcome, and no chain
/// frozen.
fn expect_session(
    report: &ReconcileReport,
    t: u64,
    receipts_sum: u64,
    outcome: SettlementOutcome,
) -> anyhow::Result<()> {
    let expected = SettledSessionView { chain_id: 1, spend_seq: 1, s: MAXIMUM_SPEND, t, receipts_sum, outcome };
    ensure!(
        report.sessions.as_slice() == std::slice::from_ref(&expected),
        "settled sessions {:?}, expected only {expected:?}",
        report.sessions
    );
    ensure!(report.frozen_chains == 0, "{} chains are frozen", report.frozen_chains);
    Ok(())
}

/// Requires a process to have died by SIGABRT at `point`: a crash-point
/// build names the point on stderr just before it aborts.
fn aborted_at(what: &str, status: ExitStatus, stderr: &str, point: &str) -> anyhow::Result<()> {
    let tail = stderr.lines().rev().take(LOG_TAIL).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
    let tail = tail.join("\n");
    ensure!(status.signal() == Some(SIGABRT), "{what} ended with {status}, not SIGABRT at `{point}`:\n{tail}");
    ensure!(stderr.contains(&format!("crash point `{point}` armed")), "{what} aborted, but not at `{point}`:\n{tail}");
    Ok(())
}

/// Prints one passed check of the test at `point`.
fn check(point: &str, what: impl std::fmt::Display) {
    ok(format!("{point}: {what}"));
}

/// One finished wallet-cli run.
struct Run {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// A fresh wallet for one test, in `kill/<point>/` of the stack's temp
/// dir, run with the crash-point builds of wallet-cli and walletd.
struct Wallet {
    /// The test's crash point, armed only where a call asks for it.
    point: &'static str,
    binaries: Binaries,
    /// `wallet()` is the wallet directory, `logs()` walletd's log.
    tree: Tree,
    walletd: Option<Service>,
}

impl Wallet {
    /// A new wallet with the stack's pins and its manifest accepted.
    async fn new(kill: &Kill, point: &'static str) -> anyhow::Result<Self> {
        let tree = Tree::new(kill.world.tree.root().join("kill").join(point));
        create_private_dir(&tree.wallet())?;
        std::fs::copy(kill.world.tree.wallet().join(PINS_FILE), tree.wallet().join(PINS_FILE))?;
        let wallet = Self { point, binaries: kill.binaries.clone(), tree, walletd: None };
        wallet.cli_ok(&["init"]).await?;
        wallet.cli_ok(&["manifest", "fetch", "--approve"]).await?;
        Ok(wallet)
    }

    /// A new wallet that has bought, paid for and collected one `usd-5`.
    async fn funded(kill: &Kill, point: &'static str) -> anyhow::Result<Self> {
        let wallet = Self::new(kill, point).await?;
        let order_id = wallet.buy().await?;
        kill.world.pay(&order_id).await?;
        wallet.collect_one_chain().await?;
        Ok(wallet)
    }

    /// Runs wallet-cli on this wallet with `stdin`, with the test's crash
    /// point armed when `armed`.
    async fn cli(&self, args: &[&str], stdin: Option<&str>, armed: bool) -> anyhow::Result<Run> {
        let binaries = if armed { self.binaries.arming("wallet-cli", self.point) } else { self.binaries.clone() };
        let mut command = tokio::process::Command::from(binaries.command("wallet-cli"));
        command.arg("--wallet-dir").arg(self.tree.wallet()).args(args);
        command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        let mut child = command.spawn().context("starting wallet-cli")?;
        if let Some(text) = stdin {
            let mut pipe = child.stdin.take().context("no stdin pipe")?;
            pipe.write_all(text.as_bytes()).await?;
        }
        let output = child.wait_with_output().await?;
        Ok(Run {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    /// Runs wallet-cli and requires it to succeed, returning stdout.
    async fn cli_ok(&self, args: &[&str]) -> anyhow::Result<String> {
        let run = self.cli(args, None, false).await?;
        ensure!(run.status.success(), "wallet-cli {} ended with {}: {}", args.join(" "), run.status, run.stderr.trim());
        Ok(run.stdout)
    }

    /// Runs `wallet-cli --json` and decodes its answer.
    async fn cli_json<T: DeserializeOwned>(&self, args: &[&str], stdin: Option<&str>) -> anyhow::Result<T> {
        let all: Vec<&str> = ["--json"].iter().chain(args).copied().collect();
        let run = self.cli(&all, stdin, false).await?;
        ensure!(run.status.success(), "wallet-cli {} ended with {}: {}", args.join(" "), run.status, run.stderr.trim());
        serde_json::from_str(&run.stdout).with_context(|| format!("decoding `wallet-cli {}`", args.join(" ")))
    }

    /// Runs wallet-cli with the test's point armed and requires it to die there.
    async fn cli_crash(&self, args: &[&str]) -> anyhow::Result<Run> {
        let run = self.cli(args, None, true).await?;
        aborted_at(&format!("wallet-cli {}", args.join(" ")), run.status, &run.stderr, self.point)?;
        Ok(run)
    }

    /// Runs wallet-cli and requires it to fail with exit 1, as a call does
    /// when the process serving it dies.
    async fn cli_fails(&self, args: &[&str], stdin: Option<&str>) -> anyhow::Result<()> {
        let run = self.cli(args, stdin, false).await?;
        ensure!(
            run.status.code() == Some(1),
            "wallet-cli {} should fail with exit 1, but ended with {}: {}",
            args.join(" "),
            run.status,
            run.stdout
        );
        Ok(())
    }

    /// Buys `usd-5` and returns the order id buy printed.
    async fn buy(&self) -> anyhow::Result<String> {
        let bought = self.cli_ok(&["buy", "--product", "usd-5", "--no-browser"]).await?;
        line_value(&bought, "order_id: ").context("buy printed no order id")
    }

    /// Collects the paid order, which must leave exactly one chain, Ready
    /// with 5,000,000 credits, and the purchase Collected into it.
    async fn collect_one_chain(&self) -> anyhow::Result<()> {
        let collected = self.cli_ok(&["collect"]).await?;
        ensure!(collected.contains("chain 1 holds 5000000 credits (Ready)"), "collect said: {collected}");
        let chains = self.journal()?.chains()?;
        ensure!(chains.len() == 1, "the journal holds {} chains", chains.len());
        let purchase = self.purchase()?;
        ensure!(
            purchase.state == PurchaseState::Collected && purchase.chain_id == Some(chains[0].id),
            "the purchase is {purchase:?}"
        );
        check(
            self.point,
            "wallet-cli collect: exactly one chain, Ready with 5,000,000 credits, the purchase Collected",
        );
        Ok(())
    }

    /// The test's one turn through walletd. Returns its charge, checked
    /// against the fake engine's count for a fresh session of `s`.
    async fn chat(&self) -> anyhow::Result<u64> {
        let args = ["chat", "--model", MODEL, "--conversation", CONVERSATION, "-"];
        let turn: ChatResult = self.cli_json(&args, Some(PROMPT)).await?;
        let receipt = turn.receipt()?;
        let expected = expected_turn(PROMPT, FAKE_SMALL, MAXIMUM_SPEND);
        ensure!(
            receipt.turn == 1
                && receipt.charge == expected.charge
                && receipt.remaining == MAXIMUM_SPEND - expected.charge,
            "receipt {receipt:?}, expected turn 1 charging {}",
            expected.charge
        );
        check(
            self.point,
            format!("one {MODEL} turn: its receipt charges {} as the fake engine counts", receipt.charge),
        );
        Ok(receipt.charge)
    }

    /// The test's turn, which must fail because a process dies under it.
    async fn chat_fails(&self) -> anyhow::Result<()> {
        let args = ["--json", "chat", "--model", MODEL, "--conversation", CONVERSATION, "-"];
        self.cli_fails(&args, Some(PROMPT)).await
    }

    /// Closing the test's conversation, which must fail because a process
    /// dies under it.
    async fn close_fails(&self) -> anyhow::Result<()> {
        self.cli_fails(&["--json", "close", "--conversation", CONVERSATION], None).await
    }

    async fn reconcile(&self) -> anyhow::Result<ReconcileReport> {
        self.cli_json(&["reconcile"], None).await
    }

    /// Starts walletd, with the test's crash point armed when `armed`.
    async fn start_walletd(&mut self, armed: bool) -> anyhow::Result<()> {
        ensure!(self.walletd.is_none(), "walletd is already running");
        let binaries = if armed { self.binaries.arming("walletd", self.point) } else { self.binaries.clone() };
        let (service, _) = stack::walletd(&binaries, &self.tree).await?;
        self.walletd = Some(service);
        Ok(())
    }

    /// Requires walletd to die at the test's point.
    async fn walletd_died(&mut self) -> anyhow::Result<()> {
        let walletd = self.walletd.take().context("walletd is not running")?;
        let (status, log) = walletd.exited(ABORT_TIMEOUT).await?;
        aborted_at("walletd", status, &log, self.point)?;
        check(self.point, "walletd died by SIGABRT there");
        Ok(())
    }

    /// Waits until every chain is Ready, as a restarted walletd's startup
    /// reconcile leaves them. Reading the balance drives no chain itself.
    async fn wait_ready(&self) -> anyhow::Result<()> {
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        loop {
            let view: BalanceView = self.cli_json(&["balance"], None).await?;
            if view.chains.iter().all(|chain| chain.state == ChainState::Ready) {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "chains {:?} not Ready {SETTLE_TIMEOUT:?} after walletd restarted; see {}",
                view.chains,
                self.tree.logs().display()
            );
            tokio::time::sleep(SETTLE_POLL).await;
        }
    }

    /// Requires the balance to be `expected`, all in chain 1, Ready and
    /// resolved, with nothing frozen or waiting: nothing was credited
    /// twice and nothing lost.
    async fn check_balance(&self, expected: u64, why: &str) -> anyhow::Result<()> {
        let view: BalanceView = self.cli_json(&["balance"], None).await?;
        let [chain] = view.chains.as_slice() else { bail!("expected one chain, got {:?}", view.chains) };
        ensure!(
            chain.state == ChainState::Ready && chain.resolved && chain.credits == expected,
            "chain {chain:?}, expected Ready with {expected} credits"
        );
        ensure!(
            view.total_credits == expected
                && view.frozen_orders == 0
                && view.frozen_chains == 0
                && view.orders_awaiting_payment.is_empty(),
            "balance {} (expected {expected}), {} frozen orders, {} frozen chains, {} orders awaiting payment",
            view.total_credits,
            view.frozen_orders,
            view.frozen_chains,
            view.orders_awaiting_payment.len()
        );
        check(self.point, format!("wallet-cli balance: {expected} credits, all in chain 1, Ready ({why})"));
        Ok(())
    }

    /// Stops walletd, if it runs, with SIGTERM.
    async fn stop(mut self) -> anyhow::Result<()> {
        match self.walletd.take() {
            Some(walletd) => walletd.stop().await,
            None => Ok(()),
        }
    }

    fn journal(&self) -> anyhow::Result<Journal> {
        Ok(Journal::open(&self.tree.wallet())?)
    }

    /// The wallet's only purchase, in any state.
    fn purchase(&self) -> anyhow::Result<PurchaseRow> {
        use PurchaseState::{Abandoned, AwaitingPayment, Collected, Frozen, Ordered};
        let mut rows = self.journal()?.purchases_in(&[Ordered, AwaitingPayment, Collected, Frozen, Abandoned])?;
        ensure!(rows.len() == 1, "the journal holds {} purchases", rows.len());
        Ok(rows.remove(0))
    }

    /// Chain 1's journalled spend, which must be spend 1, of `s` =
    /// 500,000, in `state`, with its spend state: the bytes every retry
    /// must resend, durable before the first send (docs 04 section 9).
    fn unsettled_spend(&self, state: ChainState) -> anyhow::Result<SecretBlob> {
        let journal = self.journal()?;
        ensure!(journal.chains()?.len() == 1, "the journal holds more than one chain");
        let row = journal.chain(ChainId(1))?;
        ensure!(
            row.state == state && row.spend_seq == 1 && row.budget == Some(MAXIMUM_SPEND),
            "chain 1 is {row:?}, not spend 1 of {MAXIMUM_SPEND} {state:?}"
        );
        ensure!(row.spend_state.is_some(), "chain 1 has no journalled spend state");
        let message = row.spend_message.as_ref().context("chain 1 has no journalled spend")?;
        Ok(SecretBlob::new(message.expose_secret().to_vec()))
    }

    /// The charges of the receipts journalled for chain 1's spend 1.
    fn receipts(&self) -> anyhow::Result<Vec<u64>> {
        let receipts = self.journal()?.receipts(ChainId(1), 1)?;
        Ok(receipts.iter().map(|receipt| receipt.charge).collect())
    }
}
