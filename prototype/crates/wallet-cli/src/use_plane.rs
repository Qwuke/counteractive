//! The use-plane commands, all through walletd: `balance`, `models`,
//! `chat`, `close` and `reconcile`. With `--json` each prints walletd's
//! answer as JSON for scripts; otherwise it prints lines for people.

use std::io::Write as _;

use anyhow::{Context as _, bail};
use http_body_util::BodyExt as _;
use proto::Receipt;
use proto::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatMessage, ChatRole, ModelList};
use proto::sse::{OPENAI_DONE, SseSplitter};
use proto::walletd::{
    BalanceView, RECOVERED_HEADER, ROLLOVER_HEADER, ReconcileReport, WalletCloseRequest, WalletCloseResponse,
    WithReceipt,
};
use serde::Serialize;

use crate::local::Walletd;

/// The largest single event accepted from walletd's stream.
const MAX_EVENT: usize = 64 * 1024;

/// `balance`.
pub async fn balance(walletd: &Walletd, json: bool) -> anyhow::Result<()> {
    let view: BalanceView = walletd.get("/wallet/balance").await?;
    if json {
        return print_json(&view);
    }
    println!("total: {} credits ({} USD)", view.total_credits, view.total_usd);
    for chain in &view.chains {
        let exact = if chain.resolved { "" } else { " (unsettled: run reconcile)" };
        println!("chain {}: {:?} {} credits{exact}", chain.id, chain.state, chain.credits);
    }
    for order in &view.orders_awaiting_payment {
        println!("awaiting payment: {} ({} credits)", order.product, order.credits);
    }
    if view.dust_credits > 0 {
        println!("dust: {} credits below the smallest session", view.dust_credits);
    }
    if view.frozen_orders + view.frozen_chains > 0 {
        println!("frozen: {} orders, {} chains need investigation", view.frozen_orders, view.frozen_chains);
    }
    for (model, estimate) in &view.estimated_tokens_remaining {
        println!("{model}: at most {} output tokens (estimate)", estimate.output_tokens_at_most);
    }
    if view.experimental_platform {
        println!("warning: the accepted release runs on an experimental platform without full verification");
    }
    Ok(())
}

/// `models`.
pub async fn models(walletd: &Walletd, json: bool) -> anyhow::Result<()> {
    let list: ModelList = walletd.get("/v1/models").await?;
    if json {
        return print_json(&list);
    }
    for model in list.data {
        println!("{}", model.id);
    }
    Ok(())
}

/// What one chat turn produced, as `chat --json` prints it.
#[derive(Debug, Serialize)]
struct ChatResult {
    text: String,
    receipt: Option<Receipt>,
    rollover: Option<String>,
    recovered: Option<String>,
}

/// `chat --model <id> [--conversation <id>] <prompt>`: one user message,
/// streamed. The model's text goes to stdout as it arrives; the receipt
/// and any session notices go to stderr.
pub async fn chat(
    walletd: &Walletd,
    model: &str,
    conversation: Option<&str>,
    prompt: String,
    json: bool,
) -> anyhow::Result<()> {
    let request = ChatCompletionRequest {
        model: model.to_owned(),
        messages: vec![ChatMessage::new(ChatRole::User, prompt)],
        max_completion_tokens: None,
        max_tokens: None,
        stream: Some(true),
    };
    let response = walletd.post_streaming("/v1/chat/completions", &request, conversation).await?;
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let mut result = ChatResult {
        text: String::new(),
        receipt: None,
        rollover: header(ROLLOVER_HEADER),
        recovered: header(RECOVERED_HEADER),
    };
    if !json {
        if let Some(why) = &result.rollover {
            eprintln!("rollover: the session ended ({why}); this turn runs in a new session");
        }
        if let Some(outcome) = &result.recovered {
            eprintln!("recovered: the gate had lost the session; it settled as {outcome}");
        }
    }

    let mut body = response.into_body();
    let mut splitter = SseSplitter::new(MAX_EVENT);
    let mut stdout = std::io::stdout();
    'stream: while let Some(frame) = body.frame().await {
        let Ok(data) = frame.context("reading the stream from walletd")?.into_data() else { continue };
        for event in splitter.push(&data)? {
            if event.data == OPENAI_DONE {
                break 'stream;
            }
            let chunk: WithReceipt<ChatCompletionChunk> =
                serde_json::from_str(&event.data).context("walletd sent a malformed chunk")?;
            if let Some(text) = chunk.body.text() {
                result.text.push_str(text);
                if !json {
                    stdout.write_all(text.as_bytes())?;
                    stdout.flush()?;
                }
            }
            if chunk.receipt.is_some() {
                result.receipt = chunk.receipt;
            }
        }
    }

    if json {
        return print_json(&result);
    }
    println!();
    match &result.receipt {
        Some(receipt) => eprintln!(
            "receipt: turn={} model={} prompt_tokens={} completion_tokens={} charge={} remaining={} finish={}",
            receipt.turn,
            receipt.model,
            receipt.prompt_tokens,
            receipt.completion_tokens,
            receipt.charge,
            receipt.remaining,
            receipt.finish.as_str()
        ),
        None => bail!("the turn ended without a receipt; run `wallet-cli reconcile` after closing"),
    }
    Ok(())
}

/// `close [--conversation <id>]`: one conversation's session, or all.
pub async fn close(walletd: &Walletd, conversation: Option<String>, json: bool) -> anyhow::Result<()> {
    let answer: WalletCloseResponse =
        walletd.post("/wallet/close", &WalletCloseRequest { conversation_id: conversation }).await?;
    if json {
        return print_json(&answer);
    }
    println!("closed {} session(s); {} of them the gate had already ended", answer.closed, answer.recovered);
    Ok(())
}

/// `reconcile`: settles what no conversation holds, then lists every settled session.
pub async fn reconcile(walletd: &Walletd, json: bool) -> anyhow::Result<()> {
    let report: ReconcileReport = walletd.post("/wallet/reconcile", &serde_json::json!({})).await?;
    if json {
        return print_json(&report);
    }
    println!("recovered {} session(s); {} frozen chain(s)", report.recovered, report.frozen_chains);
    for session in &report.sessions {
        println!(
            "chain {} spend {}: s={} t={} receipts={} {}",
            session.chain_id,
            session.spend_seq,
            session.s,
            session.t,
            session.receipts_sum,
            session.outcome.as_str()
        );
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
