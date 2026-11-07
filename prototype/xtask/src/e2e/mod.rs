//! The end-to-end acceptance run (SPEC section 10).
//!
//! Every service runs as its own process from the real binaries, in a
//! fresh temp dir, on Unix sockets and OS-assigned ports. The wallet is
//! driven through `wallet-cli` and `walletd` exactly as a user would;
//! xtask talks to the gate and storefront directly only where a step
//! needs a client the wallet deliberately doesn't have (a second proof
//! from an old journal, a different issue request for a paid order).
//!
//! Each step prints what it checked. Steps 2 to 10 build on the state
//! step 1 brings up, so after a failure they are not run; steps 11 to 13
//! are independent, and steps 12 and 13 bring up stacks of their own
//! (step 13's from separate crash-point builds). A temp dir, with every
//! service's log, is kept when anything in it fails.

mod failures;
mod kill;
mod purchase;
mod sessions;
pub(crate) mod tier2;
pub(crate) mod toolchain;
pub(crate) mod world;

use std::fmt;
use std::future::Future;

use crate::process::Binaries;
use world::World;

/// How one step ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
    NotRun,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("PASS"),
            Self::Fail(why) => write!(f, "FAIL ({why})"),
            Self::Skip(why) => write!(f, "SKIP ({why})"),
            Self::NotRun => f.write_str("NOT RUN (an earlier step failed)"),
        }
    }
}

/// The steps: SPEC section 10's twelve, then the kill tests of docs 03
/// section 5.6.
const STEPS: [&str; 13] = [
    "Bring-up",
    "Attestation negatives",
    "Purchase",
    "Issuance idempotency",
    "Chat and metering",
    "Close and reconcile",
    "Replay",
    "Gate crash",
    "Lost close response and idle close",
    "Budget errors",
    "Two-stack check",
    "Tier 2",
    "Kill tests",
];

/// Collects each step's outcome and prints them as they finish.
struct Report {
    outcomes: Vec<Outcome>,
}

impl Report {
    fn new() -> Self {
        Self { outcomes: Vec::new() }
    }

    /// Runs the next step and records how it ended.
    async fn step<T>(&mut self, run: impl Future<Output = anyhow::Result<T>>) -> Option<T> {
        let number = self.outcomes.len() + 1;
        println!("\nstep {number}: {}", STEPS[number - 1]);
        let result = run.await;
        let (outcome, value) = match result {
            Ok(value) => (Outcome::Pass, Some(value)),
            Err(error) => (Outcome::Fail(format!("{error:#}")), None),
        };
        println!("step {number}: {outcome}");
        self.outcomes.push(outcome);
        value
    }

    fn skip(&mut self, why: String) {
        let number = self.outcomes.len() + 1;
        println!("\nstep {number}: {}\nstep {number}: SKIP ({why})", STEPS[number - 1]);
        self.outcomes.push(Outcome::Skip(why));
    }

    fn not_run_until(&mut self, step: usize) {
        while self.outcomes.len() + 1 < step {
            self.outcomes.push(Outcome::NotRun);
        }
    }

    fn passed(&self) -> bool {
        self.outcomes.iter().all(|outcome| matches!(outcome, Outcome::Pass | Outcome::Skip(_)))
    }

    fn print_summary(&self) {
        println!("\nsummary");
        for (index, outcome) in self.outcomes.iter().enumerate() {
            println!("  {:>2}. {:<36} {outcome}", index + 1, STEPS[index]);
        }
    }
}

/// Prints one passed check.
fn ok(what: impl fmt::Display) {
    println!("    ok  {what}");
}

/// Runs every step. Returns whether all passed or were skipped.
pub async fn run(tor: bool) -> anyhow::Result<bool> {
    let binaries = Binaries::build(tor)?;
    let mut report = Report::new();
    if let Some(mut world) = report.step(World::bring_up(&binaries, None)).await {
        let tier1 = &mut world;
        let _ = report.step(purchase::attestation_negatives(tier1)).await.is_some()
            && report.step(purchase::purchase(tier1)).await.is_some()
            && report.step(purchase::issuance_idempotency(tier1)).await.is_some()
            && report.step(sessions::chat_and_metering(tier1)).await.is_some()
            && report.step(sessions::close_and_reconcile(tier1)).await.is_some()
            && report.step(sessions::replay(tier1)).await.is_some()
            && report.step(failures::gate_crash(tier1)).await.is_some()
            && report.step(failures::idle_close(tier1)).await.is_some()
            && report.step(failures::budget_errors(tier1)).await.is_some();
        if let Some(dir) = world.finish(report.passed()).await {
            println!("\nlogs and state kept in {}", dir.display());
        }
    }
    report.not_run_until(11);
    report.step(async { toolchain::two_stack_check() }).await;
    match toolchain::tor_available() {
        Some(path) if tor => {
            Box::pin(report.step(tier2::run(&binaries, &path))).await;
        }
        Some(path) => report.skip(format!("needs `--tor`; tor is at {}", path.display())),
        None => report.skip("`tor` is not installed (`command -v tor` found nothing)".into()),
    }
    report.step(kill::run(&binaries)).await;
    report.print_summary();
    Ok(report.passed())
}
