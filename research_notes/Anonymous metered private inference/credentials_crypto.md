# Anonymous-credential cryptography for prepaid, per-token metering

Research date 2026-11-06. This was a source-only investigation: no third-party code was run. Every claim about protocol or library behaviour comes from reading a spec or source, so it is **untested in practice** unless marked otherwise. The only thing executed was my own standard-library simulation of amount leakage.

Section mapping for the three evidence classes: "Cited Findings" = verified from a primary source, "Inferences" = my reasoning, "Gaps" = unknown.

Versions relied on:

- `draft-schlesinger-mole-ec-act-00` (2 Nov 2026), read from the repo's Markdown source on 6 Nov 2026
- `draft-jms-mole-protocols`, `draft-jms-mole-http-transport`, `draft-jms-mole-architecture`, same repo and same fetch date
- `draft-schlesinger-cfrg-act-01` (13 Feb 2026, expired)
- `act-rs` 0.1.0 (pushed 2026-11-04)
- `anonymous-credit-tokens` 0.4.2 (2026-02-16)
- Meta Private Processing whitepaper V3 (20 Oct 2026)

## Is the cryptographic foundation solid, and which primitives should the PoC use?

### Takeaway

The foundation is good enough for a capped PoC but not for a production money system on the current evidence. EC-ACT (the current name for ACT) has the right semantics, but its own draft says its security reductions "remain to be written out", it has no audit that I could find, and it has no production deployment I could find. Use EC-ACT through `act-rs` as the primary behind a replaceable interface, with partially blind RSA or VOPRF denominations plus change issuance as the fallback.

### Cited Findings

- The proposal's central idea is real at the crypto level: a spend reveals `s`, a refund returns `t` with `0 <= t <= s + a`, and the new balance is `c - s + t`. The draft says "The Moderator places the new balance anywhere in `[c - s, c + a]` without learning where in that interval it falls." — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- The draft states: "The credit-conservation reduction and the statistical unlinkability argument for the idealized scheme remain to be written out." — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- A real private-inference system already meters with blind credits rather than ACT: OpenPCC's `anonpay` uses partially blind RSA with the value as public metadata and `CommitWithUnspend` for change. — [OpenPCC anonpay](https://github.com/openpcc/openpcc/tree/main/anonpay), [processor.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/processor.go)

### Inferences

Recommendation:

| Role | Choice | Why | Cost |
|---|---|---|---|
| Primary for the PoC | EC-ACT via `act-rs`, pinned git rev, behind a `CreditBackend` interface | Matches the ceiling-and-return flow with one credential, hides the balance, compact, actively developed | Unproven, unaudited, individual draft, fast-moving spec, whole balance at risk per spend, designated verifier |
| Fallback | Partially blind RSA (OpenPCC pattern) or VOPRF denominations, with change by fresh blind issuance (Cashu NUT-08 pattern) | RFC-based building blocks with deployed precedent; only spent coins at risk; RSA variants let a verifier check without minting power | More wallet logic, more issuance traffic, no standard for the combined flow |
| Not suitable | ARC, plain BBS | ARC has no variable cost or refund; BBS needs a custom balance and nullifier protocol | |

Because the PoC takes real card payments, cap the purchase size and tell buyers the credit layer is experimental. A forgery bug costs compute credits; a linkability bug harms users and is the higher stake.

Corrections to the proposal:

1. ACT is not a CFRG product and has moved to the MOLE effort under a new name and curve.
2. No complete security proof is published.
3. The MOLE protocol profile fixes `t` before the operation runs, so post-inference settlement is a deviation.
4. Retiring a context drops balances, which conflicts with prepaid money.
5. Exact per-token `t` contradicts the spec's amount guidance.
6. Every spend exposes the whole remaining balance to loss.
7. Whoever holds `skM` sees the exact net charge per request and can mint.
8. Only the client knows the balance, so wallet loss means money loss.

At PoC scale, cryptography cannot deliver anonymity alone: with few buyers in a key and context, the anonymity set is at most those buyers, and any exact amount makes a request nearly unique.

### Gaps

- Whether a proof exists privately or in progress beyond the draft's statement. Ask the authors.
- Whether regulators or card networks treat a prepaid hidden balance differently (other track).

## What is ACT today: drafts, status, authors, implementations, deployment?

### Takeaway

ACT is an individual draft that recently moved and was renamed. The current text is `draft-schlesinger-mole-ec-act-00`, not adopted by any working group or research group. Two Rust libraries exist; neither claims an audit, and no production deployment was found.

### Cited Findings

Drafts and status:

- `draft-schlesinger-cfrg-act-00` (Aug 2025) and `-01` (13 Feb 2026; Schlesinger, Katz of Google) are expired individual drafts, expired 17 Aug 2026. — [datatracker](https://datatracker.ietf.org/doc/draft-schlesinger-cfrg-act/), [-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt)
- The `-01` abstract says "This document is a product of the Crypto Forum Research Group (CFRG)". Martin Thomson's issue on that text says "This is not true", and the line was removed from the editor's copy on 2026-07-11. — [-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt), [issue #27](https://github.com/SamuelSchlesinger/draft-act/issues/27)
- On 2026-10-01 Schlesinger wrote that the latest spec moved to the MOLE internet-drafts repo with `act-rs` as the implementation. — [PR #28](https://github.com/SamuelSchlesinger/draft-act/pull/28)
- The current spec is `draft-schlesinger-mole-ec-act-00`, 2 Nov 2026, expires 6 May 2027, individual submission; authors Schlesinger, Katz (Google), Faz-Hernandez (Cloudflare). — [datatracker](https://datatracker.ietf.org/doc/draft-schlesinger-mole-ec-act/)
- Companion drafts: protocols, HTTP transport, architecture, `draft-ladd-mole-ec-ihat`, `draft-authors-mole-vole-act`. — [repo](https://github.com/Moderation-of-unLinkable-Endorsements/internet-drafts)
- MOLE is not yet a working group. It has a BoF request and charter text, and its 2026-11-04 notes say "Review IETF 127 BoF status". — [administrivia](https://github.com/Moderation-of-unLinkable-Endorsements/administrivia), [meetings](https://github.com/Moderation-of-unLinkable-Endorsements/meetings)
- `draft-schlesinger-privacypass-act-01` (13 Feb 2026) has expired; its token type 0xE5AD was provisional. — [datatracker](https://datatracker.ietf.org/doc/draft-schlesinger-privacypass-act/)

Implementations:

| Library | Version and date | Suite | Licence | Audit and maturity | Tracks |
|---|---|---|---|---|---|
| [`act-rs`](https://github.com/Moderation-of-unLinkable-Endorsements/act-rs) | 0.1.0, created 2026-09-07, pushed 2026-11-04 | P-256, SHA-256, SHAKE128; RustCrypto default, BoringSSL optional | Apache-2.0 | No audit stated; "not an officially supported Google product"; byte-for-byte vectors from the draft's Python reference at L=1,2,4,8,16,64; CI with mutation testing and Miri | EC-ACT |
| [`anonymous-credit-tokens`](https://docs.rs/anonymous-credit-tokens/latest/anonymous_credit_tokens/) | 0.4.2, 2026-02-16 | Ristretto255, BLAKE3, CBOR; L up to 128 | Apache-2.0 | Says "experimental and unaudited" | Expired `-01` |

- Supply-chain trap: `act-rs` on crates.io (0.5.0) is an unrelated actor framework, and `ihat-rs` is not on crates.io, so the library must be a pinned git dependency. — [crates.io/act-rs](https://crates.io/crates/act-rs), [act-rs Cargo.toml](https://github.com/Moderation-of-unLinkable-Endorsements/act-rs)
- Cloudflare has a demo on an earlier draft, marked unaudited. — [agent demo](https://github.com/cloudflareresearch/anonymous-credentials-agent-demo)

Deployment:

- IETF 124 slides say "Cloudflare and Google are exploring deployment to rate limit AI agents" and that Chrome and Firefox are exploring. — [slides](https://datatracker.ietf.org/meeting/124/materials/slides-124-privacypass-anonymous-credit-tokens-00)
- Cloudflare wrote that ACT is not standardized and that it is "actively working on vetting" (30 Oct 2025). — [Cloudflare](https://blog.cloudflare.com/private-rate-limiting/)

### Inferences

- ACT was built mainly for anti-abuse moderation, not paid credits. The [MOLE BoF text](https://github.com/Moderation-of-unLinkable-Endorsements/administrivia) mentions that endorsements may tie to "a digital identity, a payment, or user account", so payment is in scope but is not the design centre.
- The published crate appears to come from a non-default branch: [`main`](https://github.com/SamuelSchlesinger/anonymous-credit-tokens) shows 0.2.1 while crates.io has 0.4.2 from commit e51acfef. Treat its provenance as something to check before use.

### Gaps

- CFRG and MOLE list threads: the mail archive search returned 403 and they were not read.
- I found no production deployment, but absence from public sources does not prove there is none.
- The commit hash of the spec text I read was not recorded; pin one when implementing.

## Is the security footing sound: proofs, known attacks, post-quantum status?

### Takeaway

Conditional. The building blocks have a strong lineage, but the EC-ACT reduction is unwritten, concrete security on P-256 is eroded by recent work on BBS-style schemes, and implementation pitfalls can be catastrophic. Money forgery is quantum-vulnerable; unlinkability is intended to be statistical but the quantum analysis is open.

### Cited Findings

Proofs:

- Stated assumptions are q-SDH in the group, the random oracle model and no known linear relations among generators. The reduction is "expected to follow" TZ23 in the algebraic group model. — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- "Bounds for the concrete scalar and nonce derivations in the random oracle model, and unlinkability against quantum random-oracle queries, also remain open." — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)

Known concrete-security issue:

- Chairattana-Apirom and Tessaro (ASIACRYPT 2025): after q signatures, BBS+ and deterministic BBS key recovery has Θ(q)-DL complexity, with O(√(p/q)) attacks for many q. — [ePrint 2025/1093](https://eprint.iacr.org/2025/1093). Only the abstract and search snippets were readable.
- cjpatton said in the draft's issue tracker that this implies larger curves for 128-bit security (Ristretto255 to Decaf448, P-256 to P-384), and compared CMZ and BBS proof models. — [issue #11](https://github.com/SamuelSchlesinger/draft-act/issues/11)
- A follow-up tight-proof paper exists. — [ePrint 2025/1973](https://eprint.iacr.org/2025/1973)

Spec-listed pitfalls:

- "A repeated nonce in the `IssueResponse` or `Refund` proof reveals `skM`", and a repeated exponent `e` under one context lets a client "forge Credentials at any balance below `2^L`, each with a fresh nullifier, which the nullifier store cannot detect". — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- Rejecting the identity element is "soundness-critical for `A_prime`". — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)

Post-quantum:

- "A quantum computer capable of solving discrete logarithms in `G` can recover `skM` from `pkM` and forge tokens." — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- `-01` says "This protocol is NOT quantum-resistant" and also that user privacy is preserved. — [-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt)
- VOLE-ACT is a candidate post-quantum variant: issuance 5.4 KB, spend 12.0 KB, "few milliseconds", undeployed, with immature UOV/MAYO cryptanalysis and proofs pending. — [Cloudflare vole-mole](https://blog.cloudflare.com/vole-mole), [code](https://github.com/cloudflareresearch/vole-mole)

### Inferences

- My arithmetic applying the abstract's O(√(p/q)) to P-256 (p≈2^256): q=2^30 gives about 2^113, q=2^40 about 2^108. That is margin erosion below 128 bits, not a practical break, and it affects forgery and key recovery, not linkability.
- Every refund is a signature, so q grows with requests rather than purchases. Rotating keys bounds q, but rotation collides with balance loss (see semantics).
- EC-ACT stays on P-256 and does not cite 2025/1093 (I searched the source). It may be accounted for implicitly, or not at all; ask the authors.
- For a PoC, the proof gap is an accepted and disclosed risk. For production it should block launch unless a proof and an audit appear, or the system moves to the fallback.

### Gaps

- Full text of 2025/1093 (403), so the exact conditions for P-256 and whether they transfer to the EC-ACT MAC are unknown.
- `Private_BBS_Security.pdf` and the old repo's `docs/design.pdf`, referenced as background, were not read. — [authenticated-pseudonyms](https://github.com/SamuelSchlesinger/authenticated-pseudonyms/blob/dev/design/Private_BBS_Security.pdf)
- No ACT-specific formal analysis or independent cryptanalysis was found.

## What are the exact semantics of issuance, spend, refund, verification, rotation, concurrency and recovery?

### Takeaway

Ceiling-and-return works at the crypto layer, but several protocol properties conflict with a prepaid product: `t` is decided before the operation in the MOLE profile, the verifier can mint, retiring a context drops balances, one chain supports one request at a time, and a refused or lost refund forfeits the whole remaining balance.

### Cited Findings

All from [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html) and [MOLE Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html) unless linked otherwise.

What each party sees:

| Step | Sent | Hidden | Visible to the holder of `skM` |
|---|---|---|---|
| Issue | Blinded commitment `K` and proof; response `(A, e)` and proof | Holder secrets | Chosen balance `c`, and time. Payment identity comes from the payment layer |
| Spend | Nullifier `k`, `s`, `a`, `A'`, `B_bar`, `K_n`, `Com1[]`, proof | Balance `c`, credential identity | `s`, `a`, nullifier |
| Refund | `t` and signature material | New balance | `t`, so net charge `s - t` exactly |

Ceiling and return:

- `-01` says "This enables pre-authorization patterns where the client holds s" and "The spent amount s is revealed to the issuer." — [-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt)
- `act-rs` exposes `verify_spend` (does not check the nullifier), `issue_refund(verified, t)` (rejects `t > s + a`) and `finalize_refund`. Its README example spends 30 with `t=5` and ends with balance 75. — [act-rs](https://github.com/Moderation-of-unLinkable-Endorsements/act-rs)
- **The MOLE profile decides `t` first.** Step 3 runs `IssueRefund` and records the result; "The Moderator MUST NOT attempt the protected operation unless this request's result was successfully recorded"; step 4 then attempts the operation. — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)
- "Spending credits authorizes one attempt of the protected operation. A failure after step 3 can charge the Client without completing step 4." — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)
- The transport has `Mole-Credential: held-for-update-0001="time"` and a Signoff request, but the 2026-11-04 notes say deferred update "was noted as part of moderator guidance, not exposed in the protocol or transport", with session PRs unresolved. — [Transport](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-http-transport.html), [meetings](https://github.com/Moderation-of-unLinkable-Endorsements/meetings)

Double-spend:

- Recording is atomic and "MUST fail if a result is already recorded under that nullifier", so of two racing presentations at most one is recorded. All servers on a key and context must share one atomic store. "The store MUST be durable: a Moderator that loses it re-admits every" spent credential. Nullifiers are retained while the key and context are accepted.

Designated verifier:

- "The signing key `skM` is also the verification key: `VerifySpend` requires it." "A Moderator that shares `skM` with another party lets that party issue Credentials in its name."

Rotation and expiry:

- `ctx_cred = credential_context || I2OSP(L, 1)`. "A refund is issued under the context of the spent Credential, so balances do not carry over to a new context." "this document does not specify balance migration." On retirement the client "MUST discard the Credentials and pending spend states".

Balance range:

- `1 <= L <= MAX_BIT_LENGTH`; `L` is fixed per key and context, public, and "a deployment SHOULD choose the smallest `L`". `act-rs` allows `1..=64`.

Concurrency:

- "one Credential supports one presentation in flight at a time"; "A Credential chain is therefore serialized". Slides pitch "Concurrent Sessions = Number of Credit Tokens". — [IETF 124 slides](https://datatracker.ietf.org/meeting/124/materials/slides-124-privacypass-anonymous-credit-tokens-00)

Recovery:

- A client missing an update may resend the byte-identical presentation within the retention period (example 300 s), or POST the handle `key_id || SerializeScalar(spend.k) || SHA-256(PresentationAndUpdate)`. The server answers `200` with the recorded update or `404`; a duplicate on the protected resource gets `409`. "The Moderator MUST NOT authorize another operation for a recorded nullifier, and MUST NOT change a recorded decision." "Once recovery ends, a retry is rejected, and the Client MUST discard the spend state." — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html), [Transport](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-http-transport.html)
- "A Moderator that records `NoUpdate` ends that Credential chain, and the Client loses its remaining balance with no cryptographic recourse."
- "A Client that loses the spend state after the Moderator has recorded the nullifier loses the balance." — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)

Binding:

- Presentations can be policy-scoped (bearer) or request-bound, and request binding must cover the body where relevant. `ctx_spend = SHA-256(credential_challenge)`.

### Inferences

**Post-inference settlement needs a custom two-phase flow.** It is compatible with the EC-ACT crypto but not conformant with the MOLE profile:

1. `verify_spend`, then atomically record `(k, digest, PENDING)` with the presentation bytes.
2. Run inference and stream.
3. Compute `t`, call `issue_refund`, atomically update to `Refund(t)`, and deliver in a final SSE event or via recovery fetch.
4. Publish a crash policy for records left PENDING, for example full return when no output was produced.

OpenPCC's `CheckAndLockNonce`, `Consume` and `Release` are a working precedent for this lock-then-settle pattern. An alternative that stays closer to the profile is to prepay in chunks during the stream, with each chunk a fresh presentation. That adds round trips but needs no PENDING state.

Recovery and unlinkability:

- A retry only re-exposes the same nullifier, so it links to itself and to nothing else. This is my reasoning, not a spec claim: the [transport draft](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-http-transport.html) still lists "Recovery: what recovery reveals and its lack of Client authentication" as a TODO.
- Recovery after a client crash works only because the client stored spend state before sending. A server-side retention period shorter than a Tor outage means permanent loss. Prepaid money suggests a far longer retention than the 300 s example.

Product conflicts:

- **Expiry equals forfeiture.** Options: a long context with overlap; full-balance spend and reissue, which reveals the balance; or a cross-context refund signing `K'` under the new context, which is unspecified and needs cryptographer review. The May 2026 Privacy Pass interim minutes mention proposed affine state transitions for epochs, which may address this. That third-party page is not authoritative. — [minutes](http://ietfminutes.org/minutes/2026-05-13/privacypass.html)
- **Wallet loss equals money loss.** Only the client knows the balance and secrets, so "show and reconcile budget" must be client-side, and multi-device use or backup needs its own design.
- **Split the wallet.** Several chains limit the loss per chain and give concurrency, but each chain must hold at least the largest ceiling. A second purchase naturally creates a second chain. No split or merge exists in the spec. Merging through `a` (spend all of A, then top up B) reveals and links the amounts.
- **Verifier location.** A gateway verifier can mint. An enclave verifier must hold `skM`, which also signs issuance, and it needs a shared durable nullifier store outside the enclave (cross-track).
- **Low balances.** A presentation needs `balance >= s`, so a small fixed ceiling set can strand dust and reveals a lower bound on the balance. The client must refuse any `s` outside the published list, because repeated challenges can recover the balance. [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html) states "Clients SHOULD limit the number of presentations they make in one context".

### Gaps

- Whether the MOLE authors will define deferred settlement, and in what shape; PRs #151 and #152 were unresolved.
- Whether re-running `verify_spend` on stored bytes after a crash has any subtlety; I inferred that it is safe because a non-cloneable `VerifiedSpend` only lives in memory.

## What zero-knowledge is actually involved? Are SNARKs needed?

### Takeaway

Only sigma protocols over linear relations, compiled with Fiat-Shamir. No SNARK, circuit or trusted setup is needed, so "ZK proofs" in the proposal is true but much lighter than the phrase suggests.

### Cited Findings

- Range proof: `L` bit commitments with weights, each with a `Bit` relation, plus a proof over Pedersen-style commitments, specified over `draft-irtf-cfrg-sigma-protocols` and `draft-irtf-cfrg-fiat-shamir`. — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- Heavier ZK appears only in VOLE-in-the-Head for the post-quantum variant. — [vole-mole](https://blog.cloudflare.com/vole-mole)

### Inferences

The only ZK work a PoC implements is calling the library. Proving that the enclave counted tokens correctly is a separate problem. Nothing in this stack proves that; attestation and client-side counting have to carry it (cross-track).

### Gaps

- `draft-irtf-cfrg-sigma-protocols` and `draft-irtf-cfrg-fiat-shamir` are themselves drafts. I did not review their status beyond the versions cited in EC-ACT.

## How do the alternatives compare, and what do Apple and Meta use?

### Takeaway

Apple and Meta use blind-signature or VOPRF credentials for authorization and rate limiting, not per-token metering. The strongest non-ACT option is denominated blind credits with change, as OpenPCC and Cashu already do. ARC and BBS do not fit.

### Cited Findings

| Scheme | Status and date | Variable charge and refund | Verifier needs | Funds at risk per spend | Fit |
|---|---|---|---|---|---|
| EC-ACT | Individual draft, 2 Nov 2026 | Native | `skM` only | Whole balance per spend | Best semantic fit, weakest assurance |
| Blind RSA ([RFC 9474](https://www.rfc-editor.org/rfc/rfc9474)) | Published RFC | Via denominations and change | Public key | Spent coins only | Strong fallback |
| VOPRF ([RFC 9497](https://www.rfc-editor.org/rfc/rfc9497)) | Published RFC | Via denominations and change | Secret key | Spent coins only | Strong fallback, smaller messages |
| Privacy Pass ([RFC 9576](https://www.rfc-editor.org/rfc/rfc9576.txt), 9577, 9578) | Published RFCs; batched tokens -10 at IESG | Fixed-value tokens | Depends on type | Spent coins only | Base for the fallback |
| [ARC](https://www.ietf.org/archive/id/draft-ietf-privacypass-arc-crypto-01.html) | Crypto -01, 2 Mar 2026; protocol -00, 4 Feb 2026 | N fixed-cost presentations, no refund | Secret key | | Unsuitable |
| [BBS](https://www.ietf.org/archive/id/draft-irtf-cfrg-bbs-signatures-12.html) and [blind BBS](https://datatracker.ietf.org/doc/draft-irtf-cfrg-bbs-blind-signatures/) | CFRG -12 (Sep 2026); blind -03 | None; needs a custom protocol | Public key, BLS12-381 | | Unsuitable as-is |
| [Cashu](https://raw.githubusercontent.com/cashubtc/nuts/main/08.md) | Community spec | NUT-08 blank outputs return change | Mint secret key | Spent coins only | Pattern to copy, not USD rail |
| [OpenPCC anonpay](https://github.com/openpcc/openpcc/tree/main/anonpay) | Apache-2.0 code | Partially blind RSA, value as public metadata, `CommitWithUnspend` | Public key possible (RSA); its `Processor` verifies through an `Issuer` object | Spent coins only | Closest precedent |

Details:

- OpenPCC encodes values as `1.XXXX × 2^Y`, up to 33,285,996,544, and rounds stochastically: "this rounding is guaranteed to be fair and unbiased over the long run". — [value.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/currency/value.go)
- OpenPCC's unspent amount returns as an unblinded credit, and its `Exchanger` swaps that for a fresh blind signature at the bank. — [processor.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/processor.go), [exchanger.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/exchanger.go)
- Cashu's example reserves 1,000, charges 100 and returns 900 as 4+128+256+512, using `n_blank = max(ceil(log2(fee_reserve)), 1)`. — [NUT-08](https://raw.githubusercontent.com/cashubtc/nuts/main/08.md)
- Routstr uses Cashu for LLM payments. — [Cashu blog](https://blog.cashu.space/routstr/)
- LLM-Tor uses blind-RSA single-use tokens and two long-lived abuse tokens. — [LLM-Tor](https://github.com/prince776/LLM-Tor)
- Kagi runs Privacy Pass plus a Tor onion for paying members. — [Kagi](https://blog.kagi.com/kagi-privacy-pass), [Tor](https://help.kagi.com/kagi/privacy/tor.html)

Apple:

- PCC uses a single-use credential on RSA Blind Signatures (RFC 9474) and a third-party OHTTP relay. — [Apple PCC](https://security.apple.com/blog/private-cloud-compute/)

Meta:

- Private Processing obtains "a new ACS credential which can be used a finite number of times", and ACS is used to authenticate and "rate limit access to our infrastructure". — [Meta whitepaper V3](https://ai.meta.com/static-resource/private-processing-technical-whitepaper)
- ACS "is built on top of VOPRFs (verifiable oblivious pseudorandom functions), blind signatures, and key derivation functions". — [facebookresearch/acs](https://github.com/facebookresearch/acs), [Meta engineering](https://engineering.fb.com/2022/12/12/security/anonymous-credential-service-acs-open-source/)

### Inferences

Size comparison for change on a 2,000 ceiling:

- Blank outputs: `ceil(log2(2000)) = 11`.
- Blind RSA-2048: 11 × 256 B = 2,816 B each way, and 11 × 2.69 ms ≈ 30 ms of signing.
- VOPRF on Cloudflare's figures: 11 × 32 B = 352 B request, up to 11 × 96 B = 1,056 B response, and 11 × 260 µs ≈ 2.9 ms.
- EC-ACT at L=14: 2,209 B spend plus 137 B refund.

So message size does not separate these options. Assurance, wallet complexity and failure blast radius do.

What the denominated fallback really gives up and gains:

- It also reveals the net charge through the denominations, and it links coins in one request, so it does not fix amount leakage.
- It avoids the whole-balance risk and, with RSA variants, lets a verifier check coins without minting power.
- The issuer of change can still mint, unless change is issued by a separate exchange service.
- Fixed-size coins plus one public-value change coin (OpenPCC) needs one blind exchange; many-small-coin change (Cashu) needs many.

Neither Apple nor Meta documents per-token metering, so the proposal's combination has no large-scale precedent. OpenPCC and Routstr are the closest smaller ones.

### Gaps

- Fedimint and the Lightning fit of Cashu were not researched.
- Apple's token batches are known to me only through third-party analyses. — [arXiv 2605.24239](https://arxiv.org/html/2605.24239), [arXiv 2604.15637](https://arxiv.org/html/2604.15637)
- ARC was not listed on the WG documents page I fetched, though the drafts are named `draft-ietf-privacypass-*`. Its status needs confirming. May interim minutes questioned its future. — [minutes](http://ietfminutes.org/minutes/2026-05-13/privacypass.html)
- Partially blind RSA's own document status was not checked.

## How do key-consistency (partitioning) attacks apply, and what mitigates them?

### Takeaway

The issuer can tag users with per-user keys or parameters, and client-side proof verification does not detect this. The relevant Privacy Pass drafts are expired, so there is no live standard. The practical mitigation is to bind key material into attested, publicly logged configuration, and to fetch it through a path the operator does not control.

### Cited Findings

- "Failure to apply a consistency check can allow Client-specific keys to violate Origin-Client unlinkability." Clients "SHOULD bound the number of active issuance configurations per Origin". — [RFC 9576](https://www.rfc-editor.org/rfc/rfc9576.txt)
- `draft-ietf-privacypass-key-consistency` and `draft-ietf-privacypass-consistency-mirror` are both shown by the datatracker as "Expired & archived" (last updated 2024-01-11 and 2024-08-02). MOLE still cites the mirror draft. — [key-consistency](https://datatracker.ietf.org/doc/draft-ietf-privacypass-key-consistency/), [consistency-mirror](https://datatracker.ietf.org/doc/draft-ietf-privacypass-consistency-mirror/)
- MOLE asks that deployments make configuration "consistent across Clients and resistant to split views" via consistency mirrors, key transparency or SCITT. Local tolerance limits "cannot entirely eliminate partitioning attacks". — [MOLE Architecture](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-architecture.html)
- Amounts are also a tagging channel: "The initial balance, `s`, `a`, and `t` MUST NOT vary per Client under a policy, since a Moderator could recognize a Client by the values it uses." — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)
- Meta distributes ACS and OHTTP key configs through a third-party CDN (currently Fastly), signed with a Meta-owned key hardcoded on the client, "to prevent Meta from being able to target specific users". Its threat model lists "ACS de-anonymization attacks" where insiders craft tokens, and says clients validate tokens with third-party-distributed keys. — [Meta whitepaper V3](https://ai.meta.com/static-resource/private-processing-technical-whitepaper)
- Meta's ACS derives keys from public attributes such as a time epoch, so "clients can verify the authenticity of the server easily". — [Meta engineering](https://engineering.fb.com/2022/12/12/security/anonymous-credential-service-acs-open-source/)

### Inferences

Tagging vectors in this design: per-client `pkM`, `ctx_cred`, `L`, issued `c`, ceilings, `t`, configuration URIs, and purchase-time timing.

Suggested mitigation for the PoC, all cross-track for the attestation owner:

1. Put `pkM`, `ctx_cred`, `L` and the price and amount tables in one configuration file, whose hash is bound into the attested measurement or report data.
2. Publish that hash in a transparency log, and let the client compare it across Tor circuits and over a third-party mirror.
3. Have the client reject non-listed amounts and verify `t` against enclave-reported usage.
4. Fetch configuration over the anonymous path, and do the same check in the clearnet purchase flow, since purchase is the one identified moment and the best tagging opportunity.

Issuance tagging is the critical case. If the buying client receives a different `pkM` at purchase, every later spend is linkable to that purchase, and this is exactly the link the proposal must prevent.

### Gaps

- Whether binding `pkM` into attestation is compatible with the operator being able to rotate keys without an image rebuild depends on the attestation track.
- MOLE's privacy section is a TODO list. It already concedes "Local Client checks do not establish consistency across Clients", but it does not yet state a required mechanism. — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)

## How much do amounts leak: exact per-token versus rounded?

### Takeaway

On a synthetic population, exact per-token amounts leave a median anonymity set of only 2 users at 1,000 active users, with 40% of requests unique. Rounding to 256 tokens lifts that to 233, about 100 times larger, and fair stochastic rounding removes mean overcharge. Exact metering contradicts the spec authors' SHOULD. Rounding gives nothing if the same observer can count stream chunks.

### Cited Findings

- "A Moderator SHOULD draw them from a small set of values fixed by policy, so that a value reveals no more than the policy decision it encodes." — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)
- The privacy section is still a TODO list, and it names both problems: "Anonymity-set partitioning by protocol type, keys, contexts, epochs, accepted-set contents and order, and public amounts" and "Amount tagging: conformance requirements do not prevent a malicious Moderator from choosing identifying amounts." — [Protocols](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html)
- `-01` says implementations "MAY add random delays and amount obfuscation", and it does not claim network, amount, timing or context privacy. — [-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt)
- OpenPCC uses a 4-bit mantissa value grid with stochastic rounding. — [value.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/currency/value.go)

### Inferences

**Simulation.** Script: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/amount_leak.py`; output is `amount_leak.out` beside it. Run with `python3 -I`, seed 20261106, standard library only.

The population is invented, not measured. Per-user log-mean is N(log 800, 0.7), per-request tokens are lognormal with sigma 1.0 (median 801, mean 1,690), about 3 requests per user in a one-hour window. Everyone is assumed active in the window, which is favourable to anonymity. `k` is the number of distinct users showing the same observed value in the window.

| Policy | median k, U=100 | U=1,000 | U=10,000 | U=100,000 | % of requests unique, U=1,000 | Entropy, bits, U=1,000 | Mean overcharge if rounded up |
|---|---|---|---|---|---|---|---|
| Exact | 1 | 2 | 13 | 122 | 40.0 | 10.67 | 0 |
| Multiple of 16 | 3 | 18 | 195 | 1,945 | 5.4 | 7.78 | 0.4% |
| 4-bit mantissa (OpenPCC grid) | 5 | 34 | 326 | 3,269 | 0.8 | 6.79 | 2.3% |
| Multiple of 64 | 10 | 67 | 738 | 7,072 | 2.1 | 5.93 | 1.9% |
| Multiple of 256 | 28 | 233 | 2,457 | 24,723 | 0.9 | 4.01 | 7.6% |
| Multiple of 1,024 | 80 | 803 | 8,124 | 80,701 | 0.3 | 2.08 | 34.1% |
| Powers of 2 | 44 | 445 | 4,124 | 41,342 | 0.1 | 2.89 | 44.5% |
| Five fixed buckets to 256k | 80 | 803 | 8,124 | 80,701 | 0.1 | 1.36 | 125% |

Fair stochastic rounding between adjacent multiples:

| m | Mean error | Mean absolute error per request | Requests charged zero |
|---|---|---|---|
| 64 | ≈0% | 21 tokens | 0.6% |
| 256 | ≈0% | 87 tokens | 7.2% |
| 1,024 | ≈0% | 352 tokens | 32.6% |

Join attack: an observer who estimates the amount from another source (for example response size) with 2% relative error gets this median candidate set:

| Policy | U=1,000 | U=10,000 |
|---|---|---|
| Exact | 33 | 340 |
| 4-bit mantissa | 60 | 595 |
| Multiple of 256 | 344 | 3,218 |
| Powers of 2 | 442 | 4,057 |
| Five fixed buckets | 834 | 7,973 |

Other results:

- **Total request volume** drives exact-value uniqueness: 100% unique at 50 requests in the window, 81.6% at 500, 27.0% at 5,000, 5.7% at 50,000, 1.1% at 500,000.
- **Sensitivity:** with request sigma 0.6, 1.0 and 1.4 at U=1,000, median k for exact stays at 2, for multiple-of-256 it is 229, 248 and 221. The conclusion does not depend on the sigma.
- **Profile re-identification** of a user from amounts already linked together is weak in this model: after 100 linked amounts, top-1 is 1.6% and top-10 is 11% with exact amounts, against chance of 0.1% and 1%. Most rounding policies barely change it, and differences of about 0.5 points are noise (1,000 trials). The five-bucket policy read 3.2% top-1, which I did not investigate; ties in a coarse grid may inflate it. This model has one per-user parameter, so it is a floor. Real users are likely to be more distinctive.

Interpretation:

- The amount does not name anyone by itself. It matters as a join key with another observation.
- At PoC scale, the anonymity set is limited by volume. Even with perfect bucketing, the set can't exceed the active users.
- **A deterministic grid is client-checkable** and stochastic rounding is only statistically checkable. For a PoC, a deterministic multiple-of-256 round-up plus a published policy is simplest. Use stochastic rounding once overcharge matters.
- **Rounding also shrinks proofs.** If one credit is 256 tokens, `L` falls by 8 bits, saving 8 × 129 = 1,032 B and about 2 ms of RustCrypto verification per spend.
- **Rounding is only worth it if the observer lacks a better signal.** If the Moderator runs in the enclave and only nullifiers leave, the operator does not see `s` or `t`, subject to TEE trust, and then needs aggregate revenue accounting. But if the operator or a relay sees stream chunk sizes or counts, it can estimate token count regardless, so padding or batching is needed too (cross-track).

### Gaps

- Real token-count distributions were not used. See Proposed validation.
- Token-length side channels in streamed LLM responses (I recall Weiss et al., USENIX Security 2024) were not verified here; this belongs to the Tor and streaming track.
- Because the MOLE privacy section is a TODO list, the authors' guidance on amounts is incomplete and may change.

## What are the message sizes and CPU costs, and do they matter over Tor?

### Takeaway

They do not matter for latency or bandwidth: about 2.2 to 4.5 KB per spend and low tens of milliseconds at most, against LLM and Tor latency. What matters is HTTP header limits at large `L`, and asymmetric verification cost as a denial-of-service lever over an anonymous network.

### Cited Findings

Sizes:

- With 33-byte elements and 32-byte scalars, "the request is `129` bytes and the response and refund are `137` bytes each. The spend message is `468` bytes for a refresh (`s = a = 0`) and otherwise `129 * n + 403` bytes". — [EC-ACT](https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html)
- Tor: `CELL_LEN` is 514, `CELL_BODY_LEN` is 509, and relay padding is `CELL_BODY_LEN - 11 - Length`, leaving 498 B of data. — [Tor preliminaries](https://spec.torproject.org/tor-spec/preliminaries.html), [relay cells](https://spec.torproject.org/tor-spec/relay-cells.html)
- nginx default: `large_client_header_buffers 4 8k`. — [nginx](https://nginx.org/en/docs/http/ngx_http_core_module.html)

`act-rs` timings, M-series single core, from its README (not reproduced by me):

| Operation | BoringSSL | RustCrypto |
|---|---|---|
| `issue_request` | 104 µs | 108 µs |
| `issue_response` | 199 µs | 429 µs |
| `finalize_issue` | 133 µs | 305 µs |
| `prove_spend` L=8 / L=64 | 1.78 / 10.2 ms | 2.59 / 14.6 ms |
| `verify_spend` L=8 / L=64 | 1.38 / 8.1 ms | 2.62 / 16.6 ms |
| `issue_refund` | about 124 µs | about 316 µs |
| `finalize_refund` | about 133 µs | about 304 µs |

— [act-rs](https://github.com/Moderation-of-unLinkable-Endorsements/act-rs)

Cloudflare's published comparison (client / issuer / finalize):

| Scheme | Issuance timings | Issuance sizes | Redeem |
|---|---|---|---|
| Blind RSA | 63 µs / 2.69 ms / 37 µs | 256 B each | 300 B, 37 µs |
| VOPRF | 54 / 260 / 376 µs | 32 / 96 / 64 B | 96 B, 57 µs |
| ARC | 323 / 1,349 / 1,293 µs | | Present 735 µs, 288 B; verify 740 µs |
| ACT, N=1,000 | 64 / 251 / 204 µs | | Present 1,867 B, 1.74 ms; verify 1.785 ms; update 0.508 ms |

— [Cloudflare](https://blog.cloudflare.com/private-rate-limiting/)

### Inferences

My size arithmetic (`129 × L + 403`):

| L | Max balance | Spend bytes | With 32-byte key id, base64url chars | Tor cells (498 B) |
|---|---|---|---|---|
| 14 | 16,383 | 1,806 + 403 = 2,209 | 2,988 | 5 |
| 24 | about 16.8M | 3,096 + 403 = 3,499 | 4,708 | 8 |
| 32 | about 4.3B | 4,128 + 403 = 4,531 | 6,084 | 10 |
| 40 | about 1.1T | 5,160 + 403 = 5,563 | 7,460 | 12 |
| 64 | about 1.8e19 | 8,256 + 403 = 8,659 | 11,588 | 18 |

- L=64 exceeds the 8 KB nginx default as a single header, and L=40 is close. Carry presentations in the body where possible, or configure the limit.
- Balance sizing with an illustrative price of $0.50 per million tokens: $100 buys 200M tokens, needing L=28 at one credit per token or L=20 at one credit per 256 tokens (2,983 B). The proposal's 10,000 example fits L=14.
- Linear interpolation for L=32: RustCrypto prove ≈ 2.59 + 24 × 0.2145 = 7.74 ms, verify ≈ 2.62 + 24 × 0.2496 = 8.61 ms; BoringSSL ≈ 5.39 ms and 4.26 ms. Capacity is about 110 spends per second per core on RustCrypto.
- Nullifier storage: 32 B each, so 1M requests per day ≈ 32 MB per day ≈ 11.7 GB per year before indexes, plus a result record per request during the retention period.
- **Denial of service is the real cost.** A forged presentation costs the sender almost nothing and costs the server about 8.6 ms at L=32. The spec's mitigation is rate limiting before verification, which is hard over Tor because there is no source identity (cross-track). Candidates are onion-service proof-of-work and a cheap early rejection on malformed sizes or a known nullifier.

### Gaps

- x86 and CC-instance timings are unknown, and memory-encryption overhead on CC instances could change them.
- Cloudflare's Go benchmarks are unreleased, so its numbers can't be reproduced from source.

## Proposed validation

None of this was run. Each item says what to do, what confirms or refutes the claim, and an estimate.

**V1. End-to-end EC-ACT test (half a day to one day).** Use `act-rs` at a pinned git rev, not the crates.io name. Optionally repeat on `anonymous-credit-tokens` 0.4.2.

Flow, at `L=14` and `L=32`:

1. Issue `c=10,000`.
2. Spend `s=2,000`, `a=0`; refund `t=600`; assert balance 8,600 (usage 1,400).
3. Spend `s=2,000` again with `t=0`; assert balance 6,600.

Negative checks, each of which must fail closed:

- Same nullifier presented twice, sequentially and in a concurrent race with exactly one fresh acceptance.
- Bit flips in each of `s`, `a`, `t`, `K_n`, `Com1[i]`, and the proof.
- Wrong `ctx_cred` and wrong `ctx_spend`.
- Identity element for `A'`, `t > s + a`, `s > c`, and amounts at or above `2^L`.
- Refund from a different spend fails in `finalize_refund`.
- A byte-identical replay returns the stored refund only, with no second authorization, and a third party's replay yields a refund it cannot finalize.

Measurements:

- Spend bytes equal 2,209 at L=14 and 4,531 at L=32; refund 137 B; request 129 B.
- Timings on target hardware within a factor of 2 to 3 of the README table.

Refuted if any negative case is accepted, sizes differ, or the balance is wrong.

**V2. Two-phase settlement and crash injection (one day).** Implement PENDING record, inference stub, `issue_refund`, final delivery. Kill the process before the record, after the record, mid-stream, after the refund, and before client finalization. Confirm that no credit is lost or doubled beyond the published policy, that re-verify from stored bytes works, and that expiry of retention produces the documented client loss. Refuted if any path leaves the client unable to proceed without a stated policy.

**V3. Fallback comparison (half a day).** Prototype 10,000 / 2,000 / 600 with denominated blind credits and change using a pinned library. Compare traffic, CPU, wallet code size and failure handling. This decides whether the fallback is cheap enough to switch to.

**V4. Real amount distribution (one to two hours).** Re-run the simulation with token counts from a public LLM usage dataset, by hour, including multi-modal users. Confirms or refutes the factor of about 100 between exact and multiple-of-256 at U=1,000.

**V5. Non-executable checks (days, elapsed).** Ask the authors whether a proof is in progress, whether 2025/1093 applies at P-256, and what shape deferred settlement will take. Commission review before real production money.

## Claim summary

All rows other than the simulation are source-read and **untested in practice**.

| Claim | Status | Evidence URLs |
|---|---|---|
| ACT or EC-ACT supports ceiling-and-return at the crypto layer | solid (untested in practice) | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html ; https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt ; https://github.com/Moderation-of-unLinkable-Endorsements/act-rs |
| ACT is a CFRG product or standards-track work | false | https://github.com/SamuelSchlesinger/draft-act/issues/27 ; https://datatracker.ietf.org/doc/draft-schlesinger-mole-ec-act/ |
| ACT's security is proven | false, as of the draft's own text; intended assumptions are stated | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html |
| Implementations are audited | unknown; none claims it, one says unaudited | https://github.com/Moderation-of-unLinkable-Endorsements/act-rs ; https://docs.rs/anonymous-credit-tokens/latest/anonymous_credit_tokens/ |
| ACT is deployed in production | unknown; only "exploring" found | https://datatracker.ietf.org/meeting/124/materials/slides-124-privacypass-anonymous-credit-tokens-00 ; https://blog.cloudflare.com/private-rate-limiting/ |
| EC-ACT gives 128-bit security on P-256 | weak; recent BBS results erode the margin, transfer unconfirmed | https://eprint.iacr.org/2025/1093 ; https://github.com/SamuelSchlesinger/draft-act/issues/11 |
| EC-ACT is post-quantum | false for forgery; privacy intended statistical, quantum analysis open | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html ; https://blog.cloudflare.com/vole-mole |
| SNARKs are required | false; sigma protocols only | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html |
| The MOLE profile allows post-inference refund | false today; deferred update unresolved, custom flow needed | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html ; https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-http-transport.html ; https://github.com/Moderation-of-unLinkable-Endorsements/meetings |
| The spend hides the balance while still enforcing it | conditional; holds by design, reductions unwritten | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html |
| Exact per-token billing fits spec privacy guidance | false; conflicts with a SHOULD | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html |
| Exact amounts narrow anonymity far more than rounding | conditional; simulated on a synthetic population | Own simulation; https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/currency/value.go |
| Gateway can verify without minting power under ACT | false; `skM` is both | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html |
| Balance survives key or context rotation | false in current drafts | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html |
| Crash or connection loss can be recovered idempotently without new linkage | conditional; works in retention, whole balance lost after it, privacy text TODO | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html ; https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-http-transport.html |
| One credential can serve parallel requests | false; one presentation in flight per chain | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-protocols.html |
| Key consistency has a live standard | false; relevant Privacy Pass drafts expired, mitigation is deployment-specific | https://www.rfc-editor.org/rfc/rfc9576.txt ; https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-jms-mole-architecture.html ; https://ai.meta.com/static-resource/private-processing-technical-whitepaper |
| Apple or Meta use credentials for metering | false; authorization and rate limit only, per their documents | https://security.apple.com/blog/private-cloud-compute/ ; https://ai.meta.com/static-resource/private-processing-technical-whitepaper ; https://github.com/facebookresearch/acs |
| Denominated blind credits with change are a workable fallback | solid as a pattern (untested in practice) | https://github.com/openpcc/openpcc/tree/main/anonpay ; https://raw.githubusercontent.com/cashubtc/nuts/main/08.md ; https://www.rfc-editor.org/rfc/rfc9474 |
| Message sizes and CPU are a problem over Tor | false for latency; conditional on L for headers; verification cost is a DoS issue | https://moderation-of-unlinkable-endorsements.github.io/internet-drafts/draft-schlesinger-mole-ec-act.html ; https://github.com/Moderation-of-unLinkable-Endorsements/act-rs ; https://spec.torproject.org/tor-spec/preliminaries.html ; https://nginx.org/en/docs/http/ngx_http_core_module.html |
