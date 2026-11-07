# Spike: LLM-style streaming over a real Tor onion service

Run 2026-11-06, 13:37 to about 14:25, on the user's Mac (arm64, macOS 26 / Darwin 25.4). Throwaway validation, not the PoC.

Raw files are under `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw`. That directory is session scratch, so copy it if the evidence needs to outlive the session.

## Bottom line

- **Works:** Arti 2.8.0 hosted an onion service and streamed to a separate Arti client. All 44,100 tokens across the 11 streaming and sustained runs arrived, and 2 of 2 five-minute streams survived.
- **Latency is usable but bursty:** median per-token latency was 264-415 ms. Stalls over 1 s occurred about once per 73 stream-seconds, and the longest was 9.9 s.
- **Unlinkability has a real cost:** a fresh isolated circuit took a median 4.33 s to first byte, against 1.37 s when reusing a circuit. Fresh circuits also failed sometimes (3 of 42 instrumented attempts, plus one 120 s hang).
- **Timing:** at the client's application reads, gaps under 50 ms did not survive Tor (Spearman 0.04-0.12, against 1.00 without Tor). Pauses and overall rate did survive. 250 ms batching weakens the signal but does not remove it.
- **Not tested:** what a network observer sees at cell level, especially next to the service, where timing has not yet crossed the Tor network.
- **Incident:** for up to 53 s my onion service forwarded to another spike's local server. See [Incident](#incident-onion-service-briefly-forwarded-to-another-spikes-server).

## Provenance

| Item | Arti (only third-party code run, with its locked dependencies) |
|---|---|
| Canonical source | https://gitlab.torproject.org/tpo/core/arti, linked from Tor Project's site https://arti.torproject.org/core/arti/. The crate's `repository` field names the same repo. |
| Publisher | The Tor Project. crates.io publisher of 2.8.0 is `gabi-250` (Gabi Moldovan). |
| Version | 2.8.0, tag `arti-v2.8.0`, commit `7c038125baef924a4bf287429c192c5af606f79c`. Published 2026-11-04, two days before this run. |
| Licence | MIT OR Apache-2.0 |
| Checksum | Crate SHA-256 `34f021b3ac0b757e0c60710fd15df3b5157d41327289ebb0ab493bce1050b0d4` matches the crates.io index. Not yanked. |
| Source match | `.cargo_vcs_info.json` names the tag commit. `build.rs`, `src/main.rs` and `src/lib.rs` are byte-identical to the GitLab tag. Other files were not compared. |
| Signature | `git verify-tag` reports a good signature from `8F56AA5800BA4E2325EFFCC83946E0ADE72BAC99`, served by keys.openpgp.org for gabi@torproject.org. The key is not in any web of trust I hold. |
| Registry owners | `gabi-250`, `ijackson`, `nmathewson`, `dgoulet-tor`; the same four on all 7 crates checked. They match upstream committers with @torproject.org commit emails (gabi@, iwj@, nickm@, dgoulet@). The match is by name and email, not cryptographic, because crates.io logins are GitHub identities. |
| Advisories | rustsec.org has no page for 5 Arti crates checked (HTTP 404); the same URL pattern returns 200 for `openssl`, so the lookup works, but a missing page does not show the crates are free of advisories. The 2.8.0 changelog fixes TROVE-2026-069 and -070 and upgrades dependencies for a batch of RUSTSEC non-constant-time advisories. Arti's TROVE tracker itself was not read. |
| Build scripts | Top-level `build.rs` only prints a `rustc-check-cfg` line. Dependency build scripts were not reviewed. |
| Dependencies | `--locked`; 727 of 728 lockfile packages come from the crates.io index, the other is Arti itself, and there are no git sources. |
| Binary | SHA-256 `0d886a8699095ef1e58ab2ea81137c47e0df26512821c86b6456d011f2a71b21`. Built in 55 s with rustc 1.94.0. |

My own harness uses the Python standard library only. The pre-existing Homebrew `python3`, `curl` and `gpg` were used. The C tor fallback was not needed and was not downloaded.

### Is the onion service marked experimental?

**Not in the build flags, but upstream's documents disagree, so treat it as not production-ready.**

| Source | What it says |
|---|---|
| `Cargo.toml` and README at the tag | `onion-service-service` is an ordinary additive feature, not gated by `__is_experimental`. I built without the `experimental` feature. |
| `doc/OnionService.md` at the tag | "suitable for testing and experimentation only", and "may even compromise the privacy of your other uses of the same Arti instance". It calls itself temporary and dates from February 2024. Its "missing Vanguard support" point looks stale: `vanguards` is now a default feature with lite mode as the default. I did not verify vanguard use at runtime. |
| arti.torproject.org | "Arti's proxy and onion service support are ready for use, but not all features of C Tor are available" (page last updated Jul 23, 2026). |
| `capability-limitations.md` | Says hosting is only partly implemented, but carries its own warning that it has not been updated since 2023. |

Two gaps matter for a public inference service: `hs-pow-full` (proof-of-work DoS defence) is experimental in `Cargo.toml`, and the README says restricted discovery is still experimental.

## Method

- **Generator:** a Python asyncio SSE server on 127.0.0.1 emitting synthetic text as `chat.completion.chunk`-shaped events of about 103 bytes, with `TCP_NODELAY` set.
- **Service:** `arti proxy` with its own state directory, no SOCKS port, and onion port 80 forwarded to the generator. No relay or exit was run.
- **Client:** a separate `arti proxy` with separate state. A raw-socket Python client speaks SOCKS5 and logs each `recv()` time and size. A fresh SOCKS username gives a fresh isolated circuit.
- **Clocks:** both sides use `time.monotonic_ns()` on one machine, so one-way latency is directly comparable here.
- **Control:** the same generator and client over loopback with no Tor, which bounds instrument noise.
- **Order:** phases ran one at a time, except the two sustained streams (concurrent by design) and the loopback control alongside the exit test.

Additions beyond the brief, all GET only with no credentials:
- A loopback control.
- `check.torproject.org/api/ip` on each first-pass exit circuit, to confirm that circuits used different exits.
- A second exit pass following redirects, because 307 and 308 alone say little.
- Fetches of the release tag from the official repo and the signing key from keys.openpgp.org.

## Results

### 1. Cold start (n=5, empty client state each time)

Seconds from client launch. All 5 succeeded.

| Milestone | Min | Median | Max |
|---|---|---|---|
| Bootstrapped ("sufficiently bootstrapped" log line) | 1.51 | 3.09 | 4.27 |
| Descriptor fetched (start of rendezvous setup) | 3.10 | 5.24 | 11.61 |
| Rendezvous complete (HS circuit up) | 4.64 | 6.63 | 14.29 |
| **First byte** | **5.32** | **7.74** | **15.09** |

By segment, across the 5 runs: descriptor fetch 1.21-7.48 s, rendezvous 1.21-2.69 s, stream open and HTTP 0.66-1.11 s.

On the service side (n=1), the descriptor reached 8 of 8 HSDirs 7.0 s after launch.

### 2. Streaming at fixed rates (3 streams of 30 s per rate, each on a fresh circuit)

Inter-chunk gaps and latency are pooled across the three streams. Client was already bootstrapped.

| Rate | Tokens received/emitted | Connect to first byte (s) | Request to first byte (s) | Inter-chunk gap p50/p95/p99 (ms) | Stalls >1 s (per stream) | Token latency p50/p95/p99 (ms) | App bytes/s |
|---|---|---|---|---|---|---|---|
| 30/s | 2,700/2,700 | 4.17, 5.20, **43.25** | 0.31-0.46 | 14 / 113 / 251 | 3 (0, 0, 3) | 274 / 627 / 1,677 | 2,957-3,086 |
| 60/s | 5,400/5,400 | 4.30, 5.25, 6.94 | 0.36-0.62 | 6.2 / 101 / 128 | 1 (0, 0, 1) | 415 / 553 / 863 | 6,129-6,186 |
| 100/s | 9,000/9,000 | 2.57, 3.32, 3.33 | 0.29-0.38 | 4.5 / 66 / 96 | 5 (1, 0, 4) | 264 / 1,006 / 1,808 | 10,316-10,323 |
| Control, no Tor (n=1 per rate) | all | n/a | under 1 ms | 33 / 35 / 35 at 30/s; 10 / 12 / 12 at 100/s | 0 | 0.1 / 0.2 / 0.2 | same |

- Delivery is bursty. Gaps cluster near zero with a tail around 100 ms, while the generator's own spacing is 10-33 ms. I did not find the cause.
- Bytes/s is application payload at the requested rate. It is not a capacity test and excludes Tor overhead.

**Stalls across all 19 Tor streaming runs** (fixed-rate, sustained and timing): 18 stalls over 1 s in 1,322 stream-seconds, affecting 8 runs. The longest was 9.9 s, with the worst token latency at 10.2 s. The generator did not cause these: its largest emitted gap in the 8 jittered runs was 0.69 s, and the fixed-rate runs have no generated pauses.

### 3. Sustained stream (n=2, concurrent, on separate circuits)

| Rate | Duration | Tokens | Survived | Stalls >1 s | Longest | Latency p50/p99/max (ms) |
|---|---|---|---|---|---|---|
| 30/s | 300 s | 9,000/9,000 | yes | 2 | 2.20 s | 429 / 1,202 / 2,496 |
| 60/s | 300 s | 18,000/18,000 | yes | 2 | 2.12 s | 306 / 1,084 / 2,447 |

One stall hit both streams at the same moment (14:14:28), which points to something they shared such as a guard, the local link or the machine. The other two did not coincide.

### 4. Per-request isolation (15 of each, interleaved)

| | Attempts | OK | First byte min / median / mean / p90 / max (s) | Descriptor fetches | New HS circuits |
|---|---|---|---|---|---|
| Fresh isolated circuit | 15 | 13 | 3.18 / **4.33** / 5.91 / 9.79 / 17.95 | 15 | 13 |
| Reuse one circuit | 15 | 15 | 1.29 / **1.37** / 1.42 / 1.63 / 1.73 | 0 | 0 |

- Client logs confirm the mechanism: every fresh request re-fetched the descriptor and built its own circuit, and reuse did neither.
- Each request opened a new stream and made an HTTP round trip, which is why reuse costs over a second on that circuit. Reuse is a sample of one circuit, and other circuits showed quicker round trips (request to first byte 0.29-0.62 s in section 2).
- **Fresh-circuit reliability across all instrumented runs:** 39 of 42 attempts succeeded, with setup p50 3.92 s, p90 7.04 s and max 42.94 s.
- The 3 failures were fast errors after 4.1-11.2 s: two SOCKS "general" replies during the isolation run and one `hs-desc-not-found` later. Arti logs for the isolation failures show "Circuit took too long to build" repeating at 0.5-0.8 s intervals, plus "Could not connect to guard" and "Too many preemptive onion service circuits failed".
- **Hang with no error:** in the reachability check, one attempt was acknowledged at introduction and then got nothing. The service logged "Could not establish circuit to rendezvous point", and the client waited until my 120 s timeout. A PoC needs its own timeout and retry here.
- Hypotheses only, untested: failures may relate to rapid circuit churn, or to Arti's adaptive build timeout being tight.

Fresh circuits are necessary for unlinkability but do not establish it. The same client guard carries every circuit, and this spike did not test linkage by timing or content.

### 5. Timing signal (jittered tokens, 40 s each, 2 repetitions per cell)

Tokens have log-normal gaps (sigma 0.8) with 2% pauses of 100-500 ms added, and the same seeds across modes. Median emitted gap was about 25 ms at 30/s and about 7.6 ms at 100/s.

**This is an application-level proxy.** It records `recv()` events at the client, after the client's Arti has reassembled the stream. I could not capture cells without root. Read sizes in particular are not what a network observer sees, since on the wire tokens of this size should be inside fixed-size cells.

| Mode, rate | Read events per token | Spearman, all gaps | Spearman, gaps <50 ms | Pearson, gaps ≥150 ms | Gaps ≥150 ms recovered | Tokens per 250 ms, best-lag r | Tokens per 1 s, best-lag r |
|---|---|---|---|---|---|---|---|
| Flush, 30/s, run 1† | 0.53 | 0.20 | 0.04 | 0.67 | 68% | 0.23 | 0.37 |
| Flush, 30/s, run 2 | 0.74 | 0.31 | 0.12 | 0.84 | 86% | 0.86 | 0.97 |
| Flush, 100/s, run 1 | 0.56 | 0.17 | 0.11 | 0.83 | 77% | 0.87 | 0.99 |
| Flush, 100/s, run 2 | 0.59 | 0.12 | 0.07 | 0.65 | 64% | 0.92 | 0.99 |
| Batch, 30/s, run 1 | 0.27 | 0.29 | 0.10 | 0.67 | 95% | 0.46 | 0.73 |
| Batch, 30/s, run 2 | 0.27 | 0.27 | 0.09 | 0.73 | 87% | 0.32 | 0.76 |
| Batch, 100/s, run 1 | 0.11 | 0.18 | 0.08 | 0.70 | 95% | 0.46 | 0.85 |
| Batch, 100/s, run 2† | 0.10 | 0.18 | 0.08 | 0.39 | 70% | 0.34 | 0.21 |
| **Control, flush, no Tor** (2 runs, 15 s) | 1.00 | 1.00 | 1.00 | 1.00 | 100% | 1.00 | 1.00 |
| Control, batch, no Tor (2 runs, 15 s) | 0.06-0.15 | 0.25-0.36 | 0.12-0.21 | 0.72-0.80 | 100% | 0.32-0.35 | 0.76-0.85 |

† Run contained stalls (2 stalls up to 2.39 s; 3 stalls up to 9.9 s), which depress the window correlations. "Recovered" means the received gap was at least half the emitted gap. Each run has 1,200 or 4,000 tokens, with 37-75 gaps of 150 ms or more.

Reading this table:
- **Individual token timing does not survive to the client's reads.** Tor removes the fine-grained signal that is perfect in the control. In flush mode 26-47% of consecutive tokens landed in one read, and the median gap error (9-31 ms) is about as large as the gaps themselves.
- **Pauses and rate do survive.** In flush mode without stalls, 1 s rate correlates at 0.97-0.99 and long gaps at 0.65-0.84. Treating Tor as hiding the shape of a response would be wrong.
- **Batching reduces the coarse signal but does not remove it.** Without stalls, 250 ms correlation falls from 0.86-0.92 to 0.32-0.46, and 1 s correlation from 0.97-0.99 to 0.73-0.85. Batched results over Tor look like the no-Tor batch control, so batching itself does the work on fine timing.
- **Batch volume still tracks token count at the source.** Estimated cells per flush correlate with tokens per flush at 0.88-0.99. This is arithmetic from flush bytes assuming about 498 bytes per cell, not a measurement. Client bytes per 250 ms correlated with flush size at only 0.15-0.23, which is too crude to say what an on-wire observer could recover.
- **Not tested:** padding, constant-rate sending, and any observer next to the service.

### 6. Interrupted stream

Latency from the action to each side noticing, on one clock.

| Action | n | Client sees | Generator sees | Tokens emitted after the action |
|---|---|---|---|---|
| Client app closes its socket | 3 | (it closed) | EOF in 0.17-0.27 s | 4-8 at 30/s |
| SIGKILL client Arti (its circuits die) | 3 | EOF in 5-6 ms | EOF in 0.16-0.25 s | 5-7 at 30/s |
| SIGKILL service Arti | 1 | EOF in 0.45 s | EOF in 5 ms | not measured |

All of these end cleanly at TCP or Tor level. A silent network drop with no FIN or RST could not be tested without root and would probably take far longer to notice. For metering, the generator kept producing for about 0.2 s after a disconnect, so billing needs to tolerate some overrun on a cancelled stream.

### 7. Exit reachability (GET, status only, no credentials, default curl user agent)

| Target | No redirects (3 circuits, distinct exit IPs) | Following redirects (3 more circuits) |
|---|---|---|
| https://docs.tinfoil.sh | 308, 308, 308 | 200, **403**, 200 |
| https://stripe.com | 307, 307, 307 | 200, 200, 200 |
| https://inference.tinfoil.sh/.well-known/tinfoil-proxy | 200, 200, 200 | not run |

The docs name `inference.tinfoil.sh` as the model router and API host, and `/.well-known/tinfoil-proxy` as a router endpoint. So 14 of 15 circuits got through and one exit got a 403. These are public pages only. They say nothing about whether Stripe checkout or an authenticated `POST /v1/chat/completions` works over Tor. `github-proxy.tinfoil.sh` and `kds-proxy.tinfoil.sh`, also named in the docs, were not tried.

## Incident: onion service briefly forwarded to another spike's server

**What happened.** My harness hard-coded port 18080 for the generator. The sibling `spikes/attestation` already held it (Python, PID 83525, started 13:45:48), so my generator failed to bind. My harness did not check, and started the onion service anyway. From 13:47:42 until I stopped it by 13:48:35 (at most 53 s), onion port 80 forwarded to that sibling server.

**Effect.**
- My reachability client sent 4 `GET /stream?...` requests to the sibling server (4 circuits in the client log). I did not save what it returned.
- I do not believe anyone else connected, because the address was known only locally and logs redact it. This rests on address secrecy and on v3 descriptors being encrypted to the address. The service log at this level does not count inbound rendezvous, so I cannot prove it from logs.
- PID 83525 was still alive at 13:48:35 and gone by 13:49:54. I sent it no signal, because my kills matched only `spikes/tor` paths. I cannot rule out my requests having some other effect on it.
- **The attestation spike's results may include up to 4 stray requests from this window.**

**Response.** I stopped both Arti instances and deleted that onion identity, which retires the address. The harness now binds an OS-assigned port and refuses to start the service unless its own generator has bound. No measurements come from that window. Evidence is in `raw/incident_port_collision`.

## Limitations

- **Client and server share one machine and network path,** though each uses its own Tor circuit and guards. Both ends leave through the same residential link, so this is not geographic separation.
- **Other spikes ran on this Mac at the same time** (the attestation spike at least), and may have affected CPU and network. Some stalls or failures could come from that, and I cannot separate them.
- **Small samples from one session.** About 45 minutes on one network, with 1-3 runs per cell. Tor circuit quality varies a lot, so treat ranges as indicative, not as bounds.
- **No load test.** At most 2 streams ran at once, so multi-user capacity is unknown.
- **Reliability cause unknown.** Failures are counted, not root-caused.
- **Cold-start bootstrap is a best case** on an uncensored, well-connected network.
- **Causes of burstiness and stalls were not traced** through Arti or the relays.
- **Evidence retention.** Raw logs contain public relay IDs. The onion address is redacted (0 full addresses in `raw/`), and all keys are deleted.

## What this does and does not prove

**Proves, for this setup:** Arti 2.8.0 can carry LLM-rate SSE over a real onion service with complete delivery, roughly 0.3-0.4 s median latency, and occasional multi-second stalls. Per-request isolation works mechanically and has a measured cost.

**Does not prove:** that traffic is unlinkable, that token timing is hidden from a network observer, that this holds under load, or that Arti's service is ready for production.

**How the laptop differs from the target:**

| | This spike | Target |
|---|---|---|
| Service host | Residential Mac, with the client beside it | GPU confidential VM on Verda, in a data centre |
| Payload | Synthetic, precisely timed, about 103 B plain SSE | Real model output, likely with encryption framing, so sizes and token timing will differ |
| Attestation, credits | None | Client-verified attestation and anonymous metering add round trips and round-trip time on top of these numbers |
| Observer position | Client application reads only | Observers at both ends. Near the service, tokens have not crossed Tor yet, so fine timing may well remain. This is inference, not measured |
| Concurrency | 1-2 streams | Many users sharing one onion service |

## Cleanup

All processes I started are stopped (0 remaining under the spike path). Onion keys and Tor state are deleted (`state/` removed), and the build directory is removed. No Arti directories appear in the real home directory, and `~/.cargo` shows no changes from today. The 17 key-like files left under `downloads/crate` are Arti's public test fixtures from the crate tarball, not keys from this run.

## Claims

Evidence base: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw`

| Claim | Status | Evidence file |
|---|---|---|
| Arti 2.8.0, built without the `experimental` feature, hosts an onion service and streams SSE to a separate client on macOS | confirmed by test | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis.json |
| Upstream regards Arti's onion service as production-ready | inconclusive (its documents conflict) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/provenance/OnionService.md |
| Fresh client reaches first byte in under 20 s | confirmed by test (5/5, 5.3-15.1 s) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/cold.json |
| 30, 60 and 100 tokens/s are delivered completely at rate | confirmed by test (17,100/17,100) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/pooled_streams.json |
| Streaming runs free of stalls over 1 s | refuted by test (18 stalls, 8 of 19 runs, longest 9.9 s) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis_tables.txt |
| A 5-minute stream survives | confirmed by test (2/2) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis.json |
| Fresh isolated circuits cost materially more first-byte time than reuse | confirmed by test (median 4.33 s vs 1.37 s) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/iso.json |
| SOCKS-username isolation builds separate circuits; same-username requests reuse one | confirmed by test (client logs) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/iso.json |
| Fresh circuits succeed reliably without retries | refuted by test (3/42 failed, plus one 120 s hang) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/connect_attempts.jsonl |
| Fresh circuits make requests unlinkable to the service or a network observer | inconclusive (not tested) | none |
| Token-scale gaps (<50 ms) survive to client read events in flush mode | refuted by test (Spearman 0.04-0.12; control 1.00) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis.json |
| Pauses and overall rate survive to client read events | confirmed by test (long gaps r 0.65-0.84; 1 s rate r 0.97-0.99 in 3 of 4 flush runs) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis.json |
| 250 ms batching eliminates the timing signal | refuted by test (reduced, not eliminated) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/analysis.json |
| What a network observer sees at cell level, on either side | inconclusive (no capture without root) | none |
| Both sides notice a clean disconnect or process kill within about 0.5 s | confirmed by test (n=7) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/interrupt.json |
| Silent network loss is noticed quickly | inconclusive (not tested) | none |
| Tor exits reach docs.tinfoil.sh, stripe.com and inference.tinfoil.sh at status level | confirmed by test (14/15; one docs 403) | /private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/tor/raw/results/exit_follow.json |
| Stripe checkout or authenticated Tinfoil inference works over Tor | inconclusive (not tested, by design) | none |
| These numbers transfer to a Verda confidential VM under load | inconclusive (not tested) | none |
