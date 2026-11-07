# Prototype contract

This is the contract every crate in `prototype/` is built against. The architecture it implements is [`../ARCHITECTURE.md`](../ARCHITECTURE.md); deeper behaviour per component is in [`../docs/architecture/`](../docs/architecture/README.md) (03 code, 04 wallet, 05 gate, 07 manifest). Where this file is more specific, follow this file. Where a doc mentions vouchers, RFC 9578, blind RSA or webhooks, ignore it: decision 6 removed them.

The prototype runs on one laptop. It replaces only what a laptop can't provide, and says so loudly:

| Real system | Prototype stand-in | Same code above it? |
|---|---|---|
| SEV-SNP report and GPU evidence | `dev-insecure` platform: an Ed25519 signature over the same `REPORT_DATA` | Yes: binding, role, release selection, TLS checks |
| vLLM on a GPU | `fake-engine`, an OpenAI-compatible server with vLLM's `/tokenize` shape | Yes: the `openai` backend adapter |
| Stripe API | `fake-stripe`, the two Checkout Session endpoints plus a test pay call | Yes: the storefront's HTTP client, pointed at another base URL |
| Tor | Tier 1: the wallet dials the gate's Unix socket directly. Tier 2: C tor onion service plus Arti | Yes: the same TLS, attestation and HTTP code over a different `Dialer` |
| Compiled-in wallet pins | `wallet.toml`, written by `xtask` | Read through one `Pins` type, so a release build can swap in constants |
| Measured image, systemd, firewall | Processes started by `xtask` | No |

Not in the prototype: drain, export, import, SNP verification, real GPU appraisal, the boot collector, the image, release signing CI, proof-of-work, the OS keychain.

## 1. Workspace

`prototype/` is one Cargo workspace, edition 2024, stable Rust (1.94 here). Crates and their jobs are in ARCHITECTURE section 8.1. Layering rules in section 8.2 apply, including: only `credit` names `act` types, domain crates do no IO, secrets never reach `Debug` or logs.

`act-rs` is a git dependency: `act-rs = { git = "https://github.com/Moderation-of-unLinkable-Endorsements/act-rs", rev = "0fe1d8f75ba1fa40e9b464b27d747555f560c889" }`, library name `act`. Its public API: `Params::new(L)`, `SecretKey::from_seed`, `DefaultBackend::generate_seed()` (48 bytes), `issue_request`, `verify_issue_request`, `issue_response`, `finalize_issue`, `prove_spend(params, cred, ctx_cred, s, a, ctx_spend)`, `verify_spend`, `issue_refund(verified, t)`, `finalize_refund`, and `to_bytes`/`from_bytes` on every message, `Credential`, `ClientIssuanceState` and `ClientSpendState`. `VerifiedSpend` borrows its input. `prove_spend` and the `finalize_*` functions consume their input.

## 2. Encodings and constants

- Binary in JSON: base64url without padding, written `b64` below. Hashes: lowercase hex.
- Canonical JSON for signing: `serde_json::to_vec` of a `serde_json::Value` (default `Map` is sorted), no whitespace.
- Signatures over manifests: minisign (`minisign` crate to sign, `minisign-verify` to check, `allow_legacy = false`). The signature travels as the full minisign signature text.
- Balance width `L = 24`, so a balance is at most 16,777,215 credits. The allowance `a` is always 0; the gate rejects anything else with `400`.
- `ctx_cred = SHA-256("counteractive-cred-v1" || u8(len(service_id)) || service_id || u32be(key_epoch))`
- `ctx_spend = SHA-256("counteractive-spend-v1" || u8(len(service_id)) || service_id)`
- Refund sealing: `seal_key = HKDF-SHA256(salt = "counteractive-seal-v1", ikm = SHA-256(spend_bytes), info = "refund")`, then `sealed = nonce(24) || XChaCha20-Poly1305(seal_key, nonce, refund_bytes)`.
- Session tier ladder (wallet pin): `[500000, 50000, 5000]`. `s` = largest tier not above `min(balance, maximum_spend)`. Below the smallest tier the chain is `Dust`.

## 3. Attestation (dev-insecure)

```text
lp(x)       = u16be(len(x)) || x
report_data = SHA-512( lp("counteractive-report-v1") || lp(role) || spki_sha256 || gpu_evidence_sha256
                       || nonce || release_config_sha256 )        # role is "gate" or "storefront"
report      = Ed25519-Sign(dev_attestation_key, report_data)       # 64 bytes
```

- `spki_sha256` is SHA-256 of the DER SubjectPublicKeyInfo of the server's TLS leaf certificate. The server knows it from its key pair; the client extracts it from the presented certificate (`x509-parser`).
- `gpu_evidence_sha256` is 32 zero bytes in the prototype.
- Evidence JSON: `{"platform":"dev-insecure","role":"gate","release_config_sha256":hex,"gpu_evidence_sha256":hex,"report":b64}`.
- The verifier recomputes `report_data` from **its own** recorded SPKI hash, the role it expects, and the nonce it sent. It selects the manifest release whose `release_config_sha256` equals the evidence's, requires `attestation_policy.kind == "dev-insecure"`, and checks the signature with that release's `dev_public_key`. Any mismatch rejects. A wallet accepts `dev-insecure` only when its pins say `allow_dev_attestation = true`, and then prints a warning on every connection.
- `snp` exists as a verifier name that returns `Unsupported` in the prototype.

## 4. Inner TLS and connections

- Each server generates a P-256 or Ed25519 key and a self-signed certificate (`rcgen`) in memory at startup. Nothing persists.
- Server: TLS 1.3 only, no tickets, no session storage, ALPN `http/1.1`, hyper http1 per connection, handshake timeout 10 s, spawned per connection.
- Client: TLS 1.3 only, resumption disabled, SNI off, ALPN `http/1.1`, fixed server name `gate.invalid` or `storefront.invalid`. A custom `ServerCertVerifier` accepts the certificate without a CA but **does verify the TLS 1.3 handshake signature** with the provider's algorithms, and records the leaf's SPKI hash.
- `gate-conn` types: `Dialer` (`UnixDialer`, `TcpDialer` behind feature `tcp`, `ArtiDialer` behind feature `arti`), `Target` (`unix:/path`, `tcp:host:port`, `onion:host.onion:443`), `UnverifiedConn` (TLS done, SPKI recorded; can only fetch health and manifest, and attest), `AttestedConn` (only constructed by a successful attest; can send anything; supports streaming response bodies for SSE). A verdict cache keyed by SPKI hash lets a new connection to the same server skip a fresh evidence fetch for up to 1 hour.
- `gate-conn` also has a `server` feature with the shared server pieces: identity generation, the rustls acceptor config, and a serve loop over a `UnixListener` or `TcpListener` that runs an axum router.

## 5. Gate

Two Unix sockets in its run directory: `gate.sock` (inner TLS; tor forwards here) and `issue.sock` (plain HTTP/1.1, for the storefront only). Its engine is reached at `engine.sock`. States: `Locked`, `AwaitingManifest`, `Ready`. Unserved routes in a state return `503 {"error":"locked"}`.

| Endpoint | Request | Response | Errors |
|---|---|---|---|
| `GET /v1/health` | | `{state, version}` | |
| `GET /v1/manifest` | | `{manifest: b64(canonical bytes), signature: text}` | `503` if none |
| `POST /v1/attestation` | `{nonce: b64(32)}` | evidence, role `gate` | |
| `POST /v1/sessions` | `{spend: b64}` | `{status:"open", session_id: hex(16 bytes), budget: s, policy}` or `{status:"settled", refund: b64}` | `400`, `409 spent`, `409 use_recover`, `503 retry` |
| `POST /v1/sessions/{id}/chat` | `{model, messages:[{role,content}], max_completion_tokens?}` | SSE (below) | `400 model`, `400 zero_output`, `402 session_exhausted`, `402 prompt_exceeds_budget {required}`, `404`, `409 turn_active`, `409 closing` |
| `POST /v1/sessions/{id}/close` | | `{refund: b64}` | `404`, `503 retry` |
| `POST /v1/sessions/recover` | `{spend: b64}` | `{status:"settled", refund}` or `{status:"live", session_id}` | `400`, `404 no_row`, `409 spent`, `503 retry` |
| `POST /v1/operator/unlock` | bearer `hex(K)` | `{state, issuer_public_key: b64}` | `401` |
| `POST /v1/operator/manifest` | bearer, `{manifest: b64, signature}` | `{state}` | `401`, `409 manifest_mismatch` |
| `GET /v1/operator/state` | bearer | `{state, live_sessions}` | `401` |
| `issue.sock POST /local/issue` | `{order_ref: hex(32), credits, issue_request: b64}` | `{issue_response: b64}` | `400`, `409 order_issued`, `503 locked` |

Errors are JSON `{"error": "<code>", ...}` with the status shown.

- **Unlock:** `SHA-256(K)` must equal `release.json`'s `k_sha256` (constant-time compare). With no store: create it (SQLCipher, raw key K, under `<data>/store/current/gate.db`, where `current` is a relative symlink to `gen-1`), generate the 48-byte seed, record `key_epoch` from `release.json`, state `AwaitingManifest`. With a store and an accepted manifest recorded: `Ready`. Always return the issuer public key.
- **Manifest:** verify the minisign signature with `release.json`'s `operator_public_key`; require `service.issuer_public_key` and `service.key_epoch` to equal the store's, `version >= manifest_version`, and a release whose `release_config_sha256` equals SHA-256 of the gate's own canonical `release.json`. Store `manifest_version`, write the manifest beside the store, go `Ready`.
- **Store schema, sealing, slots, opening order, admission table, one turn, metering, one debit rule, closing (including the closing task retrying its own disk write), recover table (read slot and store under the slot lock), idle and age timers:** exactly as `docs/architecture/05-component-gate.md` sections 4 and 5, with the `issued` table from ARCHITECTURE section 9 in place of `voucher`.
- **Chat SSE** from the gate: each content chunk as `data: {"choices":[{"index":0,"delta":{"content":"..."}}]}`; then `event: receipt` with `data: {"turn", "model", "prompt_tokens", "completion_tokens", "charge", "remaining", "finish"}`; then `event: done` with `data: {}`. A stream that ends without a receipt is an unknown turn to the wallet.
- **Engine body** sent upstream: `stream: true`, `stream_options: {include_usage: true}`, `max_completion_tokens: allowed_output`. Prompt tokens come from `POST /tokenize {model, messages}` → `{count}` on the same messages.

## 6. Storefront

Listens on TCP (default `127.0.0.1:8443`) with inner TLS and role `storefront`. Talks to Stripe through `PaymentProvider` over `reqwest` with a configurable base URL (`https://api.stripe.com` or fake-stripe), and to the gate through `issue.sock`. Stateless: no database.

| Endpoint | Request | Response | Errors |
|---|---|---|---|
| `GET /v1/health` | | `{state: "unconfigured" or "ready"}` | |
| `GET /v1/manifest` | | as the gate's; read from a manifest file path in its config | `503` |
| `POST /v1/attestation` | `{nonce}` | evidence, role `storefront` | |
| `POST /v1/orders` | `{product, secret_hash: hex}` | `{order_id, checkout_url}` | `400`, `503 unconfigured` |
| `POST /v1/orders/{id}/collect` | `{order_secret: hex, issue_request: b64}` | `{issue_response: b64, credits}` | `401`, `402 unpaid`, `404`, `409 order_issued`, `409 amount_mismatch`, `503 locked` |
| `POST /v1/operator/configure` | bearer `hex(token)`, `{stripe_key}` | `{state}` | `401` |

Stripe calls, form-encoded with `Authorization: Bearer <stripe_key>`:

- `POST /v1/checkout/sessions`: `mode=payment`, `line_items[0][quantity]=1`, `line_items[0][price_data][currency]=usd`, `line_items[0][price_data][unit_amount]=<usd_cents>`, `line_items[0][price_data][product_data][name]=<product id>`, `client_reference_id=<secret_hash>`, `metadata[product]=<product id>`, `success_url=<config>`. Read `id` and `url`.
- `GET /v1/checkout/sessions/{id}`: read only `id`, `status`, `payment_status`, `amount_total`, `currency`, `client_reference_id`, `metadata.product`. Never log the body.

Collect order: session lookup (`404` if Stripe says no such session), `SHA-256(order_secret) == client_reference_id` (`401`), `payment_status == "paid"` (`402`), product exists and `amount_total`/`currency` match it (`409 amount_mismatch`), then `issue.sock` with `order_ref = SHA-256(order_id)` and the product's credits.

`fake-stripe` serves those two endpoints with the same shapes (in memory), checks the bearer key is non-empty, returns `url = http://<its addr>/pay/<id>` (a page with a Pay button that POSTs), and has `POST /test/pay/{id}` to mark a session paid from a test.

## 7. Wallet

- `wallet-core`: journal (SQLCipher, key in `<wallet_dir>/journal.key`, mode 0600, standing in for the keychain), chain state machine (ARCHITECTURE section 11), balance view and reconcile (docs 04 sections 7 and 10), manifest store with accept rule (docs 04 section 6, using the pins).
- `wallet-cli` (TCP dialer only): `init`, `manifest fetch [--approve]` (from the storefront), `buy --product <id> [--no-browser]` (journal, order, print the checkout URL, open the browser unless told not to), `collect [--wait <secs>]` (poll until paid, then issue and journal a Ready chain), `balance`, `models`, `chat --model <id> [--conversation <id>] <prompt>`, `close`, `reconcile`. The last five call `walletd` over localhost.
- `walletd` (Unix or Arti dialer only, never TCP): local API on `127.0.0.1:7777`, bearer token in `<wallet_dir>/walletd.token`, `Host` header check. `GET /` page (model selector, streaming chat, balance with reconcile), `GET /paid` (static, no auth), `GET /v1/models`, `POST /v1/chat/completions` (OpenAI-compatible, streaming, optional `X-Conversation-Id`), `GET /wallet/balance`, `POST /wallet/close`, `POST /wallet/reconcile`, `GET /wallet/manifest`, `POST /wallet/manifest {action: "fetch" | "approve"}`. One open session per conversation; sessions idle for 5 minutes are closed by `walletd`.
- Pins, from `wallet.toml`: `service_id`, `key_epoch`, `gate_target`, `storefront_addr`, `operator_public_key`, `tier_ladder`, `allow_dev_attestation`.

## 8. Fake engine

HTTP/1.1 on a Unix socket. `GET /health`; `POST /tokenize {model, messages}` → `{count, max_model_len: 4096}`, where count is 4 per message plus whitespace-separated words of each content; `POST /v1/chat/completions` streaming OpenAI chunks, a final chunk with `usage {prompt_tokens, completion_tokens}`, then `data: [DONE]`. Output is deterministic words, `min(max_completion_tokens, 16 + words(prompt) % 17)` of them, one per chunk. Any model name is accepted. Fault hooks triggered by text in the last user message: `[[stall:<ms>]]` pauses before the first token, `[[die-after:<n>]]` closes the connection after n tokens with no usage chunk, `[[long]]` produces the full limit.

## 9. Configuration and `xtask`

`xtask` writes a dev tree (default `prototype/.dev/`, gitignored):

```text
.dev/
├── keys/           k.bin, operator.key, operator.pub, dev_attest.key, storefront.token
├── release.json    products usd-5 (500 cents, 5,000,000 credits) and usd-1 (100 cents, 1,000,000 credits);
│                   catalogue fake-small (in 1, out 3) and fake-large (in 4, out 12); policy as docs 07
├── manifest.json, manifest.sig
├── run/            gate.sock, issue.sock, engine.sock
├── gate-data/
└── wallet/         journal.db, journal.key, wallet.toml, walletd.token
```

Commands: `keygen`, `release`, `up` (start fake-engine, fake-stripe, gate, storefront in the background, with logs in `.dev/logs/`), `unlock` (gate unlock, sign the manifest with the returned issuer key, post it, configure the storefront), `down`, and `e2e [--tor]`.

## 10. End-to-end acceptance

`cargo run -p xtask -- e2e` runs these in a temp dir and exits non-zero on any failure. Each step prints what it checked.

1. **Bring-up.** Keygen, release, start services, unlock with the bootstrap flow, sign and post the manifest; gate `ready`, storefront `ready`.
2. **Attestation negatives.** A verifier expecting role `gate` rejects the storefront's evidence; a wrong nonce rejects; a manifest without the matching release rejects; a manifest signed by another key isn't accepted by the wallet.
3. **Purchase.** `wallet init`, `manifest fetch --approve`, `buy usd-5 --no-browser`; `collect` returns `402` before payment; pay through fake-stripe; collect gives a Ready chain of 5,000,000 credits.
4. **Issuance idempotency.** Re-collecting with the same request returns identical bytes; a different request for the same order gets `409 order_issued`; a wrong secret gets `401`.
5. **Chat and metering.** `walletd` up; two streamed turns on `fake-small`; receipts match the fake engine's counts times the rates; one turn on `fake-large` in another conversation; models list shows both.
6. **Close and reconcile.** Close; balance equals 5,000,000 minus the sum of charges exactly; reconcile reports `exact` for every settled session.
7. **Replay.** Resending a settled session's spend bytes to open returns `409 use_recover`, and recover returns the byte-identical refund. A copy of the journal from before a spend, used to prove a second spend from the same credential, gets `409 spent`.
8. **Gate crash.** Open a session, finish one turn, `kill -9` the gate, restart and unlock it; the wallet's next turn sees `404`, recovers, and gets `t = s`; the balance returns to its value before that session.
9. **Lost close response and idle close.** With `idle_seconds` set to 2 in a separate release, an untouched session is closed by the gate; the wallet's close gets `404`, recover returns the exact refund.
10. **Budget errors.** A prompt larger than any tier gets `402 prompt_exceeds_budget`; a turn when the session can't cover the next hold gets `402 session_exhausted` and the wallet rolls over once.
11. **Two-stack check.** `cargo tree -p wallet-cli` has no `arti-client`; `cargo tree -p walletd` has no `reqwest` and no TCP dialer feature; `cargo tree -p gate` has no `reqwest` or `arti-client`.
12. **Tier 2, with `--tor` only.** Same as 3 to 6 with the gate behind a local C tor onion service and `walletd` on `ArtiDialer`. Skipped with a clear message when `tor` isn't installed.
