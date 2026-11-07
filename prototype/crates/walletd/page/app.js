// The wallet page: model selector, streaming chat and balance.
//
// Model text is only ever inserted with textContent, and the CSP allows
// no other origin, so a reply can't load a remote resource or run code.
"use strict";

const token = document.querySelector('meta[name="walletd-token"]').content;
const $ = (id) => document.getElementById(id);

// The page keeps the history in memory only; walletd and the gate keep none.
let conversationId = newConversationId();
let history = [];

function newConversationId() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

async function api(path, options = {}) {
  const headers = Object.assign({ Authorization: `Bearer ${token}` }, options.headers || {});
  const response = await fetch(path, Object.assign({}, options, { headers }));
  if (!response.ok) {
    let message = `${response.status}`;
    try {
      const body = await response.json();
      message = body.message ? `${body.error}: ${body.message}` : body.error;
      if (body.required) message += ` (needs ${body.required} credits)`;
    } catch (_) { /* not JSON */ }
    throw new Error(message);
  }
  return response;
}

function setStatus(text) {
  $("status").textContent = text;
}

// Send and New conversation are disabled together while a turn or a close
// is in flight. Otherwise a send racing a close goes to the old
// conversation with its whole history, and reopens a session there that
// holds the chain until walletd's idle close.
function setBusy(busy) {
  $("send").disabled = busy;
  $("new-conversation").disabled = busy;
}

async function loadModels() {
  const list = await (await api("/v1/models")).json();
  const select = $("model");
  select.replaceChildren();
  for (const model of list.data) {
    const option = document.createElement("option");
    option.value = model.id;
    option.textContent = model.id;
    select.append(option);
  }
}

async function loadBalance() {
  const view = await (await api("/wallet/balance")).json();
  $("total-usd").textContent = view.total_usd;
  $("total-credits").textContent = String(view.total_credits);
  $("experimental").hidden = !view.experimental_platform;
  const estimates = $("estimates");
  estimates.replaceChildren();
  for (const [model, estimate] of Object.entries(view.estimated_tokens_remaining)) {
    const item = document.createElement("li");
    item.textContent = `${model}: at most ${estimate.output_tokens_at_most} output tokens (estimate)`;
    estimates.append(item);
  }
  const notes = [];
  const unresolved = view.chains.filter((chain) => !chain.resolved).length;
  if (unresolved) notes.push(`${unresolved} session(s) not yet settled: reconcile`);
  if (view.orders_awaiting_payment.length) notes.push(`${view.orders_awaiting_payment.length} order(s) awaiting payment`);
  if (view.dust_credits) notes.push(`${view.dust_credits} credits of dust, not spendable`);
  if (view.frozen_orders || view.frozen_chains) notes.push("frozen items need investigation");
  $("balance-notes").textContent = notes.join("; ");
}

async function reconcile() {
  const report = await (await api("/wallet/reconcile", { method: "POST" })).json();
  const lines = [`recovered ${report.recovered} session(s); ${report.frozen_chains} frozen chain(s)`];
  for (const s of report.sessions) {
    lines.push(`chain ${s.chain_id} spend ${s.spend_seq}: s=${s.s} t=${s.t} receipts=${s.receipts_sum} ${s.outcome}`);
  }
  const pre = $("reconcile-report");
  pre.textContent = lines.join("\n");
  pre.hidden = false;
  await loadBalance();
}

function appendMessage(role, text) {
  const block = document.createElement("div");
  block.className = `message ${role}`;
  const label = document.createElement("strong");
  label.textContent = role === "user" ? "You" : "Model";
  const body = document.createElement("p");
  body.textContent = text;
  block.append(label, body);
  $("transcript").append(block);
  return body;
}

// Reads an SSE body and calls onData for each `data:` payload.
async function readEvents(response, onData) {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let end;
    while ((end = buffer.indexOf("\n\n")) >= 0) {
      const event = buffer.slice(0, end);
      buffer = buffer.slice(end + 2);
      const data = event.split("\n").filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).trimStart()).join("\n");
      if (data) onData(data);
    }
  }
}

async function send(event) {
  event.preventDefault();
  const prompt = $("prompt").value.trim();
  if (!prompt) return;
  $("prompt").value = "";
  setBusy(true);
  history.push({ role: "user", content: prompt });
  const asked = appendMessage("user", prompt);
  const reply = appendMessage("assistant", "");
  setStatus("waiting for the gate...");
  try {
    const response = await api("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Conversation-Id": conversationId },
      body: JSON.stringify({ model: $("model").value, messages: history, stream: true }),
    });
    const notes = [];
    if (response.headers.get("x-counteractive-rollover")) notes.push("started a new session");
    if (response.headers.get("x-counteractive-recovered")) notes.push("recovered a lost session");
    let receipt = null;
    await readEvents(response, (data) => {
      if (data === "[DONE]") return;
      const chunk = JSON.parse(data);
      const text = chunk.choices && chunk.choices[0] && chunk.choices[0].delta.content;
      if (text) reply.textContent += text;
      if (chunk.receipt) receipt = chunk.receipt;
    });
    history.push({ role: "assistant", content: reply.textContent });
    if (receipt) notes.push(`charged ${receipt.charge} credits, ${receipt.remaining} left in this session`);
    setStatus(notes.join("; "));
    await loadBalance();
  } catch (error) {
    history.pop();
    if (!reply.textContent) {
      // Nothing arrived: drop the turn from the page too, and give the
      // prompt back so it can be sent again.
      asked.parentElement.remove();
      reply.parentElement.remove();
      if (!$("prompt").value) $("prompt").value = prompt;
    }
    setStatus(`error: ${error.message}`);
  } finally {
    setBusy(false);
  }
}

// Switches to a fresh conversation first, then closes the previous one's
// session, so nothing sent from now on can reach the old conversation.
async function newConversation() {
  const previous = conversationId;
  conversationId = newConversationId();
  history = [];
  $("transcript").replaceChildren();
  setBusy(true);
  try {
    await api("/wallet/close", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ conversation_id: previous }),
    });
  } catch (error) {
    setStatus(`error closing the session: ${error.message}`);
  } finally {
    setBusy(false);
  }
  await loadBalance();
}

function guarded(action) {
  return () => action().catch((error) => setStatus(`error: ${error.message}`));
}

$("composer").addEventListener("submit", send);
$("refresh").addEventListener("click", guarded(loadBalance));
$("reconcile").addEventListener("click", guarded(reconcile));
$("new-conversation").addEventListener("click", guarded(newConversation));
guarded(loadModels)();
guarded(loadBalance)();
