# Spike: exercising Anonymous Credit Tokens

Run 2026-11-06, about 13:37 to 14:05 EST. Throwaway validation experiment, not the PoC. Synthetic data only.

All harnesses and raw output live under `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act`. That is a session temp directory, so copy it if the evidence has to outlive the machine's temp cleanup.

## Bottom line

- **Tested:** `anonymous-credit-tokens` 0.4.2 (Ristretto255, BLAKE3, CBOR), the reference implementation linked from `draft-schlesinger-cfrg-act-01`. Provenance validated and version pinned.
- **Not tested: `act-rs` (EC-ACT, P-256).** This is the primary candidate named in `credentials_crypto.md`. This session's auto-mode permission classifier refused its build with the reason "Code from External". I did not retry or route around that. Nothing from `act-rs`, `ihat-rs` or the `p256` stack was compiled or run. Every EC-ACT statement below comes from reading source and specs. See [Blocked: act-rs](#blocked-act-rs) for the decision this leaves with you.
- **Ceiling and change works at the crypto level.** 10,000, spend 2,000, return 600, gives 8,600, and a second spend works. In 0.4.2 it costs two proof verifications when authorization must precede inference, because there is no verify-only call.
- **The library gives you the proof system, not the money system.** It keeps no nullifier state, `refund()` is randomized, and it offers no retry support. A nullifier-only store (the shape in the crate README) strands the client's whole remaining balance if one response is lost. A result store fixed this in the in-memory test at about 240 bytes per spend.
- **Honest load is not the concern; junk is.** On this laptop at L=32 one core does about 617 spends per second. A junk proof costs an attacker about 0.4 us of CPU and the server about 1.53 ms, so about 650 junk requests per second (3.2 MB/s) occupy one core.

## Blocked: act-rs

I had pinned `act-rs` at `0fe1d8f75ba1fa40e9b464b27d747555f560c889` and finished the provenance checks in the table below. The command denied was:

```
cd /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/scripts/h_ecact
source ../env.sh && cargo build --release --locked --offline
```

The denial reason gave no detail beyond "Code from External". The registry-published `anonymous-credit-tokens` build ran just before it. `act-rs` differs in being an untagged git commit that no draft links to, but I do not know whether that is why it was refused.

**Your decision:** allow that build if you want EC-ACT measured, since it is the implementation the PoC plan leans on. The harness source `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/scripts/h_ecact/src/main.rs` covers T1 to T6, but it has never been compiled, so expect some fixes. It would answer the main open questions: whether the `verify_spend` and `issue_refund` split removes the second verification, what `ctx_spend` binding does in practice, and what the real P-256 costs and sizes are.

## Provenance

| Item | Canonical source and how established | Publisher | Version or commit | Licence | Checksum or signature | Registry owner vs upstream | Advisories | Build or install scripts | Outcome |
|---|---|---|---|---|---|---|---|---|---|
| `anonymous-credit-tokens` | Appendix B.1 of `draft-schlesinger-cfrg-act-01` links github.com/SamuelSchlesinger/anonymous-credit-tokens as the reference implementation | Samuel Schlesinger (draft co-author; GitHub profile says cryptography engineer at Google Chrome) | 0.4.2, published 2026-02-16, commit e51acfef8603f442e3b38b34f5af00688c01c645 | Apache-2.0 | SHA-256 fea70ed0...29ee2 matches download, crates.io API and sparse index. No signature available: tag is lightweight, commits unsigned | crates.io owner SamuelSchlesinger only, matches repo owner. `.cargo_vcs_info` matches tag; crate-vs-tag diff shows only packaging differences. Tag sits on `origin/v0`, not `main` | OSV: none. README says "experimental and unaudited" | `build = false`, no build.rs, `forbid(unsafe_code)` | **Run** |
| `act-rs` and `ihat-rs` | Same GitHub org (Moderation-of-unLinkable-Endorsements) as the source repo of `draft-schlesinger-mole-ec-act-00`. The draft repo README says "Existing implementations: To be added", so it is **not** formally linked from a draft | MoLE org. Contributors include draft authors Schlesinger; also Meunier, Patton | `act-rs` 0fe1d8f75ba1fa40e9b464b27d747555f560c889 (2026-11-04), `ihat-rs` 3767cebd01936be5bfcfece75005c4524243cd08. No tags or releases | Apache-2.0; `ihat-rs` MIT OR Apache-2.0 | Git commit hash pin only; commits unsigned | Not on crates.io. **Name collision:** crates.io `act-rs` belongs to an unrelated actor framework | OSV: none for locked deps | No build.rs in either. `scripts/build-boringssl.sh` not used | **Not run: build denied** |
| Rust dependencies | `Cargo.lock` for each harness, crates.io sparse index checksums | See TSV: RustCrypto, dalek-cryptography, dtolnay, rust-lang and similar | 80 locked crate versions across both harnesses, all recorded | Not individually checked | Cargo lockfile checksums | Owners and repository recorded per crate; I saw no anomalies on review | OSV: none for any locked version, including `curve25519-dalek` 4.1.3 | Searched build scripts for network and process calls: hits were `rustc --version` probes, `cc`, and local OUT_DIR writes. Read `curve25519-dalek` build.rs start | Subset for `h_ristretto` built |
| Cloudflare CIRCL | Cloudflare's blog post introducing CIRCL links github.com/cloudflare/circl; Go proxy origin metadata agrees | Cloudflare (GitHub org) | v1.6.5, commit cfa7c70defd831ffb0792ab2af560bfef43d60ca | BSD-3-Clause by reading LICENSE (GitHub API says NOASSERTION) | `h1:O64F26HEqNhznd/hrC5KZXVKYuKM2rx4deZDTc4ihQA=` from sum.golang.org, same in go.sum and binary metadata | Go has no registry owner; module path is the repo | OSV: none for 1.6.5; earlier advisories all fixed by 1.6.3. Dep `x/crypto` 0.54.0 has GO-2026-5932 for `openpgp`, which is not in this build's dependency graph (0 matches). README says as-is, experimental | No cgo or `go:generate` in `oprf` or `blindsign`; no net or exec imports there | **Run** |

The `crate_build_script_or_proc_macro` column in `provenance_deps.tsv` is a bug in my audit script. It prints `(True, None)` for every row, so ignore it and use `provenance_build_scripts.txt`.

No Go ACT implementation was found. `thibmeu/act-ts` is TypeScript, so I did not consider it.

## Method

- **Toolchain:** rustc 1.94.0 and go 1.26.3, both already on the machine. `CARGO_HOME`, `CARGO_TARGET_DIR`, `GOPATH`, `GOCACHE`, `GOMODCACHE` and, for Go, `HOME` were all redirected into the spike directory. Release build with LTO and one codegen unit, `--locked --offline`.
- **Harness:** `scripts/h_ristretto/src/main.rs`, subcommands `t1` to `t6`. Most tests use L=32; negative tests use L=16; field dumps use L=8.
- **Store model:** `Mutex<HashMap<nullifier, record>>`, in-process. A record is `Pending{digest}` or `Done{digest, refund}`, modelled on `draft-jms-mole-protocols-01` 5.2.3.2 and 5.2.5. There is no database, network, HTTP or Tor anywhere in this spike. "Response lost" means the harness dropped the bytes; "crash" means it went on to the settle step using only stored bytes. No process was killed.
- **Forged prover:** `scripts/evil_ristretto` is the crate with one change: the client-side `s <= c` check removed. Its proofs round-trip through CBOR into the unmodified issuer.
- **Timing:** thread CPU time (`clock_gettime(CLOCK_THREAD_CPUTIME_ID)`) plus wall time, per operation, 300 iterations. Tables show median, with p95 in brackets where noted. Two full ACT runs were taken. For CIRCL: same clock, `GOMAXPROCS=1`, thread locked, plus process CPU to catch GC.

## Results

### T1: happy path and partial return (L=32, one scripted run, 11 checks passed)

| Step | Result |
|---|---|
| Issue 10,000 | balance 10,000 |
| Spend ceiling 2,000, return 600 | new balance 8,600 |
| Spend again: ceiling 2,000, usage 123 (t=1,877) | 8,477 |
| t=101 with s=100 | `InvalidAmount` |
| t=s | accepted |
| s=0, t=0 | balance unchanged, new credential |

How partial return works in this API:

1. Client: `token.prove_spend::<L>(&params, s, rng)` returns `(SpendProof, PreRefund)`. `s` and the nullifier are public.
2. Issuer: `sk.refund(&params, &proof, t, rng)` verifies the proof **and** signs the new credential in a single call. The issuer chooses `t` alone, with `0 <= t <= s`.
3. Client: `prerefund.to_credit_token(&params, &proof, &refund, pk)` checks the issuer's DLEQ proof and gives a credential worth `c - s + t`.

Mapping to "pre-authorize a ceiling, return change after inference":

- The cryptography maps directly: `t` can be chosen any time after the proof arrives.
- In 0.4.2 the API does not separate verification from signing. To authorize before inference you call `refund(t=0)`, discard the result, then call `refund(t)` afterwards. The harness measured 2 verifications per spend this way. At about 1.5 ms each that is small next to inference.
- `prove_spend` takes no request or challenge input (source reading), so a proof is a bearer item, not bound to a request. A party who captures it can burn the spend. They cannot finalize it without the `PreRefund`.
- `t` and `s` are public, so the issuer learns exact usage per request and a lower bound on balance.
- `act-rs` looks better shaped here (source reading, not run): `verify_spend` returns a held `VerifiedSpend`, and `issue_refund(verified, t)` comes later, with `ctx_spend` binding. `VerifiedSpend` is not serializable, so a restart would still need to re-verify from stored bytes. Also, `draft-jms-mole-protocols-01` 5.2.3.2 records the decision `t` *before* the protected operation ("The Moderator MUST NOT attempt the protected operation unless this request's result was successfully recorded"). Choosing `t` after inference is allowed by the EC-ACT crypto but deviates from that protocol layer and needs a pending state.

### T2: negative tests (L=16, 27 checks passed)

| Attack | Result | Sample |
|---|---|---|
| Same proof twice, library alone | **Accepted both times.** Library holds no nullifier state | 1 |
| Double spend through store, new proof from same token | Same nullifier; rejected | 1 |
| Single-bit flips across a serialized spend (XOR 0x01 and 0x80 per byte) | 1,327 decode errors, 4,121 `InvalidProof`, **0 accepted** | 5,448 flips over 2,724 bytes |
| Replace s (2,000 to 1), nullifier, or ctx | `InvalidProof` | 1 each |
| Overspend through honest API; s >= 2^L | `InvalidAmount` at client | 1 each |
| Local balance inflated to 50,000 in stored token, then spend 20,000 | Client proves; issuer `InvalidProof` | 1 |
| Forged overspend with client check removed, s = 10,001, 20,000, 65,535 against c = 10,000 | Issuer `InvalidProof` | 3 |
| Re-request refund, store | Byte-identical stored refund; a new `t` is ignored | 1 |
| Re-request refund with larger t, library alone | **Mints a sibling worth 9,999 next to the 8,600 one.** They share a nullifier, so only one is spendable | 1 |
| Refund from another spend | `InvalidProof` | 1 |
| Replayed issuance request | Issuer signs again. Tokens share a nullifier | 1 |
| Other issuer key, other `Params`, wrong key at client | `InvalidProof` | 1 each |
| Trailing byte appended to a valid spend | **Accepted** | 1 |
| CBOR keys reversed in a valid spend | **Accepted** | 1 |

The last two matter for retry. With a digest over raw bytes, a party holding the proof could submit a malleated copy first, and then the victim's byte-identical retry gets rejected as a duplicate (tested). Hashing the re-encoded proof (`proof.to_cbor()`) removed both encoding variants (tested). Malleability of the proof values themselves was not tested.

### T3: failure recovery (L=32, 11 checks passed)

Issuer recorded the spend and the response was lost:

| Server design | Client retry | Outcome |
|---|---|---|
| Nullifier only (README `NullifierStore` shape) | Same request | Rejected. **8,600 credits stranded** |
| Nullifier, proof digest, stored refund | Byte-identical request | Byte-identical refund (176 B), **0 extra verifications**, 99 us wall including decode. Client finalizes 8,600 from state restored from bytes |
| Same | Fresh proof from the old credential | Same nullifier, digest differs, rejected. The stored refund also fails under the new proof and state |
| Pending then Done (two-phase) | Retry during inference, then settle after a simulated restart | Retry gets Pending. Settle is once-only; a later `t` is ignored. Client recovers 9,300 (usage 700). 2 verifications |

**Can the client safely retry?** Only if both sides were built for it. The library offers nothing beyond the proof operations.

State required:

- **Client, durable before sending:** `SpendProof` 4,919 B plus `PreRefund` 141 B = 5,060 B at L=32, stored as one atomic record. `draft-schlesinger-cfrg-act-01` 6.6.1 requires durable client state before a spend. EC-ACT says "A Client that loses the spend state after the Moderator has recorded the nullifier loses the balance."
- **Server, atomic with the decision:** nullifier 32 B, digest 32 B, refund 176 B, so at least 240 B per spend, or about 240 MB per million spends before indexes. Draft-01 6.6.1 allows expiring refund rows if clients are told the policy. The nullifier has to stay for the life of the key.

**Client-side trap found by accident.** I expected a later `PreRefund` combined with the original proof and refund to fail. It returned `Ok`, with a token claiming 8,600, and the issuer rejects that token's next spend. `to_credit_token` checks the DLEQ against the proof's commitments and never checks that the `PreRefund` opens them. So a wallet that mixes up records loses value with no error. `ristretto_t3_first_run_with_wrong_expectation.txt` keeps that first failing run.

**Does retrying leak or link?** By inspection:

- A byte-identical retry carries the same nullifier and bytes, so the issuer links the attempts to each other, for example across Tor circuits. It also learns that a retry happened.
- A fresh-proof retry also repeats the nullifier, and would expose a second `s` if it differed. Only `k`, `s` and `ctx` are identical between two proofs of one token.
- Nothing observed links the retry to other spends, but that is inspection, not proof.

**What the library is missing:**

- a verify-only call
- a store or idempotency interface (`ErrorCode::NullifierReuse` exists, but `refund()` has no state to raise it from)
- request binding
- strict decoding
- a check that the `PreRefund` matches the proof
- pending, timeout and crash-policy semantics
- key rotation and refund expiry

MoLE 5.2.5 specifies much of the record-and-replay behaviour in prose. MoLE 5.2.3.2 also warns that a failure after recording "can charge the Client" and leaves that recovery to the application. Not implemented here: who pays when inference crashes midway. The harness only applied a stand-in policy.

### T4: concurrency (L=32)

| Scenario | Result | Trials |
|---|---|---|
| Two proofs from one credential, racing threads, store holding a lock across check, verify and record | Exactly one winner every time | 200 |
| Same race, check then refund then insert with no atomicity | **Both accepted every time** | 200 |
| Four split credentials spending in parallel | All accepted every time | 50 |

The crate docs do say to record the nullifier atomically, so the second row is a demonstration of misuse. It shows that the window is real: both requests only need to land within the roughly 1.5 ms verification. My lock across verification serializes all spends, so a real server should verify outside the lock and then do an atomic insert-if-absent. That design was not tested.

How a wallet gets parallel requests:

- One credential allows one spend in flight, and the ceiling is locked for the whole inference. N parallel requests with ceiling C need N credentials, each holding at least C.
- There is no split primitive. It has to be composed: spend s=3,000 with t=0, then the issuer issues 3 x 1,000 against fresh requests (tested: 10,000 became 7,000 + 1,000 x 3).
- The issuer sees `s` in the clear and those issuances close together in time, unless the wallet spaces them out.
- There is no merge primitive. Spend-all plus issue works but reveals both balances (tested). The `act-rs` allowance `a` looks like it would hide one of them (source reading, not run).

### T5: visibility (one dump at L=8, scan of 2 wallets x 30 spends at L=16)

| Message | In the clear | Other fields (no repeats in the scan) |
|---|---|---|
| SpendProof (18 CBOR fields) | `k` (nullifier), `s` (amount), `ctx` | A', B_bar, Com[L], gamma, e_bar, r2_bar, r3_bar, c_bar, r_bar, w00, w01, gamma0[L], z[L][2], k_bar, s_bar |
| Refund (5 fields) | `t` (returned amount) | A*, e*, gamma, z |

- **Scan:** 5,040 32-byte values, 4,863 distinct. The only repeats were `s` (60 times), `t` (60 times) and `ctx` (60 times), all of which I deliberately held constant across both wallets. No repeat crossed fields.
- **Nullifiers:** the next nullifier does not appear in earlier messages. The first nullifier does not appear in the issuance request or response.
- **Per-user `ctx`:** a per-user value is accepted by the client and carried in the clear across refunds (tested). The library has no guard, so the client must check `ctx` against public config.
- **Per-user keys:** the issuer can identify a key by trial verification (hits 1 of 2 in the test). Clients must pin one public key.
- **Amounts as a fingerprint (analysis, not tested):** exact `s` and `t` are visible. Exact `t` could correlate with stream length or timing on the Tor side, so bucketing deserves attention.

**This is inspection. It cannot prove unlinkability.** It only fails to find a stable field in these samples, and it cannot see algebraic relationships between values.

### T6: cost (n=300 per cell, run 1, Apple M5 Max)

Median CPU in microseconds, with p95 in brackets:

| L | client request | server issue | client finalize issue | client prove spend | server decode spend | server verify+refund | client finalize refund |
|---|---|---|---|---|---|---|---|
| 8 | 31 (40) | 108 (124) | 61 (76) | 438 (514) | 27 (37) | 536 (654) | 62 (76) |
| 16 | 31 (34) | 107 (113) | 60 (65) | 697 (726) | 47 (52) | 866 (903) | 62 (67) |
| 32 | 31 (37) | 107 (115) | 60 (66) | 1214 (1272) | 88 (96) | 1534 (1628) | 64 (69) |
| 64 | 32 (37) | 106 (113) | 60 (64) | 2246 (2296) | 167 (179) | 2869 (2920) | 68 (73) |
| 128 | 33 (38) | 107 (118) | 61 (69) | 4319 (4443) | 331 (348) | 5548 (5716) | 77 (86) |

Serialized sizes in bytes. Only the spend varies with L, at 137 bytes per bit:

| L | request | issue response | spend | refund | token | PreRefund |
|---|---|---|---|---|---|---|
| 8 | 141 | 211 | 1,628 | 176 | 211 | 141 |
| 16 | 141 | 211 | 2,724 | 176 | 211 | 141 |
| 32 | 141 | 211 | 4,919 | 176 | 211 | 141 |
| 64 | 141 | 211 | 9,303 | 176 | 211 | 141 |
| 128 | 141 | 211 | 18,071 | 176 | 211 | 141 |

Throughput and junk-proof economics. Verifications per second means 1 / (decode + verify + refund):

| L | verifs/s/core run 1 | run 2 | server/prove (honest) | junk gen us | reject junk us | reject/gen CPU | reject known nullifier us | junk MB/s to fill one core |
|---|---|---|---|---|---|---|---|---|
| 8 | 1,774 | 1,594 | 1.29 | 0.21 | 475 | 2,264 | 28 | 3.4 |
| 16 | 1,095 | 991 | 1.31 | 0.21 | 821 | 3,912 | 48 | 3.3 |
| 32 | 617 | 611 | 1.34 | 0.38 | 1,528 | 4,020 | 88 | 3.2 |
| 64 | 329 | 307 | 1.35 | 0.50 | 2,929 | 5,858 | 169 | 3.2 |
| 128 | 170 | 151 | 1.36 | 0.75 | 5,779 | 7,705 | 334 | 3.1 |

Multi-thread throughput, 3 seconds per row, with verify, sign and decode on a shared proof (run 1, then run 2):

| L | 1 thread | 6 threads | 18 threads (6P + 12E) |
|---|---|---|---|
| 32 | 623, 541 | 3,293, 2,895 | 7,746, 5,284 |
| 128 | 170, 147 | 878, 791 | 2,162, 1,707 |

Reading these numbers:

- **Honest case is balanced:** the server does about 1.3x the client's work. Client effort is not a DoS deterrent, because a junk proof needs no proving. It is a valid-shaped template with a patched nullifier.
- **Junk rejection has no early exit:** it costs about the same as a real verification. A known-nullifier check first (88 us at L=32) stops replays only; a fresh random nullifier reaches full verification.
- **Bandwidth is the attacker's real cost:** about 3.2 MB/s per core at any L, since larger proofs cost both sides proportionally. This excludes TLS, HTTP and Tor, which were not measured. Tor's onion-service PoW and per-circuit limits are untested candidates for a defense.
- **Run variance:** run 2 ran under heavier load. Prove, verify and junk-reject medians were up to 13% higher, except at L=32 where they were within 1%. The worst difference among listed operations was 21.5% (`client_issue_request` at L=64), not counting sub-microsecond junk generation. See `cost_compare_runs.txt`.

### Comparison: CIRCL VOPRF and blind RSA (n=300, one run, Go, GOMAXPROCS=1)

Median CPU in microseconds. Sizes are raw marshalled elements, without any Privacy Pass or HTTP framing. Process CPU including GC is within about 11% of these medians (see `circl_t6.tsv`).

| Scheme | client blind | server issue | client finalize | server redeem | reject junk | request B | response B | token B |
|---|---|---|---|---|---|---|---|---|
| VOPRF ristretto255-SHA512 | 35 | 159 | 228 | 36 | 36 | 32 | 96 | 96 |
| VOPRF P256-SHA256 | 73 | 131 | 189 | 72 | 71 | 33 | 97 | 64 |
| Blind RSA-2048 SHA384-PSS (deterministic) | 49 | 1,860 | 22 | 21 | 20 | 256 | 256 | 288 |
| ACT L=32 | n/a | 107 (issue) | 64 | 1,622 (decode + verify + sign) | 1,528 | 141 (issue), 4,919 (spend) | 211 (issue), 176 (refund) | n/a |

- **Fixed-denomination only.** These tokens carry no amount and return no change, so this is not like-for-like. It prices what ACT's change and hidden balance cost.
- **Junk rejection:** ACT L=32 is about 42x the ristretto255 VOPRF and about 76x blind RSA.
- **Request rate needed to fill one core:** about 650 per second for ACT L=32, against about 27,500 for the VOPRF. By bytes they are close (3.2 MB/s against 2.6 MB/s) and blind RSA is higher (about 14 MB/s). So the ACT gap shows up in requests per second, and per-request limits bite sooner for ACT.
- **One ACT spend against tokens:** server CPU equals about 8.3 VOPRF issue-and-redeem cycles (1,622 / 195 us), and bytes equal about 23 such tokens (5,095 / 224 B). One blind RSA cycle (1,881 us, 800 B) costs more server CPU than an ACT L=32 spend. A token design needing many tokens per request would lose ACT's advantage quickly, which is the argument for ACT if the privacy properties hold. Language and curve differ across these rows, so treat the ratios as rough.

## Limitations

**What this proves:** on this machine, this crate version behaves as listed in these specific scenarios, at these sample sizes.

**What this does not prove:**

- **Security:** neither soundness nor unlinkability. Forged-overspend attempts failing at 3 amounts and bit flips failing at 2 positions per byte are not a proof. Security reductions and audits remain as described in `credentials_crypto.md`.
- **EC-ACT:** no `act-rs` code ran. Results on the older Ristretto crate (curve, encoding, API) do not transfer automatically. The spec's 4,531 B size at L=32 and the repo's self-reported timings stay unmeasured.
- **Draft conformance:** I did not replay draft test vectors, and I did not run the crate's own test suite.
- **Realistic failure:** no real process crash, database durability, network, HTTP or Tor. The store is an in-memory map, and its cost is excluded from throughput figures.
- **Side channels:** not examined.

**Laptop against target:**

| | This spike | Target |
|---|---|---|
| CPU | Apple M5 Max, arm64, 6 performance + 12 efficiency cores | x86-64 server cores in a GPU confidential VM on Verda (RTX PRO 6000 CC or B200 CC) |
| dalek backend | Serial 64-bit (build script allows SIMD only on x86-64) | Possibly AVX2 or AVX-512, not measured |
| Memory | No memory encryption | Encrypted memory, likely with some overhead |
| Conditions | On battery, shared with other spike jobs; load average 6 to 26 during runs | Dedicated, or not, unknown |
| Stack | No network, Tor, TLS or database | All of them |

The direction of the net difference is unknown. Whether the issuer would run in the confidential VM at all is an open design choice, so x86 numbers measured outside a CVM may be the relevant ones. Multi-thread figures mix core types and include whatever else was running. Thread CPU time is also not core-type normalized, though medians were stable within a run.

## Claims

| Claim | Status | Evidence file |
|---|---|---|
| 10,000, spend 2,000, return 600 gives 8,600 and a further spend succeeds | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t1.txt |
| Ceiling then change works in 0.4.2 with authorization before inference, at 2 verifications per spend | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t1.txt |
| Issuer's `t` is bounded by `s`; `t > s` is rejected | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t1.txt |
| The library by itself prevents double spending | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| Single-bit tampering of a spend is rejected (5,448 flips, 0 accepted) | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| Overspend beyond balance is rejected (honest API, inflated balance, 3 forged amounts) | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| Credential from another issuer key or other params is rejected | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| Re-requesting a refund is safe without a store | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| Spend wire encoding is non-malleable | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t2.txt |
| A nullifier-only store survives a lost response without losing client funds | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t3.txt |
| Byte-identical retry against a digest and result store returns the same refund with no re-verification (in-memory) | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t3.txt |
| A client can recover by generating a fresh proof from the old credential | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t3.txt |
| Client finalization detects a mismatched `PreRefund` | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t3_first_run_with_wrong_expectation.txt |
| Retries avoid linking the attempts to each other | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t3.txt |
| Parallel spends from one credential can both succeed under an atomic store | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t4.txt |
| A non-atomic check-then-insert is safe under concurrency | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t4.txt |
| A wallet can compose a split and then spend in parallel (50 of 50) | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t4.txt |
| In the clear: `k`, `s`, `ctx` in spends and `t` in refunds; nothing else repeated in the scan | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t5.txt |
| The client refuses a per-user `ctx` | refuted by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t5.txt |
| Spends are unlinkable | inconclusive | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t5.txt |
| On this laptop at L=32: about 617 verifications per second per core, 4,919 B spend, 176 B refund | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t6.tsv |
| Junk proofs are about 4,000x cheaper to produce than to reject at L=32, with no early exit | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/cost_compare_runs.txt |
| ACT junk rejection is about 42x a ristretto255 VOPRF and 76x blind RSA-2048 | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/circl_t6.tsv |
| These costs carry over to the target CVM | inconclusive | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/machine_load.txt |
| `act-rs` removes the second verification and binds spends to a request via `ctx_spend` | inconclusive (source reading, build denied) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/scripts/h_ecact/src/main.rs |
| EC-ACT spend is at most 4,531 B at L=32, with the repo's self-reported timings | inconclusive (not measured) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/downloads/ecact/draft-jms-mole-protocols-01.txt |
| The crate conforms to draft-01 test vectors | inconclusive (not run) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/act_sources/draft-schlesinger-cfrg-act-01.txt |

## Other evidence files

- Provenance: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/provenance_crate_sha256.txt`, `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/provenance_deps.tsv`, `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/provenance_build_scripts.txt`, `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/provenance_build_script_grep.txt`, `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/provenance_circl.txt`
- Second cost run: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/raw/ristretto_t6_run2.tsv`
- Harnesses: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/scripts/h_ristretto/src/main.rs`, `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act/scripts/h_circl/main.go`
