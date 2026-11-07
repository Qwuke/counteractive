// The demo console: drives the real wallet through the console's API.
//
// Everything dynamic is inserted with textContent, and the CSP allows no
// other origin and no inline code. The page holds only the console's own
// token; the console adds walletd's token when it forwards to walletd.
"use strict";

const token = document.querySelector('meta[name="console-token"]').content;
const $ = (id) => document.getElementById(id);
const fmt = (n) => Number(n).toLocaleString("en-US");
// walletd's "1.000000" as $1.00, and "0.998854" as $0.998854.
const usd = (text) => {
  const [whole, fraction = ""] = String(text).replace(/^\$/, "").split(".");
  return `$${whole}.${fraction.replace(/0+$/, "").padEnd(2, "0")}`;
};
const USE_PATH = ["walletd", "arti", "tor", "onion", "gate"];
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const seconds = (ms) => `${(ms / 1000).toFixed(ms < 10000 ? 2 : 1)} s`;

let view = null;
let ready = false;
let manifestAccepted = false;
let credits = 0;
let conversationId = newId();
let history = [];
let sessionOpen = false;
let busy = false;
const settledSeen = new Set();
let lastSeq = 0;
// The console's clock: its last event's time, and when that arrived here.
let clock = { at: 0, seen: Date.now() };
const consoleNow = () => clock.at + (Date.now() - clock.seen);

function newId() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// ---- API ------------------------------------------------------------

async function api(path, options = {}) {
  const headers = Object.assign({ Authorization: `Bearer ${token}` }, options.headers || {});
  const response = await fetch(path, Object.assign({}, options, { headers }));
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    let body = null;
    try {
      body = await response.json();
      message = body.message ? `${body.error}: ${body.message}` : body.error || message;
      if (body.required) message += ` (a session needs ${fmt(body.required)} credits)`;
    } catch (_) { /* not JSON */ }
    const error = new Error(message);
    error.body = body;
    throw error;
  }
  return response;
}

const getJson = async (path) => (await api(path)).json();

async function postJson(path, body) {
  const response = await api(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return response.json();
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

// ---- The planes -----------------------------------------------------

function hops(plane) {
  return Array.from(document.querySelectorAll(`#plane-${plane} li`))
    .filter((hop) => hop.offsetParent !== null);
}

function lightPlane(plane, on) {
  $(`plane-${plane}`).classList.toggle("active", on);
  if (!on) hops(plane).forEach((hop) => hop.classList.remove("lit"));
}

// Lights the named hops of a plane for a moment.
function flash(plane, names, ms = 1800) {
  lightPlane(plane, true);
  const lit = hops(plane).filter((hop) => names.includes(hop.dataset.hop));
  lit.forEach((hop) => hop.classList.add("lit"));
  setTimeout(() => {
    lit.forEach((hop) => hop.classList.remove("lit"));
    if (!document.querySelector(`#plane-${plane} li.lit`)) $(`plane-${plane}`).classList.remove("active");
  }, ms);
}

// Lights the use plane hop by hop while a request travels out.
function travel() {
  lightPlane("use", true);
  const path = hops("use");
  let index = 0;
  const timer = setInterval(() => {
    if (index < path.length) path[index++].classList.add("lit");
    else clearInterval(timer);
  }, 140);
  return () => clearInterval(timer);
}

// ---- Activity log and bring-up timeline -------------------------------

function appendLog(event) {
  const log = $("log");
  const item = el("li", event.kind);
  item.append(el("span", "when", seconds(event.at_ms)), el("span", "source", event.source), el("span", "text", event.text));
  log.append(item);
  while (log.children.length > 1500) log.firstChild.remove();
  if ($("follow").checked) log.scrollTop = log.scrollHeight;
}

let torLine = null;

function appendTimeline(event) {
  const timeline = $("timeline");
  timeline.querySelectorAll("li.pending").forEach((item) => item.classList.remove("pending"));
  const marks = { ok: "✓", info: "•", error: "✗", warn: "!" };
  if (event.source === "tor") {
    // One line that follows C tor's bootstrap.
    if (!torLine) {
      torLine = el("li", "info");
      torLine.append(el("span", "mark", "•"), el("span", "text"), el("span", "when"));
      timeline.append(torLine);
    }
    torLine.querySelector(".text").textContent = `C tor: ${event.text}`;
    torLine.querySelector(".when").textContent = seconds(event.at_ms);
    if (event.text.startsWith("Bootstrapped 100%")) torLine.dataset.done = "yes";
    return;
  }
  const item = el("li", event.kind);
  item.append(el("span", "mark", marks[event.kind] || "•"), el("span", "text", event.text), el("span", "when", seconds(event.at_ms)));
  if (event.kind === "info") item.classList.add("pending");
  timeline.append(item);
  // Until it is done, C tor's line stays last, so the times read in order.
  if (torLine && !torLine.dataset.done) timeline.append(torLine);
}

function onEvent(event) {
  if (event.seq <= lastSeq) return;
  lastSeq = event.seq;
  clock = { at: event.at_ms, seen: Date.now() };
  appendLog(event);
  if (event.source === "xtask" || (event.source === "tor" && !ready)) appendTimeline(event);
  if (event.source === "wallet-cli" && event.kind === "command") {
    if (event.text.includes(" collect")) flash("purchase", ["cli", "storefront", "issue"]);
    else if (event.text.includes(" buy")) flash("purchase", ["cli", "storefront", "stripe"]);
    else if (event.text.includes(" manifest")) flash("purchase", ["cli", "storefront"]);
  }
  if (event.source === "fake-stripe") flash("purchase", ["stripe"]);
  if (event.source === "wallet" && event.kind === "ok") flash("purchase", ["storefront", "issue"], 2400);
  // The use-plane check: walletd fetching the gate's manifest, and what it found.
  if (event.source === "walletd" && (event.kind === "ok" || event.text.startsWith("fetching the gate's manifest"))) {
    flash("use", USE_PATH, 2400);
  }
  scheduleRefresh();
}

async function followEvents() {
  for (;;) {
    try {
      await readEvents(await api("/api/events"), (data) => onEvent(JSON.parse(data)));
    } catch (_) { /* the console restarted or the network blinked */ }
    await sleep(1500);
  }
}

// ---- State --------------------------------------------------------------

let refreshTimer = null;

function scheduleRefresh() {
  if (refreshTimer) return;
  refreshTimer = setTimeout(() => {
    refreshTimer = null;
    refresh().catch(() => {});
  }, 150);
}

async function refresh() {
  view = await getJson("/api/state");
  renderPhase();
  renderComponents();
  renderProducts();
  renderPurchases();
  renderModels();
  if (view.phase === "ready" && !ready) {
    ready = true;
    $("bringup").classList.add("collapsed");
    $("bringup-toggle").hidden = false;
    await loadManifest();
  }
  // walletd has no balance to show until a manifest is accepted.
  if (ready && manifestAccepted) await loadBalance();
  renderPathCheck();
  gate();
}

// What walletd found when it fetched the gate's manifest over the use plane.
function renderPathCheck() {
  const box = $("path-check");
  const check = view && view.use_plane_check;
  if (check) {
    box.textContent = `${check.ok ? "✓" : "!"} ${check.text}`;
    box.className = `path-check ${check.ok ? "ok" : "warn"}`;
  } else if (manifestAccepted) {
    box.textContent = view.over_tor
      ? "walletd is checking its own path to the gate: Arti, Tor relays, the onion service…"
      : "walletd is checking its own path to the gate…";
    box.className = "path-check";
  } else {
    return;
  }
  box.hidden = false;
}

let remoteShown = false;

// On a GPU instance: where the services run and what answers chat.
function renderRemote() {
  if (!view.remote || remoteShown) return;
  remoteShown = true;
  const remote = view.remote;
  $("bringup-lede").textContent = `The gate, the storefront, fake-stripe and C tor run on ${remote.host}, ` +
    `next to ${remote.engine}. This machine runs only walletd and wallet-cli. xtask opens an SSH tunnel for the ` +
    "storefront and the checkout page, and unlocks the gate with K through it if it is locked; walletd reaches the " +
    "gate over Tor, never through the tunnel.";
  const hop = $("engine-hop");
  hop.textContent = `${remote.engine.split(" ")[0]} `;
  hop.append(el("small", null, "on the instance"));
  $("transcript-empty").textContent =
    `Replies come from ${remote.engine}, streamed token by token; the gate meters each token as it passes.`;
}

function renderPhase() {
  const phase = $("phase");
  phase.dataset.phase = view.phase;
  document.body.classList.toggle("local", !view.over_tor);
  if (!view.over_tor) {
    $("use-plane-sub").textContent = "local socket · walletd";
    $("chat-transport").textContent = "the gate's local socket";
    $("tor-lede").hidden = true;
  }
  renderRemote();
  const badge = $("bringup-badge");
  if (view.phase === "ready") {
    const where = view.onion ? `; the gate is ${view.onion}` : "";
    const transport = view.remote ? "ready: chat goes over Tor to the instance" : "ready: chat goes over Tor";
    $("phase-text").textContent = view.over_tor ? transport : "ready (local, no Tor)";
    badge.textContent = `up in ${seconds(view.ready_after_ms)}`;
    badge.className = "badge ok";
    badge.title = `every service is up${where}`;
  } else if (view.phase === "failed") {
    $("phase-text").textContent = "bring-up failed";
    badge.textContent = "failed";
    badge.className = "badge error";
    const error = $("bringup-error");
    error.textContent = view.error;
    error.hidden = false;
    $("bringup").classList.remove("collapsed");
  } else {
    badge.className = "badge busy";
  }
}

function renderComponents() {
  if (!view.components.length) return;
  const list = $("components");
  list.replaceChildren();
  for (const component of view.components) {
    const item = el("li");
    const name = el("span", "name", component.name);
    name.append(el("span", `tag plane-${component.plane}`, component.plane === "both" ? "both planes" : `${component.plane} plane`));
    item.append(el("span", "dot"), name, el("span", "detail", component.detail));
    list.append(item);
  }
}

function renderProducts() {
  const box = $("products");
  if (box.children.length || !view.products.length) return;
  for (const product of view.products.slice().sort((a, b) => a.usd_cents - b.usd_cents)) {
    const button = el("button", "product");
    button.type = "button";
    button.append(el("span", "price", `$${(product.usd_cents / 100).toFixed(2)}`),
      el("span", "what", `${fmt(product.credits)} credits · ${product.id}`));
    button.addEventListener("click", () => buy(product.id));
    box.append(button);
  }
}

function renderPurchases() {
  const list = $("purchases");
  list.replaceChildren();
  for (const purchase of view.purchases.slice().reverse()) {
    const item = el("li", "purchase");
    const label = el("span", "label", `${purchase.product} · $${(purchase.usd_cents / 100).toFixed(2)} · ${fmt(purchase.credits)} credits`);
    label.append(el("small", null, `order ${purchase.order_id}`));
    item.append(label);
    if (purchase.status === "awaiting_payment") {
      const link = el("a", "checkout", "Pay on the checkout page ↗");
      link.href = purchase.checkout_url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      item.append(link, el("span", "badge busy", "awaiting payment"));
    } else {
      item.append(el("span", "badge ok", `collected → chain ${purchase.chain}`));
    }
    list.append(item);
  }
  const waiting = view.purchases.some((p) => p.status === "awaiting_payment");
  const collected = view.purchases.some((p) => p.status === "collected");
  setBadge("buy-badge", waiting ? "awaiting payment" : collected ? "credit collected" : manifestAccepted ? "ready" : "waiting",
    waiting ? "busy" : collected ? "ok" : "");
}

function renderModels() {
  const select = $("model");
  if (select.options.length || !view.models.length) return;
  for (const model of view.models) {
    const option = el("option", null, model.id);
    option.value = model.id;
    select.append(option);
  }
  showRates();
}

function showRates() {
  const model = view && view.models.find((m) => m.id === $("model").value);
  if (model) $("rates").textContent = `${model.input_rate} credit${model.input_rate === 1 ? "" : "s"} per prompt token, ${model.output_rate} per output token`;
}

function setBadge(id, text, kind) {
  const badge = $(id);
  badge.textContent = text;
  badge.className = `badge ${kind || ""}`.trim();
}

// Unlocks each step once the one before it is done.
function gate() {
  $("step-manifest").classList.toggle("locked", !ready);
  $("step-buy").classList.toggle("locked", !manifestAccepted);
  $("step-chat").classList.toggle("locked", credits <= 0 && !sessionOpen);
  const canChat = ready && (credits > 0 || sessionOpen) && !busy;
  $("prompt").disabled = !canChat;
  $("send").disabled = !canChat;
  $("settle").disabled = !sessionOpen || busy;
  if (!busy && ready) {
    setBadge("chat-badge", sessionOpen ? "session open" : credits > 0 ? "ready" : "needs credit", sessionOpen ? "busy" : credits > 0 ? "ok" : "");
  }
}

// ---- Wallet -------------------------------------------------------------

async function loadBalance() {
  let balance;
  try {
    balance = await getJson("/api/wallet/wallet/balance");
  } catch (_) {
    return; // walletd is busy; the next poll asks again
  }
  const total = $("total-credits");
  if (balance.total_credits !== credits) {
    total.classList.add("changed");
    setTimeout(() => total.classList.remove("changed"), 1200);
  }
  credits = balance.total_credits;
  total.textContent = fmt(balance.total_credits);
  $("total-usd").textContent = usd(balance.total_usd);
  const chains = $("chains");
  chains.replaceChildren();
  for (const chain of balance.chains) {
    const state = String(chain.state).toLowerCase();
    const item = el("li");
    item.append(el("span", null, `chain ${chain.id}: ${fmt(chain.credits)}`), el("span", `chip ${state}`, chain.resolved ? state : `${state} · unsettled`));
    chains.append(item);
  }
  const estimates = $("estimates");
  estimates.replaceChildren();
  const remaining = balance.total_credits > 0 ? Object.entries(balance.estimated_tokens_remaining) : [];
  for (const [model, estimate] of remaining) {
    estimates.append(el("li", null, `${model}: up to ${fmt(estimate.output_tokens_at_most)} output tokens`));
  }
  const notes = [];
  if (balance.orders_awaiting_payment.length) notes.push(`${balance.orders_awaiting_payment.length} order(s) awaiting payment`);
  if (balance.dust_credits) notes.push(`${fmt(balance.dust_credits)} credits of dust`);
  if (balance.frozen_orders || balance.frozen_chains) notes.push("frozen items need investigation");
  if (balance.experimental_platform) notes.push("dev attestation: not a real TEE");
  $("wallet-note").textContent = notes.join(" · ");
  gate();
}

// ---- Step 1: manifest ---------------------------------------------------

function showTerminal(id, run) {
  const box = $(id);
  box.hidden = false;
  box.append(el("span", "cmd", `$ ${run.command}\n`));
  for (const line of run.stdout) box.append(document.createTextNode(`${line}\n`));
  for (const line of run.stderr) box.append(el("span", run.ok ? null : "err", `${line}\n`));
  box.scrollTop = box.scrollHeight;
}

async function loadManifest() {
  const status = await getJson("/api/wallet/wallet/manifest");
  manifestAccepted = Boolean(status.accepted);
  const summary = status.accepted || status.pending;
  if (summary) {
    const facts = $("manifest-facts");
    facts.replaceChildren();
    const distinct = (list) => [...new Set(list)].join(", ");
    const rows = [
      ["version", `v${summary.version}`],
      ["releases", String(summary.image_versions.length)],
      ["images", distinct(summary.image_versions)],
      ["platforms", distinct(summary.platforms)],
      ["signed by", "the operator key pinned in wallet.toml ✓"],
    ];
    for (const [term, value] of rows) facts.append(el("dt", null, term), el("dd", null, value));
    facts.hidden = false;
  }
  $("manifest-fetch").disabled = manifestAccepted;
  $("manifest-approve").disabled = manifestAccepted || !status.pending;
  if (manifestAccepted) setBadge("manifest-badge", `v${status.accepted.version} accepted`, "ok");
  else if (status.pending) setBadge("manifest-badge", `v${status.pending.version} awaiting approval`, "busy");
  else setBadge("manifest-badge", "not fetched", "");
  gate();
}

async function manifestAction(action) {
  setBadge("manifest-badge", action === "fetch" ? "fetching…" : "approving…", "busy");
  $("manifest-fetch").disabled = true;
  $("manifest-approve").disabled = true;
  try {
    showTerminal("manifest-terminal", await postJson("/api/manifest", { action }));
  } catch (error) {
    showTerminal("manifest-terminal", { command: `wallet-cli manifest ${action}`, ok: false, stdout: [], stderr: [error.message] });
  }
  await loadManifest();
  if (manifestAccepted) await loadBalance();
  renderPurchases();
  renderPathCheck();
}

// ---- Step 2: buy ----------------------------------------------------------

async function buy(product) {
  document.querySelectorAll(".product").forEach((button) => { button.disabled = true; });
  setBadge("buy-badge", "ordering…", "busy");
  try {
    const response = await fetch("/api/buy", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ product }),
    });
    const body = await response.json();
    if (body.run) showTerminal("buy-terminal", body.run);
    else showTerminal("buy-terminal", { command: `wallet-cli buy --product ${product}`, ok: false, stdout: [], stderr: [body.message || body.error] });
  } catch (error) {
    showTerminal("buy-terminal", { command: `wallet-cli buy --product ${product}`, ok: false, stdout: [], stderr: [error.message] });
  } finally {
    document.querySelectorAll(".product").forEach((button) => { button.disabled = false; });
    await refresh();
  }
}

// ---- Step 3: chat ---------------------------------------------------------

function appendMessage(role, text) {
  $("transcript-empty").hidden = true;
  const block = el("div", `message ${role}`);
  const body = el("p", null, text);
  block.append(body);
  $("transcript").append(block);
  return block;
}

function appendReceiptLine(className) {
  const line = el("div", `receipt ${className || ""}`.trim());
  $("transcript").append(line);
  $("transcript").scrollTop = $("transcript").scrollHeight;
  return line;
}

function fillReceipt(line, parts) {
  line.replaceChildren();
  for (const [label, value] of parts) {
    const part = el("span");
    if (label) part.append(`${label} `);
    part.append(el("strong", null, value));
    line.append(part);
  }
}

async function send(event) {
  event.preventDefault();
  const prompt = $("prompt").value.trim();
  if (!prompt || busy) return;
  $("prompt").value = "";
  busy = true;
  gate();
  setBadge("chat-badge", sessionOpen ? "sending…" : "opening a session…", "busy");
  history.push({ role: "user", content: prompt });
  appendMessage("user", prompt);
  const reply = appendMessage("assistant", "");
  reply.classList.add("streaming");
  const body = reply.querySelector("p");
  const via = view.over_tor ? "over Tor" : "on the local socket";
  const line = appendReceiptLine();
  const waiting = sessionOpen
    ? `sending ${via}…`
    : `walletd is spending part of a credential to open a session ${via}${view.over_tor ? " on a fresh circuit" : ""}…`;
  line.textContent = waiting;
  const stop = travel();
  const started = performance.now();
  // Until walletd answers, the wait counts up: a fresh Tor path takes a while.
  const ticker = setInterval(() => {
    line.textContent = `${waiting} ${Math.floor((performance.now() - started) / 1000)} s`;
  }, 500);
  let firstToken = null;
  let receipt = null;
  const model = $("model").value;
  try {
    const response = await api("/api/wallet/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Conversation-Id": conversationId },
      body: JSON.stringify({ model, messages: history, stream: true }),
    });
    clearInterval(ticker);
    const notes = [];
    if (response.headers.get("x-counteractive-rollover")) notes.push("walletd rolled over to a new session");
    if (response.headers.get("x-counteractive-recovered")) notes.push("walletd recovered a lost session");
    sessionOpen = true;
    line.textContent = `session open; streaming ${via}…`;
    await readEvents(response, (data) => {
      if (data === "[DONE]") return;
      const chunk = JSON.parse(data);
      const text = chunk.choices && chunk.choices[0] && chunk.choices[0].delta && chunk.choices[0].delta.content;
      if (text) {
        if (firstToken === null) firstToken = performance.now() - started;
        body.textContent += text;
        $("transcript").scrollTop = $("transcript").scrollHeight;
      }
      if (chunk.receipt) receipt = chunk.receipt;
    });
    const total = performance.now() - started;
    history.push({ role: "assistant", content: body.textContent });
    if (receipt) {
      fillReceipt(line, [
        ["turn", String(receipt.turn)],
        [null, receipt.model],
        ["tokens", `${fmt(receipt.prompt_tokens)} in + ${fmt(receipt.completion_tokens)} out`],
        ["charged", `${fmt(receipt.charge)} credits`],
        ["session has", `${fmt(receipt.remaining)} left`],
        ["first token", seconds(firstToken || total)],
        ["done", seconds(total)],
      ]);
      for (const note of notes) line.append(el("span", null, note));
      onLocalEvent(`turn ${receipt.turn} on ${receipt.model}: ${receipt.prompt_tokens} + ${receipt.completion_tokens} tokens, charged ${receipt.charge}; first token after ${seconds(firstToken || total)}`);
    } else {
      line.textContent = "the turn ended without a receipt";
      line.classList.add("error");
    }
  } catch (error) {
    history.pop();
    if (!body.textContent) reply.remove();
    line.textContent = `error: ${error.message}`;
    line.classList.add("error");
    if (!$("prompt").value) $("prompt").value = prompt;
  } finally {
    clearInterval(ticker);
    stop();
    reply.classList.remove("streaming");
    lightPlane("use", false);
    busy = false;
    await loadBalance();
    gate();
  }
}

// Ends the session: walletd closes it at the gate, which refunds the
// unspent credit as a fresh credential; reconcile then checks the receipts
// against what the session took.
async function settle() {
  if (!sessionOpen || busy) return;
  busy = true;
  gate();
  setBadge("chat-badge", "settling…", "busy");
  const stop = travel();
  const box = $("settlement");
  try {
    const closed = await postJson("/api/wallet/wallet/close", { conversation_id: conversationId });
    const report = await postJson("/api/wallet/wallet/reconcile", {});
    const fresh = report.sessions.filter((s) => !settledSeen.has(`${s.chain_id}:${s.spend_seq}`));
    report.sessions.forEach((s) => settledSeen.add(`${s.chain_id}:${s.spend_seq}`));
    box.replaceChildren();
    box.className = "settlement";
    box.append(el("h3", null, `Settled ${closed.closed} session${closed.closed === 1 ? "" : "s"}`));
    for (const s of fresh) {
      const exact = s.outcome === "exact";
      if (!exact) box.classList.add("mismatch");
      box.append(el("div", "sum",
        `chain ${s.chain_id}: took s = ${fmt(s.s)}, refunded t = ${fmt(s.t)}; ` +
        `receipts ${fmt(s.receipts_sum)} ${exact ? "= s − t ✓ exact" : `vs s − t = ${fmt(s.s - s.t)}: ${s.outcome}`}`));
    }
    box.append(el("p", "muted", "The refund came back as a fresh credential: the next session can't be linked to this one."));
    box.hidden = false;
    const divider = appendReceiptLine();
    divider.textContent = "— session settled; the next message opens a new, unlinkable one —";
    sessionOpen = false;
    conversationId = newId();
    history = [];
  } catch (error) {
    box.replaceChildren(el("p", "error-text", `error: ${error.message}`));
    box.hidden = false;
  } finally {
    stop();
    lightPlane("use", false);
    busy = false;
    await loadBalance();
    gate();
  }
}

// An entry the page adds to its own activity log.
function onLocalEvent(text) {
  appendLog({ at_ms: consoleNow(), source: "page", kind: "info", text });
}

// ---- Start ----------------------------------------------------------------

function guarded(action) {
  return (...args) => action(...args).catch((error) => onLocalEvent(`error: ${error.message}`));
}

$("manifest-fetch").addEventListener("click", guarded(() => manifestAction("fetch")));
$("manifest-approve").addEventListener("click", guarded(() => manifestAction("approve")));
$("composer").addEventListener("submit", guarded(send));
$("prompt").addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    $("composer").requestSubmit();
  }
});
$("settle").addEventListener("click", guarded(settle));
$("model").addEventListener("change", showRates);
$("bringup-toggle").addEventListener("click", () => {
  const collapsed = $("bringup").classList.toggle("collapsed");
  $("bringup-toggle").textContent = collapsed ? "show each step" : "hide the steps";
});

setInterval(() => {
  if (!view || view.phase !== "starting") return;
  $("phase-text").textContent = `starting the stack… ${Math.round(consoleNow() / 1000)} s`;
}, 1000);
setInterval(() => { if (ready && manifestAccepted && !busy) loadBalance().catch(() => {}); }, 4000);

followEvents();
refresh().catch(() => {});
