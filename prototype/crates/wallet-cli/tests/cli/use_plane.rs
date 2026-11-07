//! The use-plane commands against a stub walletd: each request carries
//! the `Host` and bearer token from walletd's files, and `--json` prints
//! walletd's answer in the shape xtask decodes (`World::cli_json`).

use std::collections::BTreeSet;

use proto::Receipt;
use proto::openai::{ChatCompletionRequest, ChatRole, ModelList};
use proto::walletd::{BalanceView, CONVERSATION_HEADER, ReconcileReport, WalletCloseResponse};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::harness::{
    Seen, StubWalletd, balance_view, close_response, model_list, receipt, reconcile_report, wallet_cli,
};

/// Runs `wallet-cli --json <args>`, which must succeed, and decodes its
/// stdout as xtask does.
async fn cli_json<T: DeserializeOwned>(dir: &std::path::Path, args: &[&str], stdin: Option<&str>) -> T {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let output = wallet_cli(dir, &all, stdin).await;
    assert_eq!(output.code, Some(0), "wallet-cli {args:?}: {}", output.stderr);
    serde_json::from_str(&output.stdout).unwrap_or_else(|error| panic!("{args:?} printed {:?}: {error}", output.stdout))
}

/// The last request the stub saw, which must name walletd's address and
/// carry its token.
fn last_request(stub: &StubWalletd) -> Seen {
    let seen = stub.seen().pop().expect("walletd was asked");
    let header = |name| seen.headers.get(name).map(|value| value.to_str().unwrap().to_owned());
    assert_eq!(header("host"), Some(stub.addr.to_string()), "{} {}", seen.method, seen.path);
    assert_eq!(header("authorization"), Some(format!("Bearer {}", stub.token)), "{} {}", seen.method, seen.path);
    seen
}

/// `chat --json` prints exactly `{text, receipt, rollover, recovered}`:
/// the streamed text joined, the receipt from the last chunk, and the
/// session notices from walletd's headers, `null` when absent. The prompt
/// `-` is read whole from stdin, and `--conversation` becomes the
/// `X-Conversation-Id` header of a streamed request.
#[tokio::test]
async fn chat_json_has_the_shape_xtask_decodes() {
    let dir = tempfile::tempdir().unwrap();
    let stub = StubWalletd::start(dir.path()).await;
    let prompt = "notices, please\nacross two lines, from stdin";
    let args = ["chat", "--model", "fake-small", "--conversation", "c-1", "-"];
    let result: Value = cli_json(dir.path(), &args, Some(prompt)).await;

    let keys: BTreeSet<_> = result.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, BTreeSet::from(["text", "receipt", "rollover", "recovered"]));
    assert_eq!(result["text"], "hi there");
    assert_eq!(serde_json::from_value::<Receipt>(result["receipt"].clone()).unwrap(), receipt());
    assert_eq!(
        (result["rollover"].as_str(), result["recovered"].as_str()),
        (Some("session_exhausted"), Some("full_refund"))
    );

    let seen = last_request(&stub);
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("POST", "/v1/chat/completions"));
    assert_eq!(seen.headers.get(CONVERSATION_HEADER).unwrap(), "c-1");
    let request: ChatCompletionRequest = serde_json::from_slice(&seen.body).unwrap();
    assert_eq!((request.model.as_str(), request.stream), ("fake-small", Some(true)));
    let [message] = &request.messages[..] else { panic!("one message: {:?}", request.messages) };
    assert_eq!(message.role, ChatRole::User);
    assert_eq!(message.content, prompt);

    let quiet: Value = cli_json(dir.path(), &["chat", "--model", "fake-small", "hello"], None).await;
    assert_eq!((quiet["rollover"].clone(), quiet["recovered"].clone()), (Value::Null, Value::Null));
    assert!(last_request(&stub).headers.get(CONVERSATION_HEADER).is_none(), "no id: a one-off turn");
    stub.stop().await;
}

/// `balance`, `models`, `close` and `reconcile` print walletd's answer
/// unchanged, every field kept, whether `--json` comes before or after the
/// subcommand. `close` sends the conversation it names, or none for all.
#[tokio::test]
async fn the_wallet_commands_print_walletds_answers_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let stub = StubWalletd::start(dir.path()).await;

    assert_eq!(cli_json::<BalanceView>(dir.path(), &["balance"], None).await, balance_view());
    assert_eq!(last_request(&stub).path, "/wallet/balance");
    let after = wallet_cli(dir.path(), &["balance", "--json"], None).await;
    assert_eq!(serde_json::from_str::<BalanceView>(&after.stdout).unwrap(), balance_view());

    assert_eq!(cli_json::<ModelList>(dir.path(), &["models"], None).await, model_list());
    assert_eq!(last_request(&stub).path, "/v1/models");

    let closed: WalletCloseResponse = cli_json(dir.path(), &["close", "--conversation", "c-1"], None).await;
    assert_eq!(closed, close_response());
    let seen = last_request(&stub);
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("POST", "/wallet/close"));
    assert_eq!(serde_json::from_slice::<Value>(&seen.body).unwrap(), serde_json::json!({"conversation_id": "c-1"}));
    let _: WalletCloseResponse = cli_json(dir.path(), &["close"], None).await;
    assert_eq!(serde_json::from_slice::<Value>(&last_request(&stub).body).unwrap(), serde_json::json!({}));

    assert_eq!(cli_json::<ReconcileReport>(dir.path(), &["reconcile"], None).await, reconcile_report());
    assert_eq!(last_request(&stub).path, "/wallet/reconcile");
    stub.stop().await;
}

/// A turn walletd refuses exits 1 with walletd's code and the credits a
/// session would need on stderr, which xtask's step 10 reads, and prints
/// nothing on stdout for a script to mistake for an answer.
#[tokio::test]
async fn a_refused_turn_exits_1_with_walletds_code() {
    let dir = tempfile::tempdir().unwrap();
    let stub = StubWalletd::start(dir.path()).await;
    let args = ["--json", "chat", "--model", "fake-large", "--conversation", "huge", "-"];
    let output = wallet_cli(dir.path(), &args, Some("a prompt too large for any session")).await;
    assert_eq!(output.code, Some(1), "{}", output.stdout);
    assert!(output.stdout.is_empty(), "{}", output.stdout);
    assert!(
        output.stderr.contains("402 prompt_exceeds_budget") && output.stderr.contains("520028"),
        "{}",
        output.stderr
    );
    stub.stop().await;
}
