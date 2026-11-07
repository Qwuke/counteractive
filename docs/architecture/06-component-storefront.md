# 06. Component: storefront

The storefront sells credit. It creates Stripe Checkout Sessions, confirms payment by asking Stripe, and then asks the gate to issue an EC-ACT credential against the wallet's blinded issue request. It runs inside the CVM, on the same Verda instance as the gate, as its own process and user `storefront`, on TCP port 8443. Wallets reach it over clearnet with attested TLS, role `storefront`, so it needs no domain name and no public certificate. It stores nothing. [ARCHITECTURE.md section 10](../../ARCHITECTURE.md#10-storefront) is the authority, and where the two disagree, it wins.

It sees the user's IP, the order, the product, and the blinded issue request and response. It never sees nullifiers, spends or prompts ([ARCHITECTURE section 7](../../ARCHITECTURE.md#7-what-each-party-can-see)). It holds no credential key, and its only route to the gate is `issue.sock`.

**Stripe sandbox only.** "Stored value" is on Stripe's restricted list. The seller-maintained case may be allowed, but that's for Stripe to confirm, and the PoC doesn't need live keys. Test cards only. [read]

## 1. Directory structure

```text
crates/storefront/src/
├── main.rs        startup, TLS, listener on :8443
├── config.rs      products from release.json, operator token hash
├── stripe.rs      create and retrieve Checkout Sessions (PaymentProvider over reqwest)
├── gate.rs        issue.sock client
├── tls.rs         per-boot key, evidence with role Storefront
└── api.rs         the endpoints in section 2
```

## 2. API

| Endpoint | Request | Response | Errors |
|---|---|---|---|
| `GET /v1/health` | | `{state}`: `unconfigured` or `ready` | |
| `GET /v1/manifest` | | manifest and signature, as the gate serves them | `503` if none |
| `POST /v1/attestation` | `{nonce}` | evidence with role `storefront` | `503 busy` |
| `POST /v1/orders` | `{product, secret_hash}` | `{order_id, checkout_url}` | `400`, `429`, `503 unconfigured` |
| `POST /v1/orders/{id}/collect` | `{order_secret, issue_request}` | `{issue_response, credits}` | `400`, `401`, `402 unpaid`, `404`, `409 order_issued`, `409 amount_mismatch`, `503 locked` |
| `POST /v1/operator/configure` | `{stripe_key}`, operator token as bearer | `{state}` | `401` |

Until it's configured, orders and collect return `503 unconfigured`. `secret_hash` is `SHA-256(order_secret)`, and `order_id` is the Checkout Session ID. A `400` on collect means a malformed body, or the gate rejected the issue request.

**The browser never visits the storefront.** The wallet talks to it over attested TLS, so no domain name or public certificate is needed. A browser-facing storefront page would need both, on port 443, and that's deferred. The browser goes only to Stripe's Checkout page, and then to `walletd`'s local `/paid` page.

**The manifest** served here is the same signed file the gate serves, read from a path in the storefront's config. A wallet fetches it here only during a purchase, and it stays pending until the user approves it ([04 section 6](04-component-wallet.md#6-attestation-check)).

## 3. Payment

**Prices.** Currency is USD. Products come from `release.json`, which is measured: `{id, usd_cents, credits}`. The storefront sends `price_data` inline, so no Stripe Price objects need creating, and what a user is charged stays part of what's attested.

**Creating a session.** Form-encoded, with the restricted key as bearer.

```text
POST /v1/checkout/sessions
Stripe-Version: 2026-10-28.endive

mode=payment
line_items[0][quantity]=1
line_items[0][price_data][currency]=usd
line_items[0][price_data][unit_amount]=<usd_cents>
line_items[0][price_data][product_data][name]=<product id>
client_reference_id=<secret_hash>
metadata[product]=<product id>
success_url=http://127.0.0.1:<walletd port>/paid
```

The storefront reads only `id` and `url` from the response, and returns them as `order_id` and `checkout_url`.

`client_reference_id` is the hash of the order secret, so only the wallet that created the order can collect it. `metadata[product]` lets collect check the amount. There's no `customer` and no `customer_email`, and successful-payment receipts are off in the Dashboard. Stripe will ask for an email anyway, and its docs give no off switch. Stripe also groups repeat guests by card, email or phone. Blind issuance protects the link from purchase to use, not the purchase itself. [read]

**Reading a session.**

```text
GET /v1/checkout/sessions/{id}
```

Parse only `id`, `status`, `payment_status`, `amount_total`, `currency`, `client_reference_id` and `metadata.product`. Never log the raw response, which holds `customer_details`.

**No webhooks.** Collect asks Stripe for the session directly. So the CVM needs no inbound connection from Stripe, no webhook secret, no public certificate and no event handling, and the storefront needs no record of which orders are paid.

A restricted API key is used in place of the full secret key, and the exact permissions need finding out in the sandbox. Whether Apple Pay or Google Pay appear on Checkout, whether a card-only setting removes them, and whether name or postal code are collected, are unknown until the sandbox is tried once. [unknown]

**Why a hand-written client.** It's two calls. `async-stripe` is in a long release-candidate phase and returns `BadParse` when a payload doesn't fit its strict types. In that case the money is taken but fulfilment stops until a crate update.

## 4. Collect

`POST /v1/orders/{id}/collect {order_secret, issue_request}` checks, in order:

1. Retrieve the session from Stripe. If Stripe has no such session, return `404`.
2. `SHA-256(order_secret)` equals the session's `client_reference_id`, compared in constant time. Otherwise `401`.
3. `payment_status` is `paid`. Otherwise `402 unpaid`.
4. `metadata.product` names a product in `release.json`, and `amount_total` and `currency` match its `usd_cents` and USD. Otherwise `409 amount_mismatch`.
5. Call `issue.sock` with `order_ref = SHA-256(order_id)`, that product's credits and the wallet's `issue_request`. Return `{issue_response, credits}`.

The gate's `400`, `409 order_issued` and `503` responses pass through to the wallet. If the gate is Locked, Draining or down, collect returns `503 locked`, and a paid order stays collectable indefinitely ([limit 22](01-system-and-data-flow.md#accepted-limits)).

**Retries are exact.** The storefront keeps nothing between calls, so a retry repeats every check and calls the gate again. The gate keys issuance by `order_ref` and stores the request hash and response. The same request gets the same response, and a different request for the same order gets `409 order_issued` ([05 section 5](05-component-gate.md#issuing)). So each paid order is credited once, however often collect runs, and the order can't be redirected to another wallet's request.

Each collect costs one Stripe read, and a wallet waiting for payment polls. How fast wallets can poll before Stripe's sandbox rate limit matters is unknown. [unknown]

## 5. Operator configuration

The storefront starts `unconfigured`, holding no Stripe key. `xtask unlock` attests it over clearnet with role `storefront`, then sends `POST /v1/operator/configure {stripe_key}` with the operator token as bearer. The storefront compares `SHA-256(token)` with `operator_token_sha256` from `release.json`, in constant time, and returns `401` on a mismatch. On success it holds the Stripe key in memory only, in a `secrecy` type, and becomes `ready`.

`release.json` pins the token's hash, so nobody else can configure a fresh storefront with their own Stripe key and take payment for credit the gate issues. The token is separate from K, so the storefront never holds anything that opens the money store. [inferred]

Every restart of the storefront, or of the instance, returns it to `unconfigured`, and the operator configures it again after the gate unlock ([07 section 8](07-enclave-image-and-deployment.md#8-first-deployment)).

## 6. No database

The storefront stores nothing. Stripe holds the order, and the gate holds the issuance record. The order ID is Stripe's session ID, the reference that proves ownership is in Stripe's session, and the record that makes a retry exact is the gate's `issued` row.

That removes a whole class of state from the CVM: no schema, no retention rule, no purge, nothing to export on an update and no second encrypted file. The money store under K stays the only durable state. A crash loses only requests in flight, which the wallet retries with the same bytes. [inferred]

## 7. Payment provider and fake Stripe

`stripe.rs` implements `PaymentProvider` ([03 section 5.7](03-code-architecture.md#57-payment-provider)) over `reqwest`, with a configurable base URL: `https://api.stripe.com`, or a local `fake-stripe`.

`fake-stripe` serves the same two endpoints with the same shapes, in memory. It checks the bearer key is non-empty, returns `url = http://<its addr>/pay/<id>`, a page whose Pay button marks the session paid, and has `POST /test/pay/{id}` to mark a session paid from a test. So tier 1 tests run the storefront's real HTTP client against a local server, and only the sandbox tier needs a Stripe key.

## 8. Logging

- Raw Stripe responses are never logged, and `customer_details` is never parsed.
- Order secrets, issue requests and issue responses aren't logged.
- The Stripe key and the operator token use `secrecy` types, so they can't reach a log through `Debug`.
- There's no access log. The rate limiter holds IP addresses in memory only.

These rules are in measured code, but they don't hide what others hold. The host sees the clearnet connections. The operator holds the Stripe account, and Stripe returns exact times, email and other buyer details to the account holder. `client_reference_id` also stays in Stripe's records, so the join from a purchase to its order hash exists for whoever holds the account ([ARCHITECTURE section 7](../../ARCHITECTURE.md#7-what-each-party-can-see)).

## 9. Failure handling

| Failure | Behaviour |
|---|---|
| Collect before payment | `402 unpaid`. The wallet polls again. |
| Payment succeeded, wallet offline | Nothing expires here. The order stays collectable. |
| Collect response lost | Retry with the same request. The gate returns its stored response. |
| Storefront crash mid-collect | Nothing was stored here. A retry repeats the checks, and the gate either issues or returns its stored response. |
| Different issue request for a paid order | `409 order_issued`. The order can't be redirected. |
| Wrong `order_secret` | `401` |
| Unknown order ID | `404` |
| Product or amount doesn't match | `409 amount_mismatch`. Nothing is issued. |
| Gate Locked, Draining or down | `503 locked`. The paid order stays collectable. |
| Gate low on disk | `503 low_disk`, passed through |
| Not yet configured after a restart | `503 unconfigured` for orders and collect |
| Stripe unreachable or failing | Orders and collect fail without issuing. The wallet keeps its state and retries. |
| Abandoned Checkout | Nothing to clean up here. The session stays unpaid at Stripe. |
| Card dispute | Out of scope, by your decision. Stripe handles it, and credit isn't recalled. |

## 10. Invariants

1. Nothing is issued unless Stripe reports the session paid, for a listed product at its listed amount, and the caller holds the order secret.
2. Each paid order is credited once. The gate keys issuance by `order_ref`, and retries return the same response.
3. No field sent to Stripe relates to the issue request or anything on the use plane. Stripe sees a hash of the order secret and a product ID.
4. Raw Stripe responses are never logged, and `customer_details` isn't parsed.
5. The Stripe key exists only in memory, in `stripe.rs`'s client. It's never written to disk.
6. The storefront's only route to the gate is `issue.sock`. It can't reach `gate.sock`, the money store or the engine socket.
7. The storefront writes no durable state.

**If the storefront is compromised**, it can mint. It can call `issue.sock` with any listed product, without a payment. Comparing Stripe's paid sessions with `issued` rows, using K, detects this after the fact. It can't read prompts without also escaping its user account inside the VM. [inferred] [Limit 20](01-system-and-data-flow.md#accepted-limits) accepts this, and [limit 21](01-system-and-data-flow.md#accepted-limits) the extra attack surface of a clearnet-facing server inside the CVM.

## 11. Running it

**Where.** Inside the CVM, as `storefront.service`, user `storefront`, ordered after the gate. It has `ProtectSystem=strict`, membership in the group that owns `issue.sock`, and no access to `gate.sock`, the store or the engine socket. Units and runtime directories are in [07 section 4](07-enclave-image-and-deployment.md#4-boot-order).

**Network.** It listens on TCP 8443 with inner TLS, its key generated in memory at each start and bound into an SNP report with role `storefront`. The runtime firewall accepts inbound TCP 8443, and lets the `storefront` user out to ports 53 and 443 only, for DNS and Stripe's API ([07 section 5](07-enclave-image-and-deployment.md#5-firewall)). No reverse proxy sits in front of it.

**Start-up.** It comes up `unconfigured`. `xtask unlock` unlocks the gate first, then configures the storefront. Issuance depends on the gate being Ready, so until it is, collect returns `503 locked`.

**Addressing.** Wallets reach it at an address compiled in beside the onion address, because a manifest can't be allowed to choose where a wallet connects. Whether an inbound port beyond SSH reaches a Verda CC instance, and whether its public IP survives Shutdown and Start, are M0 checks. If the IP changes, wallets need a DNS name pinned instead, which still needs no certificate. [unknown] This is [limit 23](01-system-and-data-flow.md#accepted-limits).

**Development.** The prototype runs it on `127.0.0.1:8443` against `fake-stripe`, and against the Stripe sandbox once a test key is available ([`prototype/SPEC.md` section 6](../../prototype/SPEC.md#6-storefront)).
