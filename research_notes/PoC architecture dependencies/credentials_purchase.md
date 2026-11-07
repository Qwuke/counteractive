# Credentials and purchase plane: dependency survey

| Component | Decision | Version | Licence | What it saves us | Main risk |
|---|---|---|---|---|---|
| `blind-rsa-signatures` (voucher crypto) | ADOPT in wallet and gate; WRAP signing in storefront | 0.18.0, 2026-09-25 | MIT | RFC 9474 blind, sign, finalize and verify with test vectors: 1 to 2 weeks (INFERRED) | No audit found; one maintainer; depends on a release candidate of `rsa` |
| `rsa` (under the above) | ADOPT, exact pin; AVOID as a direct API | `=0.10.0-rc.21`, 2026-11-05 | MIT OR Apache-2.0 | The private-key operation with blinding and fault check | Hardening landed 2026-11-01 to 11-05; RUSTSEC-2023-0071 still lists no patched version |
| `openssl` (reserve signer) | WRAP, only if review rejects `rsa` | 0.10.82, 2026-11-01 | Apache-2.0 | About 20 lines swap out the signing call (INFERRED) | Adds a C dependency to the storefront |
| RFC 9578 type 0x0002 byte layout | BUILD-OURSELVES, about 100 lines (INFERRED) | RFC 9578 | n/a | Format design, key id rule and test vectors: about 1 day (INFERRED) | The 32-byte challenge digest does no work for us |
| RFC 9577 `PrivateToken` HTTP scheme | AVOID | RFC 9577 | n/a | Nothing; redemption is a plain POST | None |
| `voprf` | AVOID | 0.5.0, 2024-03-07; 0.6.0-pre.2, 2026-11-05 | Manifest says MIT; repo signals conflict | Nothing usable | Gate would need the storefront's secret key |
| `privacypass` | AVOID at runtime; optional dev-dependency for an interop test | 0.2.0-pre.4, 2026-10-27; stable 0.1.0, 2022-07-12 | MIT | About 100 lines, at the cost of 30 extra crates | Pre-release, unaudited by its own statement, pulls a forked VOPRF |
| `ohttp` | AVOID | 0.8.0, 2026-07-15 | MIT OR Apache-2.0 | Nothing; Tor already separates IP from request | Extra layer on the use path with no gain |
| Cashu / CDK | AVOID, including as denominations fallback | 0.18.3, 2026-10-29 | Apache-2.0 OR MIT | A denominated ledger; net 2 to 4 weeks of swap work (INFERRED) | Gate needs the mint secret or breaks blindness; alpha; 343 crates |
| OpenPCC | AVOID as code; read as reference | Last commit 2026-01-08 | Apache-2.0 | No code (Go); value grid and refund ideas | TPM-based attestation; no Rust |
| Stripe Checkout, hosted page | ADOPT | API `2026-10-28.endive` | Hosted service | Card UI, 3DS and SAQ A scope: weeks plus PCI work (INFERRED) | Stripe always holds card identity and email |
| Stripe client, hand-written | BUILD-OURSELVES, about 150 lines, 1 to 2 days (INFERRED) | reqwest 0.13.5 (2026-09-08), hmac 0.13.0, sha2 0.11.1 | All MIT OR Apache-2.0 | Avoids tracking an RC that strictly parses generated types | We own the webhook signature code, and it handles money |
| `async-stripe` | AVOID until 1.0 final | 0.41.0, 2025-04-08; 1.0.0-rc.10, 2026-10-26 | MIT OR Apache-2.0 | About half a day against the raw client (INFERRED) | Stable is on the 2023-10-16 API; RC has no thin events and webhook fixes still landing |
| Stripe CLI | ADOPT for development | v1.55.8, 2026-11-03 | Apache-2.0 | Webhooks to localhost with no public endpoint: about 1 day (INFERRED) | None for development |
| SQLite through `rusqlite` | ADOPT | 0.41.1, 2026-10-28, `bundled` | MIT | Running and backing up a database server | Synchronous API; single instance only |
| `sqlx` and Postgres | AVOID for the PoC | sqlx 0.9.0, 2026-05-21 | MIT OR Apache-2.0 | Compile-time query checks | 162 crates against 22; a service to run for no PoC gain |
| `axum` | ADOPT | 0.8.9, 2026-04-14 | MIT | HTTP routing, raw-body extraction, body limits | Breaking changes are queued on main |

**How to read this note.** Every claim is marked VERIFIED (with where I read it), INFERRED (my reasoning) or UNKNOWN. Versions, dates, licences, stars and last-commit dates come from the crates.io and GitHub APIs, read on 2026-11-06, unless stated otherwise. Nothing was built or run. Crate counts come from `cargo generate-lockfile` on manifests I wrote, and they are upper bounds because a lock file lists all platforms. Source copies are under `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/dep_survey/`.

## 1. `blind-rsa-signatures`: ADOPT, with signing wrapped

**Facts**
- Maintainer is Frank Denis (`jedisct1`), a personal account. 35 stars, 0 open issues, 17 total and all closed, last commit 2026-09-25, 9.53M downloads. VERIFIED (crates.io, GitHub API).
- Licence is MIT. VERIFIED (LICENSE: "Copyright (c) 2021-2026 Frank Denis").
- Review status: the README makes no audit statement and a web search found no audit. Report this as "none found". VERIFIED for the README; absence elsewhere is INFERRED.
- Tests include `tests/vectors.rs` and `tests/test_vectors_rfc9474.json`, and CI runs `cargo test --release`. VERIFIED (repo).
- Nine crates depend on it, including geph5-client and mizaru2. VERIFIED (crates.io).

**API, all VERIFIED in `src/lib.rs`**
- `KeyPair::generate(rng, bits)` at L980. Key sizes are limited to 2048 to 4096 bits by `check_rsa_parameters` at L421.
- `PublicKey::blind(rng, msg)` at L734, returning `BlindingResult { blind_message, secret, msg_randomizer }`.
- `SecretKey::blind_sign(blind_msg)` at L925, which calls `rsa_decrypt_and_check` with an RNG at L946. It rejects wrong-length input and values at or above n.
- `PublicKey::finalize(&blind_sig, &result, msg)` at L772, which also verifies.
- `PublicKey::verify(&sig, msg_randomizer, msg)` at L796. Verification needs only the public key.
- `to_spki` at L683 and `from_spki` at L705. The SPKI uses the RSASSA-PSS algorithm identifier at L460, which is what RFC 9578 §6.5 hashes for the key id.
- Variant aliases include `KeyPairSha384PSSRandomized` and `KeyPairSha384PSSDeterministic`.

**Variant: RSABSSA-SHA384-PSS-Deterministic, RSA-2048**
- RFC 9474 §5 recommends the Randomized variants by default, but says "applications that provide high-entropy input messages can safely use named variants without randomized message preparation". VERIFIED (RFC 9474).
- RFC 9578 §6 supports only the Deterministic variants and puts a `random(32)` nonce in the signed input. VERIFIED (RFC 9578). So the high-entropy condition holds. INFERRED.
- "Deterministic" names only the message preparation. PSS still uses a 48-byte salt (RFC 9578 §6.4). VERIFIED.
- This also removes the 32-byte randomizer from the voucher. INFERRED.
- The key must not be reused for another protocol or another encoding (RFC 9474 §6.2). VERIFIED.

**Limits**
- `Secret` is `pub struct Secret(pub Vec<u8>)` with derived `Debug` (L336-337), and "zeroize" appears nowhere in `src/lib.rs` or the manifest. VERIFIED (grep). The wallet must never log a `BlindingResult`, because that value links the voucher to the order. INFERRED.
- `privacypass` documents that `KeyPair` lacks `Zeroize` ("upstream blind-rsa-signatures gap"). VERIFIED (privacypass doc comment).
- Client `blind` in `src/brsa.rs` uses `random_mod_vartime` and `invert_mod`. VERIFIED. This runs locally in the wallet, so a remote observer has no timing signal. INFERRED.
- The `pbrsa` module offers partially blind RSA with metadata and needs safe primes. VERIFIED. It is not in RFC 9474 (INFERRED), so leave it out of the PoC.
- Whether 0.18.0 compiles against `rsa` rc.21 is UNKNOWN. A fresh lock resolves to rc.21 (VERIFIED, my resolver run), and the call at L946 matches rc.21's signature on reading (INFERRED).
- Size: 55 crates in the lock. VERIFIED (resolver run).

**Alternatives, all dropped**
- `blindsign` 0.0.6 (2018) and `rsa-fdh` 0.5.0 (2020, FDH, not RFC 9474). VERIFIED (crates.io).
- `purecrypto` 0.10.4: the docs.rs item list shows only Pedersen blinding, with no blind RSA. VERIFIED (docs.rs `all.html`).

URLs: https://crates.io/crates/blind-rsa-signatures, https://github.com/jedisct1/rust-blind-rsa-signatures, https://www.rfc-editor.org/rfc/rfc9474.txt

## 2. `rsa` and bignum timing: pin exactly, do not use directly

**Why `rsa` alone is not an alternative**
- It has no blind signature protocol. The README lists OAEP, PKCS1v15 and PSS. A grep for "blind signature" and "chaum" finds nothing. VERIFIED (rc.21 source).
- `rsa::pss::BlindedSigningKey` is misleadingly named: in rc.21 it is an alias of `SigningKey` (`src/pss.rs:38`). VERIFIED. It signs a message and has no blind protocol. INFERRED.
- Only the raw `rsa::hazmat::rsa_decrypt_and_check` is useful here. VERIFIED. Building RFC 9474 on it means re-writing what `blind-rsa-signatures` already has: BUILD-OURSELVES rejected.

**Timing state, in date order**
- RUSTSEC-2023-0071 (Marvin): `patched = []`, "Still affected as of 2026-09-12: rsa 0.9.10 (latest stable) and rsa 0.10.0-rc.18". Workaround: avoid `rsa` where attackers can observe timing, "for example over the network". VERIFIED (advisory-db main).
- RUSTSEC-2026-0387 (crypto-bigint, 2026-10-23, patched >= 0.7.6): secret-dependent branches in subtraction, division and Montgomery multiplication, depending on compiler and CPU. VERIFIED (advisory-db). crypto-bigint 0.7.8 is yanked. VERIFIED (crates.io).
- RustCrypto/RSA #711 and #626 are closed. PRs #739, #751, #752, #770, #771 and #773 merged 2026-11-01 to 11-05. VERIFIED (GitHub).
- rc.21 `src/algorithms/rsa.rs`: "Performs base, exponent, and CRT modulus blinding if an `Rng` is passed, using fresh factors for each private operation". `src/algorithms/blinding.rs` is absent in rc.18. VERIFIED (source of both).
- rc.21 requires crypto-bigint 0.7.10 and Rust 1.87. VERIFIED (`Cargo.toml.orig`).
- rc.21's README still says the crate "has been previously audited" but "has diverged significantly since then, and several security issues have been discovered which we are still working to address". VERIFIED.

**Assessment**
- RFC 9474 §7.1: "BlindSign is functionally a remote procedure call for applying the RSA private key operation. As such, side-channel resistance is paramount". VERIFIED. This is the riskiest code in the storefront.
- Marvin's padding-oracle part does not apply, because blind signing has no depadding. Leakage of the private exponent still would. INFERRED.
- `blind_sign` always passes an RNG, so all three blindings run on rc.21. VERIFIED (L925-946 and rc.21 source).
- With one signing per paid order, each timing sample costs a card payment. INFERRED; depends on impact item 3.
- Decision: pin `rsa = "=0.10.0-rc.21"` and commit the lock. `blind-rsa-signatures` declares `0.10.0-rc.18`, so a stale lock can hold the unhardened code. VERIFIED (its `Cargo.toml.orig`). rc.21 is one day old today, so check for a newer RC before freezing the pin.
- `cargo audit` should keep flagging RUSTSEC-2023-0071 because `patched = []` (INFERRED). Record a dated exception.

**Reserve signer**
- `openssl` 0.10.82 has `Rsa::private_decrypt` (`src/rsa.rs:126`), `Padding::NONE` (:48), `public_encrypt` (:329) for the fault check, and 450M downloads. VERIFIED (source, crates.io).
- That OpenSSL blinds RSA private operations by default is INFERRED from general knowledge, not read in this survey. The crate has 10 RustSec entries, not read one by one. VERIFIED that they exist (advisory-db).
- Keep signing behind one function so that this swap stays small. The boring choice for real money is this path or a paid review of the RC path.

URLs: https://github.com/RustCrypto/RSA, https://rustsec.org/advisories/RUSTSEC-2023-0071.html, https://rustsec.org/advisories/RUSTSEC-2026-0387.html

## 3. `voprf`: AVOID

**Can the gate verify without the storefront's secret key? No.**
- RFC 9497: the output is F(skS, input), and the proof convinces only the client. VERIFIED.
- RFC 9578 §5.4: "Verifying a token requires creating a … VOPRF context using the Issuer Private Key". IANA lists type 0x0001 as "Publicly Verifiable: N" and 0x0002 as "Y". VERIFIED.
- The crate shows the same shape: `VoprfServer::new_with_key` (`src/voprf.rs:229`), `evaluate(&self, input)` (:380). VERIFIED.
- So the gate would either hold the key, and a CVM compromise could mint paid vouchers, or call the storefront, and the planes are no longer joined only by the voucher. INFERRED.

**Other facts**
- 0.5.0 is from 2024-03-07 and pulls crypto-bigint 0.5.5, which the 0.6.0-pre.2 changelog says is affected by RUSTSEC-2026-0387. VERIFIED. 0.6.0-pre.2 needs Rust 1.88. VERIFIED.
- Licence signals conflict: `Cargo.toml` says MIT, the README claims dual, both licence files are present, GitHub reports Apache-2.0. VERIFIED.
- The repo redirected from `facebook/voprf` to `meta/voprf`; 88 stars, last commit 2026-11-05. VERIFIED. No audit statement in the README. VERIFIED.
- Its advantage is size and speed: a 96-byte response and 159 us to issue against 256 bytes and 1,860 us for blind RSA-2048 (ristretto255, Go CIRCL, `spike_act.md` L219-221). VERIFIED there. That is irrelevant at one signature per purchase. INFERRED.

URLs: https://github.com/facebook/voprf, https://www.rfc-editor.org/rfc/rfc9497.txt

## 4. `privacypass`: AVOID at runtime

- Only a 2022 stable exists (0.1.0); the current code is 0.2.0-pre.4, with PR #28 "Release 0.2" open since 2024-03. 0 reverse dependencies, 71 stars, last commit 2026-10-27. VERIFIED.
- README: "**Warning:** This library has not been independently audited. It should not be used in production without a professional security review." VERIFIED.
- It forces `voprf-ng =0.6.0` (a fork), p384, `tls_codec` and `nom`, even for public tokens. 85 crates against 55. VERIFIED (manifest, resolver run). Issue #69 asks to make blind RSA an optional feature. VERIFIED.
- It is a thin layer on the same crate: `public_tokens::PublicKey = blind_rsa_signatures::PublicKey<Sha384, PSS, Deterministic>`. VERIFIED.
- Useful as a reference: `OriginServer::redeem_token` reserves the nonce, verifies with a `None` randomizer, then commits or releases. VERIFIED.
- Optional: use it as a dev-dependency to confirm our bytes interoperate. Work saved at runtime is about 100 lines, not worth the fork. INFERRED.

URL: https://github.com/raphaelrobert/privacypass

## 5. RFC 9578 and RFC 9577 formats: copy the bytes, skip the HTTP flow

**Layout to copy (RFC 9578 §6, VERIFIED)**
- `nonce = random(32)`
- `token_input = 0x0002 || nonce || challenge_digest || token_key_id`, which is 98 bytes
- `TokenRequest { u16 type, u8 truncated_key_id, blinded_msg[256] }`, which is 259 bytes
- `TokenResponse { blind_sig[256] }`
- `Token { u16, nonce[32], challenge_digest[32], token_key_id[32], authenticator[256] }`, which is 354 bytes
- `token_key_id = SHA-256(SPKI with id-RSASSA-PSS)` (§6.5)
- Verification is RSASSA-PSS, SHA-384, salt length 48 (§6.4), and tokens are "publicly verifiable by anyone with the Issuer Public Key" (§6 introduction)
- Appendix A.2 has test vectors

**Value for vouchers**
- We get a fixed binary format, a type field for later change, a key id, independent test vectors and an analysed pattern, for about 100 lines. INFERRED.
- RFC 9576 §4.2, Joint Attester and Issuer, matches the storefront, with the gate as origin. The mapping is INFERRED.
- A custom `{key_id, nonce, sig}` struct would be about 30 lines but has no vectors. The RFC layout is worth the 70 lines. INFERRED.

**What to skip**
- The `PrivateToken` header flow (RFC 9577). Vouchers are bought ahead of use and redeemed by a POST to the gate, so no challenge round trip is needed. INFERRED.
- Use one constant `TokenChallenge` from the signed manifest, with empty `redemption_context` and empty `origin_info`. Tokens may be cached for later use if the fields match (RFC 9577 §2.1.4). VERIFIED for the caching rule. Building the challenge locally instead of receiving it is our own choice, and no RFC conformance claim should be made. INFERRED.
- RFC 9577 leaves double-spend prevention to the origin: "Origins SHOULD implement some form of double-spend prevention". VERIFIED. Key it on the nonce, as `privacypass` does.
- Type 0x0002 has no metadata field ("Public Metadata: N"). VERIFIED. So value comes from the key: one key per denomination. INFERRED.

URLs: https://www.rfc-editor.org/rfc/rfc9578.txt, https://www.rfc-editor.org/rfc/rfc9577.txt, https://www.rfc-editor.org/rfc/rfc9576.txt

## 6. `ohttp`: AVOID

- 0.8.0, 2026-07-15, MIT OR Apache-2.0, publisher `martinthomson`, 66 stars, last commit 2026-09-08. Backends are `rust-hpke` (default) or NSS. VERIFIED (crates.io, `Cargo.toml`, README).
- OHTTP's purpose of hiding client IP from the server overlaps with Tor (`existing_systems.md` L452). VERIFIED there. It saves no work on the use plane. On the purchase plane Stripe sees the browser anyway, so it adds nothing there either. INFERRED.

URL: https://github.com/martinthomson/ohttp

## 7. Cashu and CDK: AVOID, including as the denominations fallback

**State**
- `cdk` and `cashu` 0.18.3, 2026-10-29; maintainer thesimplekid; 233 stars, 177 open, last commit 2026-11-06. Licence is Apache-2.0 OR MIT (crates.io and LICENSE.md agree; GitHub reports NOASSERTION). VERIFIED.
- README: "**ALPHA** … the api will change" and "Always use amounts you don't mind losing". VERIFIED. No audit found: UNKNOWN.

**Why it does not fit**
- `cashu::dhke::verify_message` takes the mint secret key (`src/dhke.rs:173`); NUT-00 verification is `k*hash_to_curve(x) == C`. VERIFIED. So the gate holds the mint key, or the mint must be online to the gate. INFERRED.
- NUT-12 DLEQ allows verification with public keys, but the token must then carry `r`, `e` and `s`, and "The mint can associate the `BlindSignature` with the `Proof` given `e` and `s`, or using the blinding factor `r`" (NUT-12 L107). VERIFIED. Since one operator runs both planes, that verifier gives up unlinkability. INFERRED.
- Blind RSA gives public verification with no such cost. `red_team.md` L237 and L241 already make this point for denominations ("RSA only; VOPRF does not"), and Cashu falls on the same side as VOPRF. INFERRED.
- There is no metered settlement in CDK. `routstr` built one in Python under GPL-3.0 (prior note, not re-read).
- Tor support: `crates/cdk-http-client/Cargo.toml` feature `tor` uses `arti-client` 0.19.0 from 2024-06-05; current is 0.48.0, 2026-11-04. VERIFIED. The Tor spike ran Arti 2.8.0 (`spike_tor.md` L9). VERIFIED there.
- Size is 343 crates with `cdk` mint and wallet plus `cdk-sqlite`, including `bitcoin` 0.32.102 and `secp256k1` 0.29.1. It pulls `libsqlite3-sys` 0.28.0 against rusqlite's 0.39.1. VERIFIED (resolver run). Linking both in one binary would fail (INFERRED, `links` rule).

**What does work (VERIFIED in source)**
- `CurrencyUnit::Usd` exists (`cashu/src/nuts/nut00/mod.rs:606`). This corrects "not USD rail" in the earlier notes, at least for the unit.
- The `MintPayment` trait (`crates/cdk-common/src/payment.rs:443`) could wrap Stripe, with `PaymentIdentifier::CustomId` for the session id.
- NUT-08 gives a useful change pattern, worth copying as an idea.

**Swap cost:** 2 to 4 weeks (INFERRED), covering a `MintPayment` Stripe adapter, the unit and keyset setup, metered change, a replacement for EC-ACT redemption, and the key-placement problem above, which has no clean fix.

**Better fallback:** per-denomination blind RSA keys, with the gate verifying by public key and using EC-ACT only for the session. See impact item 7.

URLs: https://github.com/cashubtc/cdk, https://github.com/cashubtc/nuts/blob/main/12.md

## 8. OpenPCC: AVOID as code

- Go only: `go.mod` has go 1.25.4, circl v1.6.1, go-tpm, and a `go-sev-guest` replaced by a fork. Apache-2.0, 957 stars, last commit 2026-01-08. VERIFIED (repo, GitHub API). The compute node is under FSL, per `existing_systems.md` L160-188 and `fact_check.md` §18. VERIFIED there.
- Credit code: `anonpay/blind_signing.go` uses `partiallyblindrsa.NewSigner` from CIRCL. The `anonpay/` tree has `issuer.go`, `exchanger.go`, `credit.go`, `currency`, `wallet`. VERIFIED. Prior notes found only in-memory banks and no audit. VERIFIED there.
- Attestation is under `attestation/` plus top-level `tpm/`, built on a vTPM assumption, not our direct SEV-SNP path. VERIFIED (tree, prior note).
- It saves no code, since we would have to port it. Read `anonpay/` for its value grid and refund exchange as design input: a few hours saved (INFERRED).
- Comparison: same family as our design, with partially blind RSA where we use per-denomination keys, and an exchange where we use EC-ACT refunds. It has no Tor. VERIFIED for no Tor (prior note). The comparison is INFERRED.

URL: https://github.com/openpcc/openpcc

## 9. Stripe Checkout: ADOPT the hosted page

All from docs.stripe.com, read 2026-11-06, with copies in `cred_meta/stripe_docs/`. No account or sign-in used. Current API is `2026-10-28.endive`. VERIFIED (`stripe/openapi` master spec `info.version`).

**Creating a session** (https://docs.stripe.com/api/checkout/sessions/create). VERIFIED.
- `POST /v1/checkout/sessions`.
- `client_reference_id` holds up to 200 characters. `metadata` is a free map; leave it empty.
- `expires_at` is 30 minutes to 24 hours, default 24 hours.
- `allowed_payment_method_types` filters the offered methods. This page does not document `payment_method_types` as a request parameter; it appears only in the example response and in the filter's description. Use the filter.

**Events** (https://docs.stripe.com/checkout/fulfillment). VERIFIED.
- `checkout.session.completed` fires when Checkout completes. `payment_status` can be `paid`, `unpaid` or `no_payment_required`, so it must be checked.
- Delayed methods such as bank debits later send `checkout.session.async_payment_succeeded` or `async_payment_failed`.
- Fulfilment "might be called multiple times, possibly concurrently, for the same Checkout Session". Checkout waits up to 10 seconds for the handler before redirecting.

**Thin events** (https://docs.stripe.com/events/how-events-work). VERIFIED.
- "For new integrations, use thin events." A thin notification has `id`, `type`, `created` and `related_object`, with no resource state, and is unversioned.
- `v1.checkout.session.completed` and `v1.checkout.session.async_payment_succeeded` are in the catalog (https://docs.stripe.com/api/v2/core/events/event-types).
- They use the same `Stripe-Signature` header and three inputs as snapshot events (`webhooks_signature.md` L139). The same HMAC scheme is INFERRED; confirm in the sandbox.
- A thin destination is registered separately from snapshot ones (`webhooks.md` L83). `event_payload = "thin"`.

**Retries and idempotency** (https://docs.stripe.com/webhooks, https://docs.stripe.com/api/idempotent_requests). VERIFIED.
- Live retries run up to three days with exponential back-off. Sandbox retries run three times over a few hours.
- Manual resend: Dashboard 15 days, `stripe events resend` 30 days. Order is not guaranteed.
- Duplicates: "In some cases, two separate Event objects are generated and sent", so identify by `data.object` id plus type. Keying only on event id is wrong.
- `Idempotency-Key` on POST, up to 255 characters, no sensitive data, prunable after 24 hours, parameters compared.
- Signature: split `t=` and `v1=`; HMAC-SHA256 of `t + "." + raw_body` with the endpoint secret; constant-time compare against each `v1`, because several appear during a secret roll of up to 24 hours; ignore other schemes; libraries default to a 5-minute tolerance.

**Local and sandbox testing** (https://docs.stripe.com/cli/listen, https://docs.stripe.com/sandboxes). VERIFIED.
- `stripe listen --forward-to <url>`, `--events`, `--print-secret`; thin events with `--forward-thin-to <url> --thin-events "*"` (`webhooks.md` L135).
- `stripe trigger` supports `checkout.session.completed` and both async events (https://docs.stripe.com/cli/trigger).
- Sandboxes: payments "aren't processed by card networks", up to five with isolated settings. Use these rather than legacy test mode. Test payments send no automatic receipts.

**Who sees card data.** VERIFIED (https://stripe.com/guides/pci-compliance, https://docs.stripe.com/security/guide).
- "Checkout or Elements | SAQ A"; card inputs are served from Stripe's domain, "so your customers' card information never touches your servers".
- We still attest annually. Card type, last4 and expiry are out of PCI scope, but we should not read them.

**Does Checkout or Radar force personal data?**
- Email: yes, in effect. "If not provided, customers will be asked to enter their email address" (`customer_email`). No documented off switch found. VERIFIED for the sentence; absence is INFERRED. A placeholder `customer_email` might remove the field (INFERRED). Whether Stripe permits that is UNKNOWN, and Radar's model uses email, so do not do it.
- Billing address: `billing_address_collection` defaults to `auto`: "Checkout will only collect the billing address when necessary". VERIFIED. What "necessary" means for card postal code, and whether a cardholder name is always asked, are UNKNOWN from the docs. Measure in a sandbox.
- Radar: "All transactions are screened using Radar's default rules" (https://docs.stripe.com/radar/rules). The postal rule "doesn't block payments where the customer doesn't provide the postal code". Checkout automatically collects device information and IP (https://docs.stripe.com/radar/integration). VERIFIED. Nothing read says Radar adds required fields. INFERRED.
- Fraud signals are kept "as long as it's useful" (https://docs.stripe.com/disputes/prevention/advanced-fraud-detection), and 3DS2 device information goes to the issuer. VERIFIED.
- Guest grouping: "Stripe automatically groups guest customers in the Dashboard based on them having used the same card, email, or phone" (https://docs.stripe.com/payments/checkout/guest-customers). VERIFIED. So repeat purchases are linked at Stripe. This is outside our voucher's protection. INFERRED.

**Minimum personal data configuration**
- Hosted page, `mode=payment`, `allowed_payment_method_types=[card]`, random `client_reference_id`, empty `metadata`.
- No Customer object passed, no `customer_email`, `billing_address_collection=auto`, no name, phone, shipping, tax, consent or custom fields.
- Successful-payment receipts off in Dashboard settings; no `receipt_email` (https://docs.stripe.com/payments/checkout/receipts). VERIFIED that the control exists.
- Link off in Dashboard. A card-only list omits it (https://docs.stripe.com/payments/link/checkout-link). INFERRED.
- Whether Apple Pay and Google Pay still show under a card-only filter is UNKNOWN. Check in the sandbox.
- Restricted key (`rk_live_`, https://docs.stripe.com/keys/restricted-api-keys) rather than the secret key. VERIFIED that these exist. The exact permission names are UNKNOWN.

## 10. Stripe client: BUILD-OURSELVES, AVOID `async-stripe` for now

**`async-stripe`**
- Maintainer `arlyon`, 760 stars, 18 open, last commit 2026-11-02, 5.87M downloads. MIT OR Apache-2.0. VERIFIED. Maintained, but in a long RC phase: rc.8 2026-08-05, rc.9 09-11, rc.10 10-26. VERIFIED.
- 0.41.0 targets API `2023-10-16` (`src/resources/generated/version.rs`), with hyper 0.14 and rustls 0.21. `Webhook::construct_event` is at `src/resources/webhook_events.rs:508`. VERIFIED.
- rc.10 targets `2026-09-30.endive` (`async-stripe-shared`), one release behind current. PRs #994 and #990 are open to regenerate and release. VERIFIED.
- rc.10 has what we need on paper: `CreateCheckoutSession` with `client_reference_id` and `metadata` builders (`async-stripe-checkout`, `src/checkout_session/requests.rs`), and `Webhook::construct_event` with multi-`v1` handling and 300-second tolerance (`async-stripe-webhook/src/webhook.rs` L210-302). VERIFIED.
- The crate named `stripe-webhook` does not exist on crates.io. The real name is `async-stripe-webhook`. VERIFIED.

**API gap and risk**
- `parse_payload` returns `BadParse` on a strict parse failure. VERIFIED. If Stripe adds a shape the generated types reject, the money is taken and fulfilment stops until a crate update. INFERRED.
- Open issue #931, "Unknown value '2026-05-27.dahlia' for enum 'ApiVersion'", has been open since 2026-06. VERIFIED. It shows this kind of mismatch happens.
- Webhook churn: rc.10 has "accept any matching v1 signature during rotation (#985)" and "reject incomplete objects instead of silently dropping list items (#978)"; rc.9 has "omit raw payload from tracing"; a later commit "bound debug deserialization stack usage (#991)" is unreleased. VERIFIED (changelog, commits).
- No thin event support: "thin" has no hit in `webhook.rs`, and "notification" hits in `generated/mod.rs` are unrelated events. VERIFIED by grep. So the crate cannot follow Stripe's recommended format.
- Full typed parsing brings `customer_details` into our process as named fields. INFERRED.

**Raw client, about 150 lines (INFERRED)**
- `create_session`: form POST with `Idempotency-Key` and `Stripe-Version`.
- `get_session`: GET, parsing only `id`, `status`, `payment_status` and `client_reference_id`.
- `verify_webhook`: the steps in section 9, using `Hmac::<Sha256>::new_from_slice` and `verify_slice`, the same calls `async-stripe-webhook` uses at `src/webhook.rs:291-295` with hmac 0.13 and sha2 0.11. VERIFIED.
- The `Stripe-Version` header overrides the account default. VERIFIED (https://docs.stripe.com/api/versioning).
- Stripe recommends official libraries but allows a custom verifier. VERIFIED (webhooks docs). There is no official Rust library. INFERRED.

**Which is less work and risk**
- Raw is 1 to 2 days and async-stripe saves about half a day of that. INFERRED.
- Raw is not smaller in the lock: 169 crates (reqwest 0.13.5 with rustls brings `aws-lc-sys`) against 123 for rc.10 with rustls. VERIFIED (resolver run). reqwest is already planned for the wallet (task description), so workspace growth is small. INFERRED.
- The decision rests on risk, not size. Three calls and one HMAC are auditable in an afternoon, and they do not break when Stripe changes an unrelated field. Writing the signature check ourselves is a real risk; test it with a fixed vector and an end-to-end `stripe listen` run. Revisit at async-stripe 1.0 final if we use more of the API.

URLs: https://github.com/arlyon/async-stripe, https://docs.stripe.com/webhooks

## 11. Storage: ADOPT SQLite through `rusqlite`

- `rusqlite` 0.41.1, 2026-10-28, MIT, maintainer gwenn, 4466 stars, last commit 2026-11-06. VERIFIED.
- `bundled` feature (`Cargo.toml.orig:53`); `TransactionBehavior::Immediate` and `transaction_with_behavior` (`src/transaction.rs:15`, `:440`). VERIFIED. `bundled` compiles SQLite from C, so the build needs a C compiler. INFERRED.
- 22 crates against 162 for `sqlx` with SQLite. VERIFIED (resolver run).
- Advisories: rusqlite has two, patched >= 0.23.0 and >= 0.26.2; sqlx 0.9.0 is past RUSTSEC-2024-0363 (patched >= 0.8.1). VERIFIED (advisory-db).
- `sqlx` moved to `transact-rs/sqlx`, 17631 stars, last push 2026-10-31. VERIFIED (GitHub API). Fine, but more than we need.
- The API is synchronous. Run it on Tokio's blocking pool (INFERRED). `tokio-rusqlite` 0.8.0, MIT, is a simpler wrapper if preferred. VERIFIED that it exists.
- `PRAGMA secure_delete` overwrites deleted content with zeros and is normally off by default. VERIFIED (https://www.sqlite.org/pragma.html). Turn it on, since orders are deleted after the retry window.
- Postgres has no PoC benefit with one storefront instance. Move if we need a second instance. INFERRED.
- Whether the gate's spent-nonce store can use the same library depends on persistent CVM storage, which is outside this note: UNKNOWN here.

URLs: https://github.com/rusqlite/rusqlite, https://github.com/transact-rs/sqlx

## 12. `axum`: ADOPT

- 0.8.9, 2026-04-14, MIT, maintainer Darksonn, `tokio-rs/axum`, 27718 stars, last commit 2026-11-06. No `crates/axum` folder in advisory-db. 51 crates. VERIFIED.
- `Bytes` extractor gives the raw body, which the webhook signature needs (`src/docs/extract.md:63-64`). `DefaultBodyLimit` is exported (`src/extract/mod.rs:22`), and the default is 2 MB (test comment, `src/routing/tests/mod.rs:672`). VERIFIED.
- `axum::serve` accepts anything implementing `Listener` (`src/serve/listener.rs:9`, `src/serve/mod.rs:97`). VERIFIED. Whether that fits the gate's onion streams belongs in the Tor survey.
- `CHANGELOG.md` on main has an Unreleased section with several breaking changes, so a major bump is coming. Its date is UNKNOWN. VERIFIED for the section.
- One of those changes makes `axum::serve` apply hyper's default `header_read_timeout`. VERIFIED. So on 0.8.9 we should set timeouts ourselves. INFERRED.

URL: https://github.com/tokio-rs/axum

## Design impact

Each item is a specific change to the design described in the task, or a confirmation where noted.

1. **Voucher format.** Make the voucher an RFC 9578 type 0x0002 `Token` of 354 bytes, signed with RSABSSA-SHA384-PSS-Deterministic under RSA-2048. Do not use PSS-Randomized and do not invent a format. The signed input is `token_input`, not a bare nonce.
2. **Keys.** Use one key per denomination. Ship each SPKI and `token_key_id` in the signed release manifest, so all wallets see the same keys. The gate holds public keys only and keeps a spent-nonce store for each key's life. Rotation bounds that store.
3. **Sign once per paid order.** Keep the rule in `purchase_wallet_abuse.md` L74: store the signed response for the retry window and return it on retry, without signing again. This caps private-key operations at one per payment, so a repeat request gives an attacker no new timing sample. Enforce it in a `BEGIN IMMEDIATE` transaction.
4. **Signer isolation.** Put signing behind one function in the storefront. Pin `rsa =0.10.0-rc.21`, commit the lock and record a dated RUSTSEC-2023-0071 exception. Before live money, either review that path or switch the function to `openssl`.
5. **Webhook shape.** Register a thin event destination for `v1.checkout.session.completed` and `v1.checkout.session.async_payment_succeeded`. On each event, verify the signature, take `related_object.id`, GET the session pinned with `Stripe-Version: 2026-10-28.endive`, check `payment_status`, and mark paid by session id. `customer_details` then never appears in webhook bodies, and our parser has no version coupling. The GET response still contains it, so parse only the four fields named in section 10 and never log raw responses. Fall back to snapshot events only if the sandbox shows a problem.
6. **Fulfilment fallback and card only.** A wallet collect on an unpaid order also GETs the session, rate limited per order, because sandbox retries stop after a few hours and live delivery can lag. Set `allowed_payment_method_types=[card]`, so `completed` normally means paid, but still check `payment_status` and handle the async event.
7. **Denominations fallback.** If the fallback is needed, build it as per-denomination blind RSA keys, and drop Cashu as a way to get it. Cashu forces the gate to hold the mint secret, or to accept DLEQ data that links token to purchase. This agrees with `red_team.md` L237 and L241.
8. **Privacy claim.** State that email cannot be configured away according to the docs, and that Stripe groups repeat buyers by card, email or phone. The voucher protects only the link from purchase to use. Name and postal code behaviour stay open until measured. Use the section 9 configuration and drop `customer_details` on receipt.
9. **Storefront placement.** One storefront instance, one SQLite file with `secure_delete=ON`, outside the CVM (confirms `red_team.md` #19 and #30: webhooks need inbound HTTPS). Delete orders after the retry window, retaining only what `purchase_wallet_abuse.md` L79 lists.
10. **When the blinded message arrives.** The task has the wallet send it when creating the order, while `purchase_wallet_abuse.md` L73 sends it at collect. Prefer collect: abandoned orders then hold no blinded value, and the first collect fixes the message for that order.
11. **Spikes before building (about 2 days, INFERRED).**
    - Compile 0.18.0 against rc.21, run the RFC 9474 vectors and RFC 9578 A.2 vectors, and time signing.
    - Run one sandbox Checkout and record each field shown to the buyer and returned by the API (email, name, postal code, wallet buttons).
    - Confirm thin event signature and `related_object` for a Checkout Session through `stripe listen --forward-thin-to`.
