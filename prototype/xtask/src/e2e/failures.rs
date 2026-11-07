//! Steps 8 to 10: a gate crash, a session the gate closed on its own and a
//! turn its client abandoned, and the two budget errors.

use std::time::{Duration, Instant};

use anyhow::{anyhow, ensure};
use proto::walletd::{ChainState, ReconcileReport, SettledSessionView, SettlementOutcome};

use super::ok;
use super::sessions::{check_turn, expected_turn};
use super::world::World;
use crate::release::{FAKE_LARGE, FAKE_SMALL, MAXIMUM_SPEND};

/// How long an abandoned turn may take to finish and settle: the engine's
/// 3 s stall plus the turn and the close, with room for a slow machine.
const ABANDONED_SETTLE: Duration = Duration::from_secs(20);
/// The same behind Tor (tier 2), where walletd first opens the turn's
/// session on a new circuit to the onion service, and every call after
/// that is a round trip through relays.
const ABANDONED_SETTLE_OVER_TOR: Duration = Duration::from_secs(120);
/// How long step 8 waits behind Tor between killing the gate and
/// restarting it, so the dead stream's END reaches walletd first: a few
/// round trips of a rendezvous circuit.
const CRASH_NOTICED_OVER_TOR: Duration = Duration::from_secs(10);

/// Step 8: kill -9 the gate after a turn; after a restart the wallet's
/// next turn finds the session gone, recovers `t = s`, and continues in a
/// new session. The crashed session costs nothing.
pub async fn gate_crash(world: &mut World) -> anyhow::Result<()> {
    let before = world.check_balance().await?;
    let earlier = world.reconcile().await?;
    let prompt = "a turn the gate will forget";
    let turn = world.chat("crash", "fake-small", prompt).await?;
    check_turn(world, &turn, prompt, "fake-small", FAKE_SMALL, 1, MAXIMUM_SPEND)?;
    // The gate crashes before settling, so this charge is refunded.
    world.spent -= turn.receipt()?.charge;

    world.stop_gate(true).await?;
    ok("gate killed with SIGKILL while the session was open");
    if world.over_tor() {
        // walletd learns that its connection died only when tor's END for
        // the stream crosses the rendezvous circuit. A turn sent before
        // that goes out on the dead stream, and since walletd can't tell
        // whether it reached the gate, it is rightly counted as an unknown
        // turn and refused, never resent. A real restart takes longer than
        // the END does; this pause stands in for it.
        tokio::time::sleep(CRASH_NOTICED_OVER_TOR).await;
    }
    let release = world.tree.release();
    world.start_gate(&release).await?;
    ok("gate restarted and unlocked: Ready again from its store");

    let prompt = "the next turn after the crash";
    let turn = world.chat("crash", "fake-small", prompt).await?;
    ensure!(turn.recovered.as_deref() == Some("full_refund"), "recovered notice: {:?}", turn.recovered);
    ok("the next turn saw 404, recovered the spend, and walletd reported it settled as full_refund");
    check_turn(world, &turn, prompt, "fake-small", FAKE_SMALL, 1, MAXIMUM_SPEND)?;
    let closed = world.close(Some("crash")).await?;
    ensure!(closed.closed == 1, "closing `crash`: {closed:?}");

    let after = world.check_balance().await?;
    let charge = turn.receipt()?.charge;
    ensure!(after == before - charge, "balance {after} != {before} - {charge}");
    ok(format!("balance {after} = {before} before the crashed session - {charge} for the new turn only"));
    let [crashed, next] = settled_since(&earlier, &world.reconcile().await?)?;
    ensure!(
        crashed.outcome == SettlementOutcome::FullRefund && crashed.t == crashed.s,
        "the crashed session settled as {crashed:?}"
    );
    ensure!(next.outcome == SettlementOutcome::Exact, "the session after the crash settled as {next:?}");
    ok(format!("reconcile: the crashed session settled with t = s = {} (full_refund), the next one exact", crashed.s));
    Ok(())
}

/// Step 9: on a release with a short idle timer (2 s; 10 s behind Tor) the
/// gate closes an untouched session itself; the wallet's close gets `404`
/// and recover returns the exact refund. Then a client abandons a turn
/// mid-way.
pub async fn idle_close(world: &mut World) -> anyhow::Result<()> {
    world.stop_gate(false).await?;
    let idle = world.tree.idle_release();
    world.start_gate(&idle).await?;
    ok(format!("gate restarted on release-idle.json (idle_seconds = {})", world.idle_seconds));

    let earlier = world.reconcile().await?;
    let prompt = "a turn and then silence";
    let turn = world.chat("idle", "fake-small", prompt).await?;
    check_turn(world, &turn, prompt, "fake-small", FAKE_SMALL, 1, MAXIMUM_SPEND)?;
    tokio::time::sleep(Duration::from_secs(world.idle_seconds + 2)).await;
    let closed = world.close(Some("idle")).await?;
    ensure!(closed.closed == 1 && closed.recovered == 1, "closing `idle`: {closed:?}");
    ok("after the idle timer, wallet-cli close: the gate answered 404 and walletd settled it through recover");
    let [session] = settled_since(&earlier, &world.reconcile().await?)?;
    ensure!(
        session.outcome == SettlementOutcome::Exact && session.receipts_sum == turn.receipt()?.charge,
        "the idle-closed session settled as {session:?}"
    );
    ok(format!("reconcile: exact, t = {} = s - the one receipt", session.t));
    world.check_balance().await?;

    world.stop_gate(false).await?;
    let release = world.tree.release();
    world.start_gate(&release).await?;
    ok("gate restarted on release.json");
    abandoned_turn(world).await
}

/// The rest of step 9, the wallet's own lost response: a client hangs up
/// on a one-off, non-streaming turn while the engine stalls. walletd must
/// finish the turn and close its session anyway, as the gate finishes its
/// own flows, rather than strand the chain in a session nobody holds.
async fn abandoned_turn(world: &mut World) -> anyhow::Result<()> {
    let earlier = world.reconcile().await?;
    let prompt = "a client that gives up [[stall:3000]]";
    world.abandon_chat("fake-small", prompt, Duration::from_secs(1)).await?;
    ok("a one-off, non-streaming chat whose engine stalls 3 s: the client hung up after 1 s");

    // Settled means no chain is left in a session and the turn's charge is
    // taken. Every chain Ready alone is not enough: behind Tor, walletd may
    // still be dialling the onion service, with no spend journalled yet,
    // when the client hangs up.
    let charge = expected_turn(prompt, FAKE_SMALL, MAXIMUM_SPEND).charge;
    let settled_total = world.expected_balance() - charge;
    let patience = if world.over_tor() { ABANDONED_SETTLE_OVER_TOR } else { ABANDONED_SETTLE };
    let deadline = Instant::now() + patience;
    loop {
        let view = world.balance().await?;
        let out_of_session =
            view.chains.iter().all(|chain| matches!(chain.state, ChainState::Ready | ChainState::Dust));
        if out_of_session && view.total_credits == settled_total {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "the abandoned turn has not settled after {patience:?}: balance {} (settled would be {settled_total}), \
             chains {:?}",
            view.total_credits,
            view.chains
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let [session] = settled_since(&earlier, &world.reconcile().await?)?;
    ensure!(
        session.outcome == SettlementOutcome::Exact && session.receipts_sum == charge,
        "the abandoned turn's session settled as {session:?}, expected one exact receipt of {charge}"
    );
    world.spent += charge;
    world.check_balance().await?;
    ok(format!("walletd finished the turn and closed its session anyway: exact, one receipt of {charge} = s - t"));
    Ok(())
}

/// Step 10: a prompt no tier can cover gets `402 prompt_exceeds_budget`;
/// a turn the session can't cover gets `402 session_exhausted`, and the
/// wallet rolls over to one new session.
pub async fn budget_errors(world: &mut World) -> anyhow::Result<()> {
    // 4 x (130,000 + 4) + 12 = 520,028 > 500,000, the largest tier.
    let earlier = world.reconcile().await?;
    let huge = "w ".repeat(130_000);
    let error = world.chat_error("huge", "fake-large", &huge).await?;
    ensure!(error.contains("prompt_exceeds_budget") && error.contains("520028"), "a 130,000-word prompt said: {error}");
    ok("a 130,000-word prompt on fake-large: 402 prompt_exceeds_budget, required 520,028 > every tier");
    world.check_balance().await?;
    let [session] = settled_since(&earlier, &world.reconcile().await?)?;
    ensure!(
        session.t == session.s && session.receipts_sum == 0 && session.outcome == SettlementOutcome::Exact,
        "the session opened for the huge prompt settled as {session:?}"
    );
    ok("the session walletd opened for it settled with t = s and no receipts: balance unchanged");

    // 4 x 100,004 + 12 x 22 = 400,280, leaving 99,720: less than the next
    // turn's hold of 400,028.
    let big = "w ".repeat(100_000);
    let turn = world.chat("big", "fake-large", &big).await?;
    let receipt = check_turn(world, &turn, &big, "fake-large", FAKE_LARGE, 1, MAXIMUM_SPEND)?;
    let turn = world.chat("big", "fake-large", &big).await?;
    ensure!(turn.rollover.as_deref() == Some("session_exhausted"), "rollover notice: {:?}", turn.rollover);
    ok(format!("the same prompt with {} left: 402 session_exhausted, rolled over once", receipt.remaining));
    check_turn(world, &turn, &big, "fake-large", FAKE_LARGE, 1, MAXIMUM_SPEND)?;
    let closed = world.close(Some("big")).await?;
    ensure!(closed.closed == 1, "closing `big`: {closed:?}");

    let total = world.check_balance().await?;
    ok(format!("balance {total} = 5,000,000 - {} charged over the whole run, exactly", world.spent));
    let report = world.reconcile().await?;
    let full_refunds = report.sessions.iter().filter(|session| session.outcome == SettlementOutcome::FullRefund);
    let others_exact = report
        .sessions
        .iter()
        .all(|session| session.outcome == SettlementOutcome::Exact || session.outcome == SettlementOutcome::FullRefund);
    ensure!(
        others_exact && full_refunds.count() == 1,
        "expected every session exact but step 8's crashed one: {:?}",
        report.sessions
    );
    ok(format!(
        "reconcile: all {} settled sessions exact, except step 8's crashed one (full_refund)",
        report.sessions.len()
    ));
    Ok(())
}

/// The `N` sessions settled between two reports, oldest first.
fn settled_since<const N: usize>(
    before: &ReconcileReport,
    after: &ReconcileReport,
) -> anyhow::Result<[SettledSessionView; N]> {
    let key = |session: &SettledSessionView| (session.chain_id, session.spend_seq);
    let mut new: Vec<_> = after
        .sessions
        .iter()
        .filter(|session| !before.sessions.iter().any(|old| key(old) == key(session)))
        .cloned()
        .collect();
    new.sort_by_key(key);
    new.try_into().map_err(|new: Vec<_>| anyhow!("expected {N} newly settled sessions, got {new:?}"))
}
