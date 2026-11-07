//! Argument handling: exit statuses for bad usage, and where the wallet
//! directory comes from.

use wallet_core::journal::JOURNAL_FILE;

use super::harness::{closed_port, command, pins, run, wallet_cli, write_pins};

/// Bad usage exits 1 like any other error, never 2, which a script reads
/// as `collect`'s "orders still unpaid": a typo'd `colect` in a payment
/// loop must not look like an unpaid order. Help and version exit 0.
#[tokio::test]
async fn bad_usage_exits_1_and_help_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let usage_errors: [&[&str]; 5] =
        [&["buy", "--no-browser"], &["chat", "hello"], &["colect"], &["collect", "--wait", "soon"], &[]];
    for args in usage_errors {
        let output = wallet_cli(dir.path(), args, None).await;
        assert_eq!(output.code, Some(1), "wallet-cli {args:?}: {}", output.stderr);
        assert!(output.stderr.starts_with("error:"), "wallet-cli {args:?} says what is wrong: {}", output.stderr);
    }
    for args in [["--help"], ["--version"]] {
        let output = wallet_cli(dir.path(), &args, None).await;
        assert_eq!(output.code, Some(0), "wallet-cli {args:?}: {}", output.stderr);
        assert!(!output.stdout.is_empty());
    }
}

/// The wallet directory is `--wallet-dir`, else `COUNTERACTIVE_WALLET_DIR`,
/// and the flag wins when both are set. `init` needs the pins there and
/// creates nothing without them.
#[tokio::test]
async fn the_wallet_dir_comes_from_the_flag_or_the_environment() {
    let with_pins = tempfile::tempdir().unwrap();
    write_pins(with_pins.path(), &pins(closed_port().await, None));
    let without = tempfile::tempdir().unwrap();

    let mut both = command();
    both.env("COUNTERACTIVE_WALLET_DIR", with_pins.path()).arg("--wallet-dir").arg(without.path()).arg("init");
    let output = run(both, None).await;
    assert_eq!(output.code, Some(1), "{}", output.stdout);
    assert!(output.stderr.contains("init needs wallet.toml"), "{}", output.stderr);
    assert!(!without.path().join(JOURNAL_FILE).exists() && !with_pins.path().join(JOURNAL_FILE).exists());

    let mut from_env = command();
    from_env.env("COUNTERACTIVE_WALLET_DIR", with_pins.path()).arg("init");
    let output = run(from_env, None).await;
    assert_eq!(output.code, Some(0), "{}", output.stderr);
    assert!(with_pins.path().join(JOURNAL_FILE).exists());
    assert!(output.stdout.contains("counteractive-cli-test epoch 1"), "{}", output.stdout);
}

/// The use-plane commands find walletd through `walletd.addr`; without
/// it they say walletd isn't running and exit 1.
#[tokio::test]
async fn use_plane_commands_need_a_running_walletd() {
    let dir = tempfile::tempdir().unwrap();
    let commands: [&[&str]; 5] =
        [&["balance"], &["models"], &["chat", "--model", "fake-small", "hi"], &["close"], &["reconcile"]];
    for args in commands {
        let output = wallet_cli(dir.path(), args, None).await;
        assert_eq!(output.code, Some(1), "wallet-cli {args:?}");
        assert!(output.stderr.contains("is walletd running?"), "wallet-cli {args:?}: {}", output.stderr);
    }
}
