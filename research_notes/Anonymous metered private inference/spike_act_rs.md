# Test: act-rs (EC-ACT on P-256)

Run 6 November 2026 after the user approved the build. This closes the gap recorded in `spike_act.md`, where the same build was refused and no EC-ACT code ran.

**Result:** `act-rs` works for what the PoC needs. It handled spend, partial refund, top-up, retry after a lost response, restart and tampering in these tests. It costs about six times the CPU of the older Ristretto crate, and its proofs and security arguments remain unfinished according to the draft. These tests show the library behaves as intended, not that the scheme is secure.

## What ran

| Item | Value |
|---|---|
| Code | `Moderation-of-unLinkable-Endorsements/act-rs` at `0fe1d8f75ba1fa40e9b464b27d747555f560c889`, the `main` HEAD, version 0.1.0, Apache-2.0 |
| Machine | M5 Max laptop, arm64, 18 cores, macOS |
| Backend | RustCrypto P-256 (`p256` 0.14). BoringSSL was not tested |
| Harness | My 665-line harness, patched copy at `scratchpad/spikes/act_rs/scripts/h2` |
| Raw output | `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/act_rs/raw/` |

Scratch paths are temporary. Copy the raw output out if it needs to be kept.

## Provenance

| Check | Finding |
|---|---|
| Repository | Created 2026-09-07, last pushed 2026-11-04, 1 star, not a fork |
| Organisation | Created 2026-05-08, describes itself as MoLE-related W3C/IETF draft specifications |
| Contributors | SamuelSchlesinger 19 commits (draft author), thibmeu 4 (Cloudflare address), cjpatton 3 |
| Signature | The pinned commit's GitHub signature is **unverified** |
| Status | README says "not an officially supported Google product". `deny.toml` carries a Google copyright |
| Lockfile | 115 packages: 113 registry, 1 git (`ihat-rs` at `3767cebd`, same organisation), plus `act-rs` itself |
| Runtime dependencies | 35 entries including `act-rs` and `ihat-rs`. Mostly RustCrypto crates |
| Build scripts in runtime dependencies | `getrandom`, `libc`, `num-traits` have `build.rs`. No proc macros in the runtime tree. No `build.rs` in `act-rs` or `ihat-rs` |
| Advisories and owners | See "Dependency audit" below |

To run upstream's tests, dev-dependencies (`hex`, `criterion`) were downloaded into a scratch cargo directory. `cargo-audit` and `cargo-deny` are not installed and were not run.

## Upstream tests

32 of 32 passed: 29 unit, 2 draft-vector and 1 doctest group, including `draft_l4` and the extended vectors. Upstream's demo printed a 1,435-byte spend and a 468-byte refresh at its own `L`.

## Scenarios

| Test | Result | Findings |
|---|---|---|
| T1 lifecycle | 12 of 12 | 10,000 − 2,000 + 600 = 8,600, then spent again to 8,477. `t > s + a` rejected. Top-up through allowance gives 8,477 − 2,000 + 2,500 = 8,977. Two-phase Pending then settle works. `ctx_spend` binding holds |
| T2 tampering | 22 pass, 1 harness error explained below | 4,934 single-bit flips at L=16 with 0 accepted (653 decode errors, 4,281 verify errors). Forged overspends of s = 10,001, 20,000 and 65,535 against a balance of 10,000 rejected |
| T3 retry and restart | 10 of 10 | See below |
| T4 races and splitting | 2 PASS lines | Atomic store had one winner in 100 of 100 races. A non-atomic store accepted both in 100 of 100. Split conserves value, and 4 split credentials spent in parallel 30 of 30 times. Merging through spend-all plus top-up hides one balance |
| T5 wire contents | Inspection | `k`, `s`, `a` and `t` are in the clear. `ctx_cred` is not on the wire, but a per-user value can be recovered by trial verification. No stable field found. This is inspection, not a proof of unlinkability |

**The one T2 failure is a harness error, not a library defect.** The test applied a refund of `t=600` to a state with `s=5` and expected `Error::Verify`. `finalize_refund` correctly returned `Error::Amount` first, because the amount check runs before the signature check. A new case applying the same refund to `s=1000` reached the signature check and returned `Error::Verify` as expected.

**T3 results that shape the design:**
- A store holding only nullifiers strands the 8,600 balance when a response is lost.
- Storing the byte-identical refund (137 B) recovers it with no second verification.
- Generating a fresh proof reveals the same nullifier, so retry attempts can be linked to each other. A retry must reuse the original message.
- A retry during inference returns Pending, settlement happens once, and 9,300 was recovered in the test.
- `VerifiedSpend` is not serialisable, so a process restart has to verify the stored spend again.
- Client in-flight state at L=32 is 4,652 B (Credential 137, SpendMessage 4,531, state 121).
- The server's minimum result record is 233 B. A recovery handle `key_id || k || SHA-256(presentation)` is 96 B.

**Other library behaviour to design around:**
- `verify_spend` keeps no nullifier state. The caller owns double-spend protection.
- `issue_refund` is randomised. Issuing again with a higher `t` mints a sibling sharing the next nullifier, and only one sibling can be spent. So a server bug or malicious retry cannot double the value, but the server must still store one result.

## Timing (T6)

Two runs of 300 iterations each, single-threaded. CPU-time medians in microseconds, shown as run 1 / run 2. Sizes are bytes.

| L | Max balance | Spend size | Prove | Verify | Reject junk | Reject duplicate |
|---|---|---|---|---|---|---|
| 8 | 255 | 1,435 | 2,397 / 2,426 | 2,770 / 2,794 | 2,839 / 2,854 | 64 / 65 |
| 16 | 65,535 | 2,467 | 4,028 / 4,054 | 4,838 / 4,887 | 4,939 / 5,006 | 110 / 112 |
| **24** | **16.7M** | **3,499** | **5,680 / 5,638** | **6,949 / 6,888** | **7,097 / 7,035** | **159 / 157** |
| 32 | 4.29B | 4,531 | 7,338 / 7,251 | 9,048 / 8,941 | 9,282 / 9,144 | 205 / 202 |
| 64 | 1.8e19 | 8,659 | 13,910 / 13,701 | 17,427 / 17,153 | 17,890 / 17,482 | 391 / 385 |

At every `L`, issuing a refund takes about 0.30 ms, finalising a refund about 0.29 ms, generating junk under 1 µs, and the credential, issue response and refund are 137 B each.

Multi-core throughput of decode, verify and refund together, in spends per second:

| L | 1 thread | 6 threads | 18 threads |
|---|---|---|---|
| 32 | 102 / 105 | 563 / 556 | 1,472 / 1,451 |
| 64 | 55 / 56 | 290 / 293 | 769 / 756 |

L=24 was not in the multi-core test. Scaling from its single-thread verify, it would be about 140 per second per thread, or about 2,000 on 18 threads. That is an estimate.

**Quality of these timings:**
- An earlier run overlapped a parallel `cargo test` and was discarded. Its L=32 verify (8.90 ms) nevertheless matches these runs.
- For the runs above, load average was 5.7 at the start, from Firefox, iTerm and system services outside this test. It rose to about 33 during the runs because the harness's own multi-core section uses 18 threads.
- Run-to-run ratios stayed between 0.975 and 1.02 for all operations except junk generation, which is too fast to time well at under 1 µs.
- Everything ran on an arm64 laptop. EPYC Turin figures may differ, and the x86 test remains open.

### Compared with the older crate

| Measure at L=32 | `anonymous-credit-tokens` 0.4.2 (Ristretto255) | `act-rs` (P-256, RustCrypto) | Ratio |
|---|---|---|---|
| Spend size | 4,919 B | 4,531 B | 0.92 |
| Prove | 1.21 ms | 7.25-7.34 ms | about 6 |
| Verify and issue refund | 1.53 ms | 9.2-9.35 ms | about 6 |
| Reject junk | 1.53 ms | 9.14-9.28 ms | about 6 |
| Reject duplicate | 88 µs | 202-205 µs | about 2.3 |
| Finalise refund | 64 µs | 291-297 µs | about 4.5 |

The verify figure read from the README and interpolated earlier was 8.61 ms. Measured here it is 8.94-9.05 ms, about 4-5% higher.

Against RSA-2048 blind signature verification (about 20 µs in Go/CIRCL, from `spike_act.md`, a different language and run), rejecting junk EC-ACT at L=24 costs roughly 350 times as much. This matters only for denial of service, because honest load is far below capacity.

## What this means for the PoC

1. **Use L=24.** It holds 16.7M credits, enough for a $5 credential at 1 credit per base-priced token, at 3,499 B and about 7 ms verify.
2. **CPU is not the limit for honest use.** A 30-vCPU VM should verify hundreds of spends per second, and a session opens with one spend.
3. **Junk spends are the cheap attack.** Junk costs the attacker under 1 µs and the server about 7 ms. At 3.5 KB per attempt, Tor bandwidth is likely to run out before 30 vCPU do. That is an inference, so the gate should still check the nullifier first, cap concurrent verifications, and turn on Tor's proof-of-work defence.
4. **Store spends before verifying is complete and store the result before releasing it.** T3 confirms this is what keeps a balance recoverable.
5. **Keep EC-ACT behind a small interface.** The library is version 0.1.0, one commit signature is unverified, the draft's security arguments are unfinished, and the unlinkability finding is inspection only.

## Not tested

- x86, EPYC, a confidential VM, or the BoringSSL backend
- Any network, real database, or crash of a real process (restart was simulated in-process)
- Side channels or constant-time behaviour
- Security proofs, which these tests cannot establish
- `cargo-audit` and `cargo-deny`, which were not run

## Dependency audit

Run 6 November 2026 with `audit_deps.py` against the 113 registry packages in `act-rs`'s `Cargo.lock`. It queried crates.io for owners and repositories and OSV for advisories, reading data only.

- **Advisories:** 0 OSV matches across all 113 packages. OSV coverage is not complete, so this means no known advisory, not that none exists.
- **Owners:** all lookups succeeded. The 33 runtime registry crates are owned by established maintainers: RustCrypto (`tarcieri`, `newpavlov`), the Rust project (`cfg-if`, `libc`), rust-random, zkcrypto (`ff`, `group`), dalek-cryptography (`subtle`), `cuviper` (`num-traits`) and `paholg` (`typenum`).
- **Not on crates.io:** `ihat-rs` comes from git at a pinned revision in the same organisation as `act-rs`, so it has no registry owner or advisory record. It is the least-reviewed piece of the runtime tree, together with `act-rs` itself.

Raw output: `act_rs/raw/audit_deps.tsv` in the scratch folder given above.
