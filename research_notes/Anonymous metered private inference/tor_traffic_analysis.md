# Tor as the anonymous transport for streaming LLM responses, including traffic analysis of token streams

Research date 2026-11-06. Source-only: no Tor client, relay or third-party code was run. "Cited Findings" = verified from the linked source (a few are marked "via search snippet" where the page itself could not be read). "Inferences" = my reasoning. "Gaps" = unknown. Nothing here has been tested on the live Tor network.

Versions relied on: C tor 0.4.9.16 (2026-11-04), Arti 2.8.0 (2026-11-03), arti-client and tor-hsservice 0.48.0 (docs.rs, 2026-11-04), RFC 9458, draft-ietf-ohai-chunked-ohttp-08 (2026-02-18), Tor Metrics data queried for 2026-08-08 to 2026-11-06.

Access limits: gitlab.torproject.org returned 403 to both WebFetch and curl, so the Arti CHANGELOG, Arti `doc/OnionService.md`, torspec source and GitLab issues could not be read directly. I used docs.rs, spec.torproject.org, the Tor blog and forum, and search snippets instead.

Calculation scripts (mine, stdlib only) are in `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/`: `metrics_summary.py`, `token_stream_arithmetic.py`, `shaping_and_correlation.py`. Downloaded CSVs are in `tor_metrics_data/` beside them.

## 1. Is token-stream bandwidth plausibly OK, and how reliable are long-lived streams? Conflux status

### Takeaway
Bandwidth is not the constraint: even flush-per-token at 100 tokens/s (411 kbit/s) is about 8-14% of the median onion-service throughput. The real unknowns are cold-start time per request, jitter, and mid-stream circuit death, which no public metric measures. Conflux does not apply to onion services and does not make streams survive failures, so the application must tolerate a dead stream.

### Cited Findings

**Tor Metrics OnionPerf, median of daily values.** Latency is request sent to response header on an established circuit. Throughput is measured between 4 and 5 MiB of a 5 MiB download. Definitions are from [Tor Metrics reproducible metrics](https://metrics.torproject.org/reproducible-metrics.html). Data: [latencies public](https://metrics.torproject.org/onionperf-latencies.csv?start=2026-08-08&end=2026-11-06&server=public), [latencies onion](https://metrics.torproject.org/onionperf-latencies.csv?start=2026-08-08&end=2026-11-06&server=onion), [throughput public](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=public), [throughput onion](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion). Rows below use the three long-running vantage points (`op-de8a`, `op-us8a`, `op-hk8a`, about 44 days each, 2026-08-08 to 2026-09-21).

| Metric | Server | DE | US | HK |
|---|---|---|---|---|
| Latency median (ms) | public | 110 | 270 | 520 |
| Latency median (ms) | onion | 270 | 425 | 670 |
| Latency q3 (ms) | onion | 370 | 522 | 780 |
| Latency high whisker (ms) | onion | 610 | 755 | 1040 |
| Throughput median (kbit/s) | public | 11,816 | 8,559 | 5,518 |
| Throughput median (kbit/s) | onion | 5,309 | 4,279 | 3,017 |
| Throughput q1 (kbit/s) | onion | 3,061 | 2,890 | 2,231 |
| Throughput low whisker (kbit/s) | onion | 190 | 182 | 216 |
| 50 KiB download median (s) | onion | 1.17 | 1.74 | 2.78 |
| 5 MiB download median (s) | onion | 9.99 | 12.2 | 16.9 |

- Coverage caveat: the three `op-*8a` sources stop on 2026-09-20/21. Replacement sources `op-nl9a`, `op-us9a`, `op-hk9a` start 2026-10-28, so there is a five-week data gap and the before/after comparison is confounded — [throughput onion CSV](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion).
- Newer sources (5-7 days) read somewhat worse: onion median 2.4-3.9 Mbit/s and latency 300-760 ms — [same CSV](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion).
- Timeouts, mean of daily rates: onion 1.7-2.4% on the `op-*8a` sources (1.3-2.5% on the newer ones); public 0.55-0.68% for US and HK. Failures (short responses) were 0. One `op-de8a` public outage around 2026-09-19 skews DE and is excluded — [failures onion](https://metrics.torproject.org/torperf-failures.csv?start=2026-08-08&end=2026-11-06&server=onion), [failures public](https://metrics.torproject.org/torperf-failures.csv?start=2026-08-08&end=2026-11-06&server=public). A timeout is "a download does not complete within the scheduled time" — [Tor Metrics failures](https://metrics.torproject.org/torperf-failures.html).
- Circuit build time per hop, median: 110, 150, 190 ms, which sums to 450 ms for three hops — [build times CSV](https://metrics.torproject.org/onionperf-buildtimes.csv?start=2026-08-08&end=2026-11-06).

**Cells and flushing.**
- A fixed cell is CircID + Command + Body; the usual values are 4 + 1 + 509 = 514 bytes — [tor-spec cell format](https://spec.torproject.org/tor-spec/cell-packet-format.html).
- The relay header is 11 bytes, so a legacy RELAY_DATA cell carries up to 498 bytes (509 - 11); the rest is padding — [tor-spec relay cells](https://spec.torproject.org/tor-spec/relay-cells.html).
- Under CGO the layout gives 488 bytes by my arithmetic (509 - 16 tag - 3 header - 2 stream ID); the proposal does not state the number and has status "Open" — [prop 359](https://spec.torproject.org/proposals/359-cgo-redux.html). This does not change any result below.
- C tor packages partial cells when asked: `package_length = n_available; /* just take whatever's available now */`, otherwise `return 0; /* nothing to do until we have a full payload */` — [relay.c](https://tpo.pages.torproject.net/core/doc/tor/relay_8c_source.html). The call sites I could read pass `package_partial = 1`; I did not read the edge-read call site.
- Arti's `poll_write` only queues, sends when the cell is full, and `poll_flush` sends whatever is pending — [tor-proto data.rs](https://docs.rs/tor-proto/latest/src/tor_proto/client/stream/data.rs.html). "DataStream buffers data internally, in order to write as few cells as possible" — [arti-client DataStream](https://docs.rs/arti-client/latest/arti_client/struct.DataStream.html).
- Congestion control defaults: `cc_sendme_inc` 31, `cc_cwnd_init` 4*31 — [prop 324](https://spec.torproject.org/proposals/324-rtt-congestion-control.html). Onion services advertise CC in the descriptor and clients request it in the INTRODUCE cell — [same](https://spec.torproject.org/proposals/324-rtt-congestion-control.html).
- KIST scheduler tick falls back to 10 ms by default — [torrc manual](https://manpages.debian.org/testing/tor/torrc.5.en.html).

**Conflux.**
- The design covers onion services ("only two rendezvous circuits are linked") and is optional about recovery: "they MAY be resumed", full resumption is not implemented in C tor, if the current sending leg fails the whole set closes, and if data was in flight on a closed leg all legs must close — [prop 329](https://spec.torproject.org/proposals/329-traffic-splitting.html).
- `ConfluxEnabled` applies to "general purpose traffic" and defaults to consensus-driven `auto` — [torrc manual](https://manpages.debian.org/testing/tor/torrc.5.en.html).
- The current onion descriptor spec has no conflux line — [rend-spec hsdesc](https://spec.torproject.org/rend-spec/hsdesc-encrypt.html). The tor ticket deferred onion pieces (via search snippet) — [tor#40716](https://gitlab.torproject.org/tpo/core/tor/-/issues/40716). A forum post citing the spec says conflux is only used for non-onion services (undated, secondary) — [Whonix forum](https://forums.whonix.org/t/tor-version-0-4-7-16-in-whonix-17-is-now-considered-obsolete-any-action-required/18320?page=2).
- Measured signature: conflux-labelled sources gain on public servers (DE 11.8 to 15.5, US 8.6 to 10.9, HK 5.5 to 7.1 Mbit/s) and show no gain on onion servers (DE 5.31 to 5.41, US 4.28 to 3.98, HK 3.02 to 2.94 Mbit/s) — [public](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=public), [onion](https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion). The methodology page does not define these source labels, so reading them as conflux-enabled is my interpretation.
- Conflux-labelled public sources did not time out less: US 1.15% vs 0.55%, HK 1.55% vs 0.68% — [failures public](https://metrics.torproject.org/torperf-failures.csv?start=2026-08-08&end=2026-11-06&server=public).
- Conflux has been a security surface in 2026: fixes appear in 0.4.9.6 and 0.4.9.10 (via search snippet) — [0.4.9.x announcements](https://forum.torproject.org/c/news/tor-release-announcement/28).

**Current network incident (affects any near-term measurement).**
- Tor 0.4.9.15 (2026-10-28) added three DoS defences for "the thunderstorm of directory requests the network is currently seeing" — [0.4.9.15](https://forum.torproject.org/t/security-release-0-4-9-15/22332).
- Tor 0.4.9.16 (2026-11-04) is "an emergency release" for directory authorities, describing a thundering herd of outdated clients in restart loops — [0.4.9.16](https://forum.torproject.org/t/security-release-0-4-9-16/22372).
- A relay operator reported a BEGIN_DIR/HSDir flood on 2026-11-01 to 11-03, with circuits rising from about 7k to about 1.5M and hourly OOM restarts — [forum](https://forum.torproject.org/t/tor-0-4-9-15-begin-dir-hsdir-flood-oom-kill-despite-maxmeminqueues/22359). A Tor contributor said the attack targets relays with the HSDir flag "as far as we know" — [forum](https://forum.torproject.org/t/tor-relays-re-does-the-begin-dir-attack-affect-user-count-metrics/22366).
- Estimated direct users jumped from about 3.0M/day (2026-10-23 to 27) to 23-64M/day (2026-10-28 to 11-03) — [users CSV](https://metrics.torproject.org/userstats-relay-country.csv?start=2026-08-08&end=2026-11-06&country=all&events=off). The same contributor attributes this "very likely" to blocking pre-0.4.9.x clients, not the flood — [forum](https://forum.torproject.org/t/tor-relays-re-does-the-begin-dir-attack-affect-user-count-metrics/22366). Another thread notes bandwidth stayed flat, pointing to measurement artefact, and root cause is not stated — [forum](https://forum.torproject.org/t/sudden-direct-connection-spike/22353).

### Inferences

**Arithmetic** (assumes an OpenAI-style SSE chunk of about 180 bytes plus 22 bytes of TLS overhead; `token_stream_arithmetic.py`). Any single write up to about 476 bytes still costs one cell, so 1 cell per token holds across chunk sizes.

| Tokens/s | Flush per token: cells/s | Wire kbit/s | 250 ms batch, SSE: cells/tick, cells/s | Wire kbit/s | 250 ms, compact 6 B/token: cells/s | Wire kbit/s |
|---|---|---|---|---|---|---|
| 30 | 30 | 123 | 3, 12 | 49 | 4 | 16.4 |
| 60 | 60 | 247 | 6, 24 | 99 | 4 | 16.4 |
| 100 | 100 | 411 | 10, 40 | 164 | 4 | 16.4 |

- The 6 B/token figure is my assumption (about 4 characters of text plus 2 bytes of framing). One cell per 250 ms then carries about 325-332 tokens/s. Larger scripts and token sizes cost more.
- Overhead versus plain SSE bytes is about 2.9x for flush-per-token, because each 180-byte chunk occupies a 514-byte cell. A 1000-token response costs 514 kB flush-per-token versus 206 kB at 250 ms. SENDME adds roughly 1 upstream cell per 31 downstream cells.
- Against the onion figures: 411 kbit/s is 8-14% of median and 13-19% of q1. It exceeds the low whisker (about 190 kbit/s), which here is effectively each day's slowest transfer, because q1 - 1.5 IQR falls below zero. Batching at 250 ms sits below even that. So a fast small model (which could exceed 100 tokens/s) should batch; bandwidth still will not be the limit.
- Flow control is not a bottleneck: a legacy stream window of 500 cells gives about 3.0 Mbit/s at 0.67 s RTT, and an initial cwnd of 124 cells sustains 100 cells/s up to 1.24 s RTT.
- Flush-per-token is an upper bound on cells, since bytes that arrive before the edge read may coalesce. Multiple cells may also share a TLS record, but byte counts remain in cell multiples.
- Exit paths have 0.4-0.8x the latency and about twice the throughput of onion paths, plus conflux, but they re-expose TLS record sizes beyond the exit (see section 4).
- Reliability: OnionPerf's 1.7-2.4% onion timeouts are for transfers that are short relative to a 3-5 minute stream, so failures per stream cannot be derived from them. Plan for failures that are not rare.
- Cross-track (metering): a stream can die before the change credential is delivered. Settlement must not depend on the client's connection surviving, and the client needs an idempotent way to collect change on a new, isolated circuit.
- Tor carries TCP only in deployed form, so HTTP/3 is out. SSE or a chunked HTTP/1.1 body over a TCP stream is fine.
- C tor's two-week security cadence, with several releases labelled high severity, means Tor inside a measured CVM image produces a new measurement about every two weeks. That churn lands on the client's allow-list and the transparency log.

### Gaps
- No public metric measures inter-chunk latency, stalls, or death rate for multi-minute streams. See Proposed validation.
- OnionPerf does not publish rendezvous time, and I could not establish whether its onion timings include rendezvous; the methodology page is silent.
- Whether the Oct-Nov 2026 incident degraded onion descriptor fetches and introductions for clients is unknown. The operator post reports only relay-side effects.

## 2. Software: Arti vs C tor, PoW, vanguards, single-onion mode, and browser clients

### Takeaway
Use C tor for the service side. Arti's own 2.8.0 config still warns that onion-service privacy features are missing. Arti is reasonable as an embedded client in a desktop PoC, with caveats. Browsers cannot reach this without Tor Browser, a local proxy, or an unaudited WASM prototype, so the PoC is effectively forced to a native client. Keep the default three-hop service mode rather than single-onion mode.

### Cited Findings

**Arti.**
- Current release 2.8.0 (2026-11-03), with low-severity security fixes and a recommendation that users "especially those running onion services" upgrade — [Arti 2.8.0](https://blog.torproject.org/arti_2_8_0_released/). 2.7.0 (2026-10-01) fixed "low- to high-severity security issues" — [Arti 2.7.0](https://blog.torproject.org/arti_2_7_0_released/).
- The 2.8.0 example config says: "NOTE: Some of the security features needed for onion service privacy are not yet implemented." It has no conflux or single-onion section, and `enable_pow = false` — [arti-example-config 2.8.0](https://docs.rs/crate/arti/latest/source/src/arti-example-config.toml).
- Conflict: Arti's `doc/OnionService.md` says there are "a *ton* of missing security features", that you should "not expect privacy (yet)", and lists vanguards, DoS defences and setup padding as missing (via search snippet; status dated Feb 2024 with later revisions) — [OnionService.md](https://gitlab.torproject.org/tpo/core/arti/-/raw/main/doc/OnionService.md). The 2.8.0 config contradicts part of this, showing `[vanguards]` with mode auto = lite, plus a PoW option — [config](https://docs.rs/crate/arti/latest/source/src/arti-example-config.toml). The guide is partly stale, but the config's own NOTE is current.
- FAQ, "as of January 2026": ready "as a proxy or by developers interested in embedding Tor support in their Rust projects", "supports accessing and hosting onion services", "does not have all of the functionality of C tor", and replacing C tor will take "several more years, at the least" (page updated 2026-07-23) — [Arti FAQ](https://arti.torproject.org/FAQs).
- `hs-pow-full` is experimental; `vanguards` is a default feature; `onion-service-service` is a separate feature; no conflux or cgo feature flags are listed — [arti features](https://docs.rs/crate/arti/latest/features). `hs-pow-full` links LGPL dependencies — [tor-hsservice](https://docs.rs/tor-hsservice/latest/tor_hsservice/).
- Congestion control is on in default builds from 2.5.0 (2026-06-30) and always on in `arti` from 2.6.0 (2026-09-01) — [2.5.0](https://blog.torproject.org/arti_2_5_0_released/), [2.6.0](https://blog.torproject.org/arti_2_6_0_released/). As of 2.5.1 (2026-08-04), CC and CGO on onion service circuits were still "experimental", with stabilisation hoped "soon" — [2.5.1](https://blog.torproject.org/arti_2_5_1_released/).
- Embedding API: `TorClient::launch_onion_service`, `launch_onion_service_with_hsid` (may persist the key depending on keystore), `isolated_client` — [TorClient docs](https://docs.rs/arti-client/latest/arti_client/struct.TorClient.html). The crate is pre-1.0 at 0.48.0 — [arti-client](https://docs.rs/arti-client/latest/arti_client/isolation/index.html).
- Conflict in secondary sources: a search summary said 2.0.0 added experimental PoW, but the 2.0.0 post itself does not mention PoW — [2.0.0](https://blog.torproject.org/arti_2_0_0_released/). I do not rely on that claim. One third-party blog calling Arti "production-ready" conflicts with the official FAQ and config and is disregarded.

**C tor.**
- 0.4.9.5 was the first 0.4.9 stable (2026-02-12) with CGO and Happy Families (via search snippet) — [0.4.9.5](https://forum.torproject.org/t/stable-release-0-4-9-5/21227). 0.4.9.16 is latest (2026-11-04) — [release list](https://forum.torproject.org/c/news/tor-release-announcement/28).
- Moved to "a two-week cadence" with 0.4.9.12 (2026-09-08), which addressed "a series of high-severity security issues" — [0.4.9.12](https://forum.torproject.org/t/security-release-0-4-9-12/22096). 0.4.9.13 (2026-09-23) did too, "affecting clients, onion services and relays alike" per the post's summary — [0.4.9.13](https://forum.torproject.org/t/security-release-0-4-9-13/22178).

**PoW and DoS.**
- `HiddenServicePoWDefensesEnabled` (default 0) prioritises rendezvous requests by client effort; `HiddenServicePoWQueueRate` default 250/s; `HiddenServiceEnableIntroDoSDefense` (default 0) rate-limits at the introduction point, default 25/s — [torrc manual](https://manpages.debian.org/testing/tor/torrc.5.en.html).
- The motivation: "building rendezvous circuits is a costly procedure both for the service and for the network" — [hspow-spec](https://spec.torproject.org/hspow-spec/motivation.html).

**Vanguards.**
- Vanguards-lite shipped in Tor 0.4.7 and addresses guard discovery; the Tor Project said an attacker who obtains a guard can use "netflow connection times" to find a user — [Tor blog, 2024-09-18](https://blog.torproject.org/tor-is-still-safe/).
- The original vanguards add-on is unmaintained and conflicts with conflux (forum, via search snippet) — [Tor forum](https://forum.torproject.org/t/should-i-use-vanguards-for-onion-services/21200).

**Single-onion mode.**
- `HiddenServiceSingleHopMode` is experimental and non-anonymous and requires `HiddenServiceNonAnonymousMode` — [torrc manual](https://manpages.debian.org/testing/tor/torrc.5.en.html).
- "fewer hops (4 rather than 6)", "not location hidden", "a security analysis of this configuration has not yet been performed", one-hop circuits "may allow intro and rendezvous points to block the service", and four-hop paths "may be statistically distinguishable" — [prop 260](https://spec.torproject.org/proposals/260-rend-single-onion.html).

**Browser.**
- Browsers cannot open arbitrary TCP connections, which is why Snowflake (WebRTC, WebSocket) fits (via search snippet) — [Tor forum](https://forum.torproject.org/t/tor-project-snowflake-relays/21506).
- Arti in WASM (Ethereum Foundation, 2026-03-20) is "a functional prototype, not a hardened production release". Its fast bootstrap server sees the user's IP, there is no process isolation from the page, RustCrypto primitives are less audited, reduced timer precision may affect congestion control and padding, onion-service support is not addressed, and the upstream merge is "in progress" — [EF post](https://reads.ethereum.foundation/blog/embedding-arti-in-the-browser/).
- `webtor-rs` lists Snowflake and WebTunnel transports and states no third-party audit (project README mirror, via search snippet) — [webtor-rs](https://skillsmp.com/creators/ethereum/webtor-rs).
- Tinfoil uses EHBP where TLS pinning is unavailable, such as in browsers — [Tinfoil attestation architecture](https://docs.tinfoil.sh/verification/attestation-architecture.md).

### Inferences
- **Recommended split for the PoC:** C tor 0.4.9.x on the service; `arti-client` for an embedded desktop client, or a C tor sidecar with SOCKS isolation if Arti's onion-client gaps matter. Keep the transport behind an interface so OHTTP or a direct exit path can be swapped in.
- **Arti client caveats:** client-side setup padding is unconfirmed and PoW is behind an experimental feature, so an Arti client may be easier to spot at the guard and may lose priority under a PoW-defended flood.
- **PoW matters more than usual here.** Credentials are checked only after rendezvous and the inner handshake, and IP-based limits are unavailable. Tor-level PoW and intro-point rate limits are the only pre-authentication filters. Flood cost falls on the CVM's CPU rather than the GPU. Restricted discovery (client auth) is unsuitable because per-client keys would be linkable.
- **Vanguards-lite protects clients too.** Per-request isolation means many fresh circuits per user, which is the raw material for guard discovery by an operator who also runs relays. Clients must keep vanguards-lite on (C tor default; Arti `auto` = lite).
- **Single-onion looks attractive but I would not use it.** Location hiding is not needed, and it cuts hops. However, the hosting operator would then see the CVM open a direct, unmixed connection to the client-chosen rendezvous point per request. That hands the operator one clean per-request trace plus the RP identity, and places an operator-RP collusion two hops from the client's guard. At low concurrency the multiplexing benefit of service-side guards is small anyway, so measure before deciding. Prop 260 has no security analysis to lean on.
- **A web client served from the onion service undermines the guarantee.** The server could change the JavaScript that does attestation, so the operator can subvert client-side verification. That property requires a native client or a pinned distribution. Native also suits credential storage and separating the clearnet purchase from the Tor path.

### Gaps
- Exact current Arti feature gaps could not be read, because the CHANGELOG and live guide are behind the GitLab block. Whether circuit padding machines exist in Arti 2.8.0 is unconfirmed; its config mentions only channel padding.
- Whether onion-circuit CC and CGO became stable between 2.5.1 and 2.8.0 is unknown.
- I did not verify Tor Browser's JavaScript and WASM behaviour at each security level.

## 3. Layering: attested channel inside an onion service, where Tor terminates, binding, key handling, per-request isolation

### Takeaway
Treat Tor as an untrusted pipe, with the attested TLS or HPKE channel inside it as the only source of content security. Terminate Tor inside the CVM for your own hosting, because terminating outside hands the operator per-circuit stream boundaries and record sizes. Per-request isolation is simple to obtain, but it adds low seconds of cold-start time. No measured current figure exists, and it is essential because a stable circuit relinks what anonymous credits unlink.

### Cited Findings
- Onion services are self-authenticating: the address encodes the Ed25519 public key (tor-dev thread, library docs, via search snippets) — [tor-dev](https://lists.torproject.org/mailman3/hyperkitty/list/tor-dev@lists.torproject.org/thread/4VKTJM2KSNXTP65TZ6DZMWGLPH4FR5GZ/), [tor-client-lib](https://docs.rs/tor-client-lib/0.3.2/src/tor_client_lib/key.rs.html).
- Prior art for binding: a Phala/dstack TDX template uses a hash of the .onion address as quote report data and persists the key in a `tor_data` volume (via search snippet; the page body would not render) — [Phala template](https://cloud.phala.com/templates/tor-hidden-service). VaulTor notes REPORTDATA routinely carries a public key whose private key is held in the enclave (via search snippet) — [VaulTor](https://arxiv.org/pdf/2412.16064). I found no standard for this.
- C tor key handling: `ADD_ONION` supports `NEW:BEST` (ED25519-V3), `DiscardPK`, `Detach`, `NonAnonymous` and PoW parameters. The service lasts until `DEL_ONION`, Tor exit, or control connection close, and the spec describes no write to disk — [control-spec](https://spec.torproject.org/control-spec/commands.html).
- Arti: `launch_onion_service_with_hsid` takes a provided keypair, with a warning it "may get persisted to disk" depending on keystore — [TorClient docs](https://docs.rs/arti-client/latest/arti_client/struct.TorClient.html).
- Tinfoil terminates TLS inside the CVM with a non-exportable key, and intermediaries forward TCP. EHBP encrypts bodies to an attested HPKE key. Attestation material goes through untrusted proxies — [Tinfoil](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- Meta fetches HPKE keys from a third-party CDN — [Meta engineering, 2025-04-29](https://engineering.fb.com/2025/04/29/security/whatsapp-private-processing-ai-tools/). RFC 9458 warns that "each active configuration partitions the anonymity set" — [RFC 9458 §7](https://www.rfc-editor.org/rfc/rfc9458.html).

**Isolation mechanisms.**
- C tor: `IsolateSOCKSAuth`, "Don't share circuits with streams for which different SOCKS authentication was provided. On by default". `MaxCircuitDirtiness` defaults to 10 minutes — [torrc manual](https://manpages.debian.org/testing/tor/torrc.5.en.html).
- Arti: `StreamPrefs::new_isolation_group()` creates a fresh `IsolationToken`. `isolate_every_stream()` "is likely to have poor performance, and imposes a much greater load on the Tor network" — [StreamPrefs](https://docs.rs/arti-client/latest/arti_client/struct.StreamPrefs.html). `isolated_client()` handles "will never share circuits with one another" — [TorClient](https://docs.rs/arti-client/latest/arti_client/struct.TorClient.html). The proxy honours an isolation string in the proxy handshake — [config](https://docs.rs/crate/arti/latest/source/src/arti-example-config.toml).

**Latency evidence is thin.**
- Three-hop build is about 450 ms (general circuits, section 1) — [build times](https://metrics.torproject.org/onionperf-buildtimes.csv?start=2026-08-08&end=2026-11-06).
- A 2020 paper measured onion connections via Orbot at 1.38 s median, 2.93 s p95, version unstated (via search snippet) — [Swirlwave](https://arxiv.org/pdf/2001.05276).
- A 2019 commit says the intro circuit cutoff is "around 2-3 seconds"; that is a timeout, not a measurement (via search snippet) — [tor commit mirror](https://archives.seul.org/or/cvs/Dec-2019/msg00922.html). I found no current v3 measurement.

### Inferences

**Where Tor terminates:**

| Option | Operator sees | TCB | Fits |
|---|---|---|---|
| Inside the CVM | Only link TLS to relays: cell multiples and timing | Adds tor to the measured image; a tor bug is inside the trust boundary, so run it as a separate process from the inference server | Self-hosted on Verda |
| Outside (host or sidecar) forwarding to CVM | Per-stream boundaries and inner TLS record sizes; holds the onion key | Smaller | Fallback only; bring back size channels unless the inner layer pads |
| Exit path, no onion | Tinfoil's operator sees exit IP and TLS record sizes | None on the backend | Tinfoil today |

- **Binding:** the inner attestation report commits to the inner TLS or HPKE key, and that is what protects content. Additionally committing to the onion public key lets clients verify the claim "Tor terminates inside the CVM". Without it, a hostile operator could front the service with its own onion service and log per-circuit stream boundaries, with content still safe.
- **Key lifecycle:** generating the key at boot inside the CVM and never exporting it is simplest, but the address then changes on each reboot and needs discovery. Keeping a stable address needs sealed storage or attestation-gated key release, which is an additional system. For a PoC, per-boot keys plus a published list seems enough.
- **Discovery is a targeting channel.** A per-client onion address or key would identify the user, as would per-client attestation responses. Clients need a single consistent view (transparency log or independent mirror), the same problem Meta addresses with its third-party CDN.
- **Fetch attestation over the same isolated Tor path, or in-band.** Fetching certificate material over the clearnet leaks the client's IP and the time of use.
- **Per-request isolation cost (estimate):** cold rendezvous about 1.5-4 s (intro extension, RP establishment, service-side 3-hop plus RP, BEGIN/CONNECTED), then inner TLS 1 RTT plus request 1 RTT at 0.3-0.7 s each. Total of about 2-6 s before model time. On an established circuit it is about 0.3-0.7 s before model time. This is a calculation, not a measurement.
- **Why isolation is non-negotiable:** the default reuse window links requests on one rendezvous circuit for 10 minutes, regardless of how unlinkable the credit scheme is. TLS session tickets, HTTP/2 connection reuse and cookies must also be per-request (RFC 9458 §6.1 notes the same linkage through content).
- **Pre-warming hides cold start but has a trap:** if the next circuit is built right when the previous response ends, the timing itself links the two at the service. Trigger on typing or use a random delay, and discard unused circuits.
- **What is still linkable:** re-sending chat history makes upload size grow turn by turn, unless prompts are padded to buckets. Spending change immediately on the next circuit creates a timing link (cross-track with the credit design).

### Gaps
- No measured current distribution of v3 rendezvous time was found.
- Whether attestation evidence retrieval from NVIDIA and AMD can be served entirely in-band for the Verda instances is a question for the TEE track.
- The dstack template's client verification flow could not be read.

## 4. Exits: do Cloudflare, Stripe or cloud WAFs block Tor in a way that rules out a clearnet backend?

### Takeaway
I found no published evidence that rules out reaching Tinfoil or Stripe through exits, but the evidence that exists is old, general-web, and proves nothing specific. Tinfoil's inference hostname does not look CDN-fronted, while two of its attestation proxies are behind Cloudflare, which is the likelier failure. Stripe is moot, because purchase is deliberately not over Tor. This must be tested.

### Cited Findings
- Cloudflare assigns Tor the pseudo-country `T1`, and customers can allow, challenge or block on it (docs mirror, via search snippet; the official page was not retrieved) — [Cloudflare docs mirror](https://fork-preview1-badc8fb6.pages.dev/support/firewall/learn-more/understanding-cloudflare-tor-support-and-onion-routing).
- Onion Routing lets Tor users skip exits, and "interactive challenges are only served to malicious bot traffic" when enabled; the page does not address API traffic — [Cloudflare Onion Routing](https://developers.cloudflare.com/network/onion-routing/).
- At least 3.67% of Alexa top-1000 sites blocked Tor at the application layer, with at least 1.3M IPs blocking at the network layer. CloudFlare, AWS and Akamai were identified as the dominant blockers (2016, via search snippet) — [Khattak et al. summary](https://www.lightbluetouchpaper.org/?p=8018), [paper](https://murdoch.is/papers/ndss16doyousee.pdf).
- 20.03% of Alexa top-500 front-page loads showed discrimination against exits, 24.56% for login. Those figures are from an auto-extracted summary that I did not check against the paper — [Lantern summary](https://corpus.lantern.io/findings/2017-singh-characterizing__tor-exit-discrimination-login-search-rate/), [Singh et al. 2017](https://faculty.cc.gatech.edu/~pearce/papers/tor_blocking_usenix_2017.pdf).
- Stripe Radar has `:is_anonymous_ip:`, "a known proxy or Tor exit node", updated daily, usable in merchant rules — [Stripe attributes](https://docs.stripe.com/radar/rules/supported-attributes.md), [rules reference](https://docs.stripe.com/radar/rules/reference). I found nothing saying Stripe blocks Tor by default or that its API refuses exits.
- Tinfoil: the router is at `inference.tinfoil.sh`, and TLS terminates inside the enclave. Attestation material flows through `github-proxy.tinfoil.sh` and `kds-proxy.tinfoil.sh`, and "none of these proxies is trusted". Model enclave hostnames "restrict inbound connections" — [Tinfoil](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- My own check, clearnet only, 2026-11-06, one request each (`dig`, `curl -I`), not through Tor:
  - `inference.tinfoil.sh` resolves via CNAMEs to 69.46.85.219, returned 307 with a `tinfoil-pt` header and no Cloudflare headers.
  - `kds-proxy.tinfoil.sh` and `github-proxy.tinfoil.sh` resolve to 104.18.16.40 and 104.18.17.40 with `server: cloudflare` and `cf-ray`.
  - `atc.tinfoil.sh` returned `via: 1.1 Caddy`. `docs.tinfoil.sh` is on Cloudflare and `tinfoil.sh` on Vercel.

### Inferences
- A non-browser SDK cannot solve an interactive challenge. So if Cloudflare challenges exit IPs on the attestation proxies, attestation fails even though inference would be reachable. The fix is pre-fetching, caching, or having a gateway serve the material. This is the plausible blocker, unconfirmed.
- A TLS-terminating CDN is structurally excluded on the attested inference path (TLS ends in the enclave), which suggests WAF-style filtering there is less likely, though IP-level rules are possible.
- **Exit paths give back the size channel.** Fixed cells protect only client to exit. From the exit to the backend, ordinary TLS records are visible to the exit, its ISP, and the backend's network. Padding must therefore be inside TLS or EHBP. The exit also learns the hostname.
- A Tinfoil API key would link requests regardless of Tor, so exit mode needs a pooled-account gateway or Tinfoil-side support (cross-track).
- Exit paths are the faster route. They gain conflux, and public medians are about twice the onion throughput. For a clearnet backend this is the natural PoC transport.

### Gaps
- No post-2017 measurement of exit blocking on API endpoints was found, and none specific to Stripe, Tinfoil, or Sigstore and AMD KDS.
- Cloudflare's current default treatment of `T1` on a customer zone without custom rules was not confirmed from a current official page.

## 5. Tor vs OHTTP, and does Tor give non-targetability when the fleet is one or a few machines?

### Takeaway
Tor gives network-origin privacy without a contracted relay, and it hides the destination from the local network, but at the price of seconds of setup and more variance. OHTTP is low-overhead, per-request and stateless, but needs an independent relay and its streaming form is not yet an RFC. Tor does not provide non-targetability in the Apple or Meta sense on a small fleet. At N=1 the property is vacuous, since everyone's requests are on the one machine an attacker needs to compromise.

### Cited Findings

**OHTTP.**
- "the Oblivious Relay Resource cannot be operated by the same entity as the Oblivious Gateway Resource"; the relay sees who but "does not know the decrypted contents"; gateways "cannot link requests from the same Client in the absence of unique per-Client keys"; OHTTP "removes linkage at the transport layer, which is only useful for an application that does not carry state between requests"; "This encapsulation does not permit progressive processing of responses"; HTTPS "does not prevent traffic analysis", and padding is optional; "Each request requires at least two regular HTTP requests" — [RFC 9458](https://www.rfc-editor.org/rfc/rfc9458.html).
- Chunked OHTTP draft-08 (2026-02-18): state "RFC Ed Queue", no RFC number, "blocked: Reference Not Received". Privacy section covers no forward secrecy between chunks, truncation handling, and no fallback to non-chunked because inconsistency partitions anonymity sets — [datatracker](https://datatracker.ietf.org/doc/draft-ietf-ohai-chunked-ohttp/).

**Non-targetability.**
- Apple's "target diffusion": limited metadata, single-use RSA blind-signature credentials, third-party OHTTP relay, load balancer picks a node subset without user information and "cannot bias the set for targeted users"; a compromised node decrypts only a small share; selection is statistically auditable; goal is that "targeting users should require a wide attack". No fleet-size threshold is stated — [Apple PCC](https://security.apple.com/blog/private-cloud-compute/).
- Meta: "a specific user cannot be routed to any specific hardware", and an attacker shouldn't be able to target a user without compromising the whole system. Meta uses a third-party relay and a third-party CDN for HPKE keys — [Meta engineering](https://engineering.fb.com/2025/04/29/security/whatsapp-private-processing-ai-tools/).

**Tor's own limits.**
- "Tor does not claim to completely solve end-to-end timing or intersection attacks"; it "does not protect against such a strong adversary" (global passive); traffic confirmation is "outside our design goals" — [Tor design paper](https://svn-archive.torproject.org/svn/projects/design-paper/tor-design.html).
- Local ISP sees only that "you are communicating with Tor nodes" — [Tor support](https://support.torproject.org/about/protections/).

### Inferences

| Property | Tor onion service | OHTTP (with chunked draft) |
|---|---|---|
| Client IP hidden from service | Yes | Yes |
| Who can see client IP and service together | Nobody singly; guard + service colluding by timing | The relay sees client and gateway; relay + gateway collusion is trivial |
| Destination hidden from local network | Yes, up to fingerprinting | Only that a relay is used, which may identify the service |
| Independent party required | None by contract; volunteers | Yes, required by the RFC |
| Message size visible | Cell granularity | Record sizes unless padded |
| Request setup | Seconds cold | About two HTTP exchanges |
| Streaming | Native | Chunked draft, not an RFC yet |
| Per-request unlinkability | Only if isolating per request | By construction |
| Availability | Exposed to network incidents such as Oct-Nov 2026 | Relay's SLA |

**Non-targetability analysis.**
- Apple's property has two parts: the attacker cannot choose which node a target hits, and each compromised node sees only a small share. Tor plus anonymous credits gives something toward the first part only, because the operator cannot pick out a target by IP.
- N=1: there is nothing to steer, and the machine sees 100% of requests. Literal reading: "cannot target without compromising the whole system" holds only because the whole system is one box. The protective consequence, that attacks are wide and detectable, disappears because a single compromise suffices.
- Small N: exposure per compromised node is about 1/N, which remains large. The client, not the operator, should choose a node from a consistent list, otherwise the operator can steer on any non-identity signal. Apple's statistical audit has little power at N=2-5.
- Partitioning can defeat this even with Tor: per-user onion addresses, per-user attestation responses, or per-user credential keys. Cross-track: issuer key consistency.
- Honest claim for the PoC: "the operator does not learn your IP and cannot link requests by network identity, absent both-ends correlation." Do not claim non-targetability.

**What is lost with no provider-independent relay:** a contractually accountable no-log party, cheap per-request unlinkability, low latency, a place to do pre-gateway rate limiting, and an operator-independent key distribution point (this needs another solution anyway).

**What is gained:** no relay contract, no single collusion point by construction, the destination hidden from the ISP, streaming that exists today, and usable from censored networks. Tor's circuit relays are also independent of the provider, just not accountable to anyone.

The transport interface should allow OHTTP later. Tor is the better PoC choice because it needs no third-party agreement.

### Gaps
- Whether any commercial OHTTP relay supports chunked messages today, and at what price, was not researched.
- How Meta and Apple stream responses over OHTTP was not established.

## 6. Traffic analysis: does the 514-byte cell defeat token-stream side channels? What remains per observer, what padding costs, and is timing correlation realistic?

### Takeaway
Fixed cells remove the per-token size channel on the Tor path. They do not defeat these attacks, because token count and timing survive. Flush-per-token over Tor turns the attacker's input into roughly the "time-only" case, which is weaker but demonstrably non-zero. No paper has tested this on Tor. Real protection needs fixed-rate shaping inside the attested channel, and it is cheap on the wire with compact framing. Two things remain: start and stop times, and a conflict with per-token billing, since an exact spend amount reveals exactly what padding hides.

### Cited Findings

**Attacks.**
- Whisper Leak (Microsoft, arXiv 2511.03675, Nov 2025): passive observer of TLS record sizes and inter-arrival times across 28 models; one target topic versus 11,716 background prompts. 17 of 28 models exceed 98% AUPRC. For 17 models, projected precision is 100% at 5-20% recall at 10,000:1 noise — [Whisper Leak](https://arxiv.org/html/2511.03675). A secondary report's "over 98% across 28 models" overstates this.
- Whisper Leak by feature: average AUPRC for size-only 93.3-97.1% across classifiers; time-only 69.7-71.8% on average, ranging 56.4-84.0% per model. The paper treats about 50% as chance; the collected set is about 46% positives by my arithmetic. Size-only is often about as strong as both combined — [same](https://arxiv.org/html/2511.03675).
- Whisper Leak mitigations: random padding 97.5 to 92.9%; 5-token batching 98.2 to 94.8%; packet injection 98.1 to 93.3% at 2-3x bandwidth; and padding to uniform size at a fixed rate "provides strong protection" at significant cost. Tor and VPN are mentioned only in related work, not tested — [same](https://arxiv.org/html/2511.03675).
- Weiss et al. (USENIX Security 2024): token-length channel. Reconstruction of 27% and topic inference of 53% on the USENIX page, against 29% and 55% in arXiv v1 — [USENIX](https://www.usenix.org/conference/usenixsecurity24/presentation/weiss), [arXiv](https://arxiv.org/abs/2403.09751). Countermeasures: random padding, grouping tokens, non-streaming — [paper](https://usenix.org/system/files/usenixsecurity24-weiss.pdf).
- Carlini and Nasr (arXiv 2410.17175, Oct 2024): timing-based, with each packet about one token. 89.5-99.8% topic classification after ten messages on open-source models; 79% (GPT-4) and 82% (GPT-4o) in the ChatGPT web UI. It remained effective after vendors patched the length channel. A constant output rate "will thwart our attacks completely": 10 ms intervals cost 50-300% bandwidth; 80 ms intervals cost under 10% bandwidth plus 4-13 ms per token — [paper](https://arxiv.org/html/2410.17175v1).
- Wei et al. (arXiv 2411.01076 v4, Feb 2026): speculative decoding leaks through per-iteration token counts or packet sizes: fingerprinting 77.6-100% at temperature 0.3. Mitigations are padding and iteration-wise aggregation — [abstract](https://arxiv.org/abs/2411.01076). Padding every packet cut attack success by about 70% with up to 8.7x packet growth (via search snippet) — [PDF](https://arxiv.org/pdf/2411.01076).
- Soleimani et al. (ePrint 2025/167): 71-92% on vLLM and 50-90% on ChatGPT for private attributes; mitigations trade latency and bandwidth — [ePrint](https://eprint.iacr.org/2025/167).
- Zhang et al. (arXiv 2412.15431): output token count alone leaks: output language at over 75% precision and input class at over 70%; recoverable through timing over a network — [Time Will Tell](https://arxiv.org/abs/2412.15431).
- NetEcho (arXiv 2510.25472, Oct 2025): about 70% information recovery; batching reduces but does not remove leakage; up to 20% injected dummy packets cost about 2% accuracy; attacker can collect its own training traces as an ordinary user — [NetEcho](https://arxiv.org/html/2510.25472).
- PromptEcho (Li et al., USENIX Security 2026): merged chunks still leak through sizes: 42.5% topic and 25.5% of responses above 0.9 cosine similarity on GPT-4o and DeepSeek-V3. Fixed padding to MSS costs about 3.2x; dynamic padding about 1.4x. Limited to single-turn English — [paper](https://www.usenix.org/system/files/usenixsecurity26-li-sijia.pdf) (text extracted with my own script).

**Vendor responses.**
- Cloudflare random `p` field (2024-03-14) — [Cloudflare](https://blog.cloudflare.com/ai-side-channel-attack-mitigated/).
- OpenAI `include_obfuscation`, on by default (SDK docs mirror, via search snippet) — [SDK docs](https://rubydoc.info/gems/openai/OpenAI/Models/Beta/ResponseCreateParams/StreamOptions). OpenAI opted for dynamic obfuscation — [PromptEcho](https://www.usenix.org/system/files/usenixsecurity26-li-sijia.pdf).
- OpenAI, xAI (Aug 2025), Mistral (Sep 2025) and Microsoft (Oct 2025) mitigated; some providers declined or did not respond — [Whisper Leak §8](https://arxiv.org/html/2511.03675).

**Tor padding is not end-to-end.**
- Circuit padding machines cover only client-side onion circuit setup, during the first 10 relay cells, and are negotiated with a relay on the path. Intro overhead is about 11 cells, rendezvous 4. Inter-arrival timing is not obfuscated — [padding-spec circuit](https://spec.torproject.org/padding-spec/circuit-level-padding.html).
- Connection padding targets NetFlow record resolution with 1.5-9.5 s timers, about 103 B/s, on client-guard links only by default — [padding-spec connection](https://spec.torproject.org/padding-spec/connection-level-padding.html).

**Fingerprinting and correlation on Tor.**
- Kwon et al. (2015): over 98% TPR and under 0.1% FPR for passively detecting hidden-service activity (via search snippet) — [paper](https://css.csail.mit.edu/6.858/2023/readings/circuit-fp.pdf). Padding machines came later.
- Cherubin et al. (USENIX Security 2022): real-traffic website fingerprinting: above 95% for 5 sites, below 80% for 25 — [abstract](https://www.usenix.org/conference/usenixsecurity22/presentation/cherubin).
- DeepCoFFEA (IEEE S&P 2022): 93% TPR versus at most 13% for prior work when tuned for high precision; "existing lightweight defenses are not sufficient" — [abstract](https://par.nsf.gov/biblio/10348312).
- Tor Project on the German case: a long-lived connection makes timing analysis more feasible ("the longevity of the user connection"), and netflow connection times can find a user once the guard is known. Attacks were on outdated software that predated vanguards-lite — [Tor blog](https://blog.torproject.org/tor-is-still-safe/).
- "an attacker watching both ends… will be able to confirm the correspondence with high probability" — [Tor design paper §7](https://svn-archive.torproject.org/svn/projects/design-paper/tor-design.html).

### Inferences

**What survives the cell (from specs and source; untested in practice):**

| Channel | With Tor flush-per-token | Papers affected |
|---|---|---|
| Per-token byte length | Gone on the Tor path; every token fits one padded cell | Weiss, size half of Whisper Leak |
| Batch byte length | Reduced to about 498-byte resolution via cells per tick | PromptEcho, NetEcho Trace B |
| Token count | Exact (cells about equal tokens) | Time Will Tell |
| Inter-token timing | Present, blurred by relay queues | Carlini and Nasr, Whisper time-only |
| Tokens per iteration | Bursts visible | Wei et al., Soleimani et al. |
| Prompt size | Upload cell count | All, if unpadded |
| Time to first token | Visible; depends on prompt and queue | Timing papers |

Reading Whisper Leak's table across, Tor plausibly moves an attacker from about 98% toward the 70-84% time-only range against a roughly 46-50% baseline. At 10,000:1 that likely fails for mass surveillance, but this column is an extrapolation with no jitter. Multi-message sessions and better classifiers score far higher in Carlini and Nasr. So Tor is a real but partial mitigation, not a defeat. An attacker can also collect its own training traces over Tor as a paying user.

Avoid speculative decoding in the PoC's serving stack, or aggregate per tick. It adds a channel for no benefit on a small model.

**Per observer:**

- **Local ISP or guard.** Sees TLS to a guard in 514-byte multiples and timing, not the destination. It can likely tell an onion circuit is in use (Kwon; C tor's setup padding helps, Arti's is unconfirmed). A steady 30-100 cells/s down, nearly nothing up, for tens of seconds is a distinctive shape, and if few Tor services stream like this, the shape may identify the class of service. Topic inference from timing is plausible for a dedicated adversary.
- **Global passive adversary.** Sees both ends, and correlation is by Tor's own account outside its design goals. Nothing in this design addresses it.
- **Hosting operator (Verda).** Always in position, no tap required, and sees every CVM packet. With Tor inside the CVM it sees cell multiples and timing; at low concurrency each request is an unmixed trace. It does not get client IPs. It can run Whisper-style classifiers on its own tenant's traffic after training as a customer. So "the operator can't learn what you asked" needs qualifying unless shaped.
- **Cross-track (TEE):** the host may also see CPU and GPU activity timing regardless of the network (scheduling, DMA through shared buffers, power). Network shaping does not address this, so token timing may leak below Tor entirely. Needs the TEE track's answer.

**Padding strategy** (`shaping_and_correlation.py`; synthetic length distribution: lognormal median 250 tokens, sigma 1, 60 tokens/s, not measured from any model):

| Strategy | Length entropy visible (bits) | Mean cells/request | Mean kB | Added mean latency |
|---|---|---|---|---|
| Flush per token, SSE | 9.99 | 411 | 211 | 0 |
| 250 ms batch, SSE | 8.63 | 149 | 77 | 125 ms |
| 1 cell per 250 ms, compact, stop at end | 6.11 | 28 | 14 | 125 ms |
| 1 cell per 250 ms, pad to 30/60/120/240/480 s bucket | 0.18 | 124 | 64 | 125 ms |
| 1 cell per 250 ms, fixed 120 s slots | 0.00 | 480 | 247 | 125 ms |

Recommended target, for validating later rather than as an architecture decision:
- Fixed tick inside the attested channel, sending real or dummy bytes each tick, in compact binary framing rather than SSE JSON. 4 cells/s is 16.4 kbit/s, enough for about 325 tokens/s at 6 B/token. 10 cells/s (100 ms tick) is 41 kbit/s for smoother text.
- Start ticking as soon as the request arrives, to hide time to first token.
- Pad to duration buckets. The user sees the true end immediately via an in-band end marker; only the connection persists.
- Pad uploads to size buckets (for example 4, 16, 64, 256 KiB), so 4 KiB is about 9 cells and 64 KiB about 132.
- This must be implemented in the enclave, since Tor offers no end-to-end padding primitive. Force each tick out with a flush (C tor partial package, Arti `flush`). Avoid compression, which leaks size.
- UX cost is small: about 125 ms mean delay (50 ms at 100 ms ticks) and possibly chunky text. Constant-rate overhead numbers from Carlini and Nasr are for direct TLS, not Tor, so measure instead of assuming.
- Remaining leaks: request start and stop, request count, session rhythm, and that the client uses this kind of service. A constant-rate stream is itself recognisable on Tor. Tor network load per request is a small multiple of unshaped traffic, at most about 247 kB per 120 s slot.

**Billing conflict (surprise, cross-track).** ACT exposes the amount spent to the issuer, per the conversation under review (verify in the ACT track). If that amount equals exact token count, the operator gets precisely the channel padding hides, and Time Will Tell shows token count alone leaks language and class. Either round billing up to coarse denominations aligned with the padding buckets, or hide the amount from the party who sees network traces. "Metered per token" and "length hidden from the operator" cannot both hold exactly. The client-chosen ceiling also leaks unless it is standardised.

**Is end-to-end correlation realistic for this service?**

Base-rate arithmetic for one flow matched on start time (Poisson arrivals; `shaping_and_correlation.py`):

| Requests/hour | Expected false candidates, within 1 s of start | Same, 3 consecutive turns |
|---|---|---|
| 10 | 0.006 | 2e-7 |
| 100 | 0.056 | 2e-4 |
| 1,000 | 0.56 | 0.17 |
| 10,000 | 5.6 | 170 |

- At PoC volumes any party with both vantage points identifies the flow from start times alone. Base-rate effects that protect Tor generally do not help a small service, and shaping does not hide start times.
- Global tick alignment gives cohorts only at volume: at 100 req/h and a 30 s tick, about 0.8 other flows in the same tick, for 15 s of mean added delay. Not worthwhile.
- Who has both vantage points: the operator plus a client's ISP (legal process or compromise), state adversaries with netflow access, and an operator who also runs relays and gets lucky on guards.
- So: realistic against a state or a targeted investigation. Not a threat from the operator acting alone, or from a commercial observer holding one side. The notes should say this plainly rather than say "anonymous".

### Gaps
- No paper tests any LLM side channel over Tor. Everything mapping those results onto cells is my inference.
- Whether a fixed-rate stream survives relay jitter as uninformative was not tested. I expect jitter to be independent of content, but this is an assumption.
- The token-rate and length distributions in the simulation are synthetic.
- DeepCoFFEA's false-positive numbers could not be read; my text extractor failed on that PDF, so only the abstract's rates are cited.
- Whether Verda's host can observe GPU activity timing is unresolved here.

## 7. Anonymity set: what does Tor achieve when very few clients use one onion service?

### Takeaway
Tor's large set applies to one question only: which IP? It stays real at any service size. The set that matters for linking requests is the service's own active users, and at PoC scale that may be single digits. Tor cannot enlarge it. Requests from a lone active user are trivially "the same client" regardless of circuits or credentials.

### Cited Findings
- Network scale: median estimated direct users 3.18M/day over the window, about 3.0M/day just before the Oct 28 artefact — [users CSV](https://metrics.torproject.org/userstats-relay-country.csv?start=2026-08-08&end=2026-11-06&country=all&events=off). About 9,400 relays — [network size](https://metrics.torproject.org/networksize.csv?start=2026-08-08&end=2026-11-06). About 660,000 v3 onion addresses (extrapolated, median; 400,472 on 2026-11-03) — [onions seen](https://metrics.torproject.org/hidserv-dir-v3-onions-seen.csv?start=2026-08-08&end=2026-11-06).
- OHTTP's analogue: a higher-volume relay "can provide better privacy by providing larger sets of messages", and each active configuration partitions the set — [RFC 9458](https://www.rfc-editor.org/rfc/rfc9458.html).
- Each new onion service type shrinks the anonymity set for those needing location privacy — [prop 260](https://spec.torproject.org/proposals/260-rend-single-onion.html).
- Meta's review noted identity could leak through correlating credential issuance timing with credential use (per the conversation under review; the primary NCC report was not read here, so the Meta or ACT track should verify).

### Inferences
| Adversary question | Effective set | Tor's contribution |
|---|---|---|
| Operator: which IP sent this? | All Tor users | Full, independent of service size |
| Local ISP: which service is this user reaching? | All Tor traffic, narrowed by shape | Good, but shape can narrow it |
| Operator: which buyer sent this? | Buyers with unspent credits, known from payment records | None |
| Operator: are these two requests the same person? | Concurrently active clients | None beyond avoiding a direct link |
| Operator + client-side observer: who is this? | Effectively 1 at low volume | None |

- With one active user, unlinkability is vacuous even with perfect ACT and fresh circuits. With five buyers, every request belongs to one of five named cardholders (or, more precisely, payment identities). The claim is "structurally anonymous, empirically small". The PoC should show this rather than advertise anonymity.
- Operator-generated decoy clients cannot help, since the operator is the adversary. Client-generated cover requests add noise, not buyers, and cost compute.
- Features partition the set: each model is a separate set (and likely a separate fleet of one), as are client versions, ceilings, Arti vs C tor (at the guard), and languages. Adding models is an anonymity trade-off, not just an engineering task.
- The first spend soon after purchase is a strong link when few people buy. Cross-track: wallets should delay and spread spends.

### Gaps
- A minimum viable set size has no agreed answer that I found. There is no population data for a service that does not yet exist.

## Proposed validation

None of this was run. All of it requires running third-party software. Thresholds are my proposals. Because of the incident described in section 1, repeat measurements on at least two separate days and record the C tor version and date.

| ID | What to run | Confirms if | Refutes if | Effort |
|---|---|---|---|---|
| V1 Streaming | Short-lived onion service (C tor 0.4.9.16) on a cheap VM in front of a synthetic SSE generator at 30, 60, 100 tokens/s; separate Tor client elsewhere. Modes: flush-per-token, 250 ms batch, fixed 1 cell per tick. Measure rendezvous time (SOCKS CONNECT to success), TTFB, inter-chunk latency p50/p95/p99, stalls over 1 s and 5 s, and stream death over 3-5 minute streams. At least 30 cold connects per mode and 10 long streams per rate. | Cold p50 under 5 s, p95 under 15 s; at 60 tokens/s and 250 ms batching, inter-chunk p95 under 500 ms; streams with a stall over 5 s under 5%; stream death under 5% per 5 minutes | Any threshold repeatedly missed, or flush-per-token sustaining less than the offered rate | About 1 h setup, 4-6 h unattended |
| V2 Cell reality | Packet capture at both client and server NICs during V1. Compare TLS record sizes and counts with tokens sent. | Records are 514-byte multiples plus overhead, and each flush yields one cell | Coalescing makes cells far fewer than tokens, or sizes vary with token length | 1-2 h |
| V3 Isolation cost | From V1: fresh SOCKS credentials or `new_isolation_group()` per request vs reused circuit vs pre-warmed isolated circuit. Add inner TLS handshake. | Cold minus warm matches the 1.5-4 s estimate | Over 10 s, or highly unstable | Part of V1, plus 1 h |
| V4 Arti vs C tor | Repeat V1 cold and streaming tests with an Arti 2.8.0 client, then with an Arti service for comparison only. | Client performance within about 25% of C tor | Failures or large gaps | 2-3 h |
| V5 Exit reachability | Through at least 50 exits, unauthenticated GET or HEAD at under one request per 5 s: `inference.tinfoil.sh/.well-known/tinfoil-proxy`, `kds-proxy.tinfoil.sh`, `github-proxy.tinfoil.sh`, `atc.tinfoil.sh`, AMD KDS, Sigstore, `api.stripe.com`, `js.stripe.com`. No credentials, no purchases, no form submission. Record status codes and challenge markers such as `cf-mitigated`. | Over 90% of exits return non-challenge responses on all hosts needed | Attestation proxies or the router challenged or blocked; then evaluate caching or gateway | 30-60 min |
| V6 Operator view | During V1, capture at the VM's NIC as the operator would, with 1, 3 and 10 concurrent streams. Attempt to split per-request traces from timing. Repeat in single-onion mode. | Quantifies mixing, and whether single-onion is worse as argued | Single-onion is not distinguishably worse, which would make its latency gain worth considering | About half a day |
| V7 Side-channel lab | Reproduce a Whisper-style classifier (LightGBM on timing features) on my own synthetic or small-model traces: direct TLS vs Tor flush-per-token vs fixed tick and bucket. | AUPRC falls near baseline only for fixed tick, and Tor flush-per-token sits between | Tor flush-per-token already near baseline (padding unnecessary), or fixed tick still clearly separates | 2-3 days; lower priority |
| V8 Hosting | Confirm outbound TCP to Tor relays from a Verda CC instance and ask Verda whether a non-exit onion service is permitted. | Works and is permitted | Egress blocked or disallowed | 1 h plus support response |

## Claim status table

"Untested" means supported only by documents, specs, source reading, published metrics, or my arithmetic, with no run done here.

| Claim | Status | Evidence URLs |
|---|---|---|
| Tor bandwidth suffices for 30-100 tokens/s streams | solid (untested for streaming) | https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion |
| Flush-per-token costs about 1 cell per token: 30/60/100 cells/s, 123/247/411 kbit/s | solid (untested; upper bound) | https://spec.torproject.org/tor-spec/relay-cells.html ; https://tpo.pages.torproject.net/core/doc/tor/relay_8c_source.html ; https://docs.rs/tor-proto/latest/src/tor_proto/client/stream/data.rs.html |
| Onion path median latency is 270-670 ms vs 110-520 ms for public | solid | https://metrics.torproject.org/onionperf-latencies.csv?start=2026-08-08&end=2026-11-06&server=onion |
| Per-request cold start is low seconds | conditional (thin evidence; untested) | https://metrics.torproject.org/onionperf-buildtimes.csv?start=2026-08-08&end=2026-11-06 ; https://arxiv.org/pdf/2001.05276 |
| 3-5 minute streams are reliable | unknown | https://metrics.torproject.org/torperf-failures.csv?start=2026-08-08&end=2026-11-06&server=onion |
| Conflux helps onion streams today | false (design exists, not deployed for onion by available evidence) | https://spec.torproject.org/rend-spec/hsdesc-encrypt.html ; https://spec.torproject.org/proposals/329-traffic-splitting.html ; https://metrics.torproject.org/onionperf-throughput.csv?start=2026-08-08&end=2026-11-06&server=onion |
| Conflux keeps a stream alive when a circuit fails | weak (C tor closes in the common cases) | https://spec.torproject.org/proposals/329-traffic-splitting.html |
| Tor network is currently calm | false (Oct-Nov 2026 incident) | https://forum.torproject.org/t/security-release-0-4-9-16/22372 ; https://forum.torproject.org/t/tor-0-4-9-15-begin-dir-hsdir-flood-oom-kill-despite-maxmeminqueues/22359 |
| Arti hosting is ready for privacy-critical onion services | false by its own config note | https://docs.rs/crate/arti/latest/source/src/arti-example-config.toml ; https://arti.torproject.org/FAQs |
| Arti works as an embedded onion client for a PoC | conditional (untested) | https://arti.torproject.org/FAQs ; https://docs.rs/arti-client/latest/arti_client/struct.StreamPrefs.html |
| C tor is the mature service side | solid, with a two-week security release burden | https://forum.torproject.org/t/security-release-0-4-9-12/22096 ; https://manpages.debian.org/testing/tor/torrc.5.en.html |
| PoW defence is available | solid on C tor; experimental on Arti | https://manpages.debian.org/testing/tor/torrc.5.en.html ; https://docs.rs/crate/arti/latest/features |
| Single-onion mode is a free latency win | weak (no security analysis; unmixed operator view is my inference) | https://spec.torproject.org/proposals/260-rend-single-onion.html |
| Ordinary browser clients can do this | weak (prototype WASM only; native client or Tor Browser otherwise) | https://reads.ethereum.foundation/blog/embedding-arti-in-the-browser/ |
| Tor inside the CVM is needed to keep stream boundaries from the operator | conditional (inference; untested) | https://docs.tinfoil.sh/verification/attestation-architecture.md |
| Onion address can be bound to attestation | conditional (pattern exists, no standard; untested) | https://cloud.phala.com/templates/tor-hidden-service ; https://arxiv.org/pdf/2412.16064 |
| Per-request circuit isolation is obtainable | solid mechanism (cost untested) | https://manpages.debian.org/testing/tor/torrc.5.en.html ; https://docs.rs/arti-client/latest/arti_client/struct.StreamPrefs.html |
| Exit blocking rules out reaching Tinfoil | unknown (no published evidence for it; attestation proxies behind Cloudflare are the risk; untested via Tor) | https://docs.tinfoil.sh/verification/attestation-architecture.md ; https://www.lightbluetouchpaper.org/?p=8018 |
| Stripe blocks Tor | unknown, and moot since purchase avoids Tor | https://docs.stripe.com/radar/rules/supported-attributes.md |
| Fixed cells remove token-length leakage on the Tor path | conditional (from spec; untested; not past an exit) | https://spec.torproject.org/tor-spec/relay-cells.html ; https://arxiv.org/abs/2403.09751 |
| Fixed cells defeat Whisper Leak class attacks | false (timing and count remain) | https://arxiv.org/html/2511.03675 ; https://arxiv.org/html/2410.17175v1 ; https://arxiv.org/abs/2412.15431 |
| Fixed-rate shaping inside the channel closes stream-shape leakage cheaply | conditional (start and stop remain; synthetic arithmetic; untested) | https://arxiv.org/html/2410.17175v1 ; https://spec.torproject.org/padding-spec/circuit-level-padding.html |
| Per-token billing is compatible with hiding length from the operator | weak (exact spend reveals token count; verify in ACT track) | https://arxiv.org/abs/2412.15431 |
| Tor gives Meta or Apple style non-targetability on a small fleet | false (inference from their definitions) | https://security.apple.com/blog/private-cloud-compute/ ; https://engineering.fb.com/2025/04/29/security/whatsapp-private-processing-ai-tools/ |
| Chunked OHTTP is a finished standard for streaming | conditional (RFC Editor queue, no number) | https://datatracker.ietf.org/doc/draft-ietf-ohai-chunked-ohttp/ ; https://www.rfc-editor.org/rfc/rfc9458.html |
| Tor hides the client's IP from the hosting operator | solid by design (untested here; absent both-ends correlation) | https://support.torproject.org/about/protections/ ; https://svn-archive.torproject.org/svn/projects/design-paper/tor-design.html |
| End-to-end correlation is a realistic threat | solid for both-ends adversaries, near-certain at PoC volume; not for an operator alone | https://svn-archive.torproject.org/svn/projects/design-paper/tor-design.html ; https://par.nsf.gov/biblio/10348312 ; https://blog.torproject.org/tor-is-still-safe/ |
| Tor gives a large anonymity set for this service | conditional (large for IP; small for request linkage and buyers) | https://metrics.torproject.org/userstats-relay-country.csv?start=2026-08-08&end=2026-11-06&country=all&events=off ; https://www.rfc-editor.org/rfc/rfc9458.html |
| Verda permits an onion service in a CC instance | unknown (visible terms do not mention Tor; schedules 4-7 not read) | https://verda.com/terms-and-conditions |
