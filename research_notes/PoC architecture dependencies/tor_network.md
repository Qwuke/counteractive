# Tor and network dependencies for the PoC (survey, 2026-11-06)

| Component | Decision | Version | Licence | What it saves us | Main risk |
|---|---|---|---|---|---|
| C tor from `deb.torproject.org` (service side) | ADOPT | 0.4.9.16, 2026-11-04; deb `0.4.9.16-1~noble+1` | 3-clause BSD source; GPLv3 as packaged, because of `--enable-gpl` | The whole onion service: crypto, descriptors, PoW, intro-point DoS defence. Not realistic to build | Six releases since June, and each one we take changes the measurement. It is C code holding plaintext if there is no inner TLS |
| Ubuntu universe `tor` | AVOID | 0.4.9.11 (noble-updates, 2026-08-26) | Same | Nothing the TPO package lacks | Five releases behind, missing onion-service security fixes |
| Onion key: tor generates it on tmpfs at boot | ADOPT for PoC | Built into tor | n/a | All key code (about 25 lines plus tests) and the SNP key-derivation step | Address changes on each boot, so it needs a published record |
| Onion key: pre-written derived key file | BUILD-OURSELVES, only if a stable address becomes a requirement | About 25 lines | Ours | Stable address across restarts of the same image on the same chip | Still changes at each image rotation; adds secret-handling code |
| `ADD_ONION` over the control port | AVOID | tor 0.4.9.16 | n/a | Writing a file | Needs a control port, rules out `Sandbox`, has no IntroDoS keywords |
| tor-interface, torut, libtor | AVOID | 0.6.8; 0.2.1; 47.13.0+0.4.7.x | BSD-3-Clause; MIT; not read for libtor | Control-port client we do not need | Extra dependency; torut and libtor are stale |
| Client authorization | AVOID | tor and Arti both support it | n/a | Nothing here | Per-client keys link users; a key shared in a public wallet adds nothing |
| Stream limits, PoW, IntroDoS options | ADOPT | tor 0.4.9.16 | Config only | The only filters ahead of the credit check | Defaults are generic; need measuring on the real VM |
| Tor `Sandbox 1` | ADOPT if the image test passes | tor 0.4.9.16 | Config only | A second syscall filter on C code that holds plaintext | Forces interpreted PoW verify; the man page warns of libc and kernel incompatibility |
| axum over a Unix socket | ADOPT | 0.8.9, 2026-04-14 | MIT | Listener, routing and SSE, about 200 lines | Low. Pre-1.0 API churn |
| hyper + hyper-util + http-body-util (wallet) | ADOPT | 1.12.0; 0.1.21; 0.1.5 | MIT | HTTP/1.1 client over `DataStream` without a shim; about 40 lines of glue | Must write our own timeouts and retries |
| Inner TLS inside the onion | AVOID for PoC, required before real user prompts | rustls not surveyed | n/a | About 150 lines including a custom verifier, one round trip per circuit | C tor is in the plaintext TCB until it is added |
| arti-client (wallet use plane) | ADOPT with explicit features | 0.48.0 (arti 2.8.0), 2026-11-04 | MIT OR Apache-2.0 | The entire Tor client. Not realistic to build | Default features omit onion and vanguards; can `exit(1)`; onion flakiness |
| Arti `hs-pow-full` feature | WRAP behind our own Cargo feature, off until flood-tested | 0.48.0; pulls equix 0.8.0 | equix is LGPL-3.0-only | Puzzle solver, which is not realistic to build | Marked experimental; live solve untested; LGPL in the wallet binary |
| arti-hyper | AVOID | 0.19.0, 2024-06-05 | Not relevant | Nothing | README says obsolete and unmaintained, no security patches |
| arti-ureq | AVOID | 0.48.0, 2026-11-04 | MIT OR Apache-2.0 | A ready connector | Second HTTP stack; wrong fit for async streaming |
| reqwest on the use plane | AVOID (keep it for purchase) | 0.13.5, 2026-09-08 | MIT OR Apache-2.0 | Nothing here | No public way to inject a `DataStream`; would mix the two stacks |
| Chutney | ADOPT with caveats, for tests a live network must not see | Commit `d6659ac3`, main @ 2026-07-01 | Permissive custom notice | A private Tor network, about a week of work | Upstream's native Arti job is disabled; about 2-3 days to wire up (INFERRED) |
| Shadow | AVOID for PoC | v3.3.0, 2025-10-16 | BSD-style text in `LICENSE` | Determinism and speed | Heavy; Linux only; adds another simulator to debug |
| systemd units + nftables | ADOPT | Ubuntu image packages, versions UNKNOWN | Not read | Network and filesystem confinement, config only | nftables boot ordering; egress exceptions at boot |
| Verda "Public + Private IP" | ADOPT | Docs updated 2026-10-27 | n/a | No NAT instance | Verda does not filter inbound, so the guest firewall is the only filter |
| Verda private-only, NAT instance | AVOID | Beta | n/a | Nothing | Private-only cannot reach Tor at all; NAT adds a billed machine and a single point of failure |

## How this was checked

- **Nothing was run.** Every runtime statement comes from reading source or documents, so behaviour marked INFERRED needs a test.
- **Sources.** Release tarball, crates.io source, Debian packaging diffs, Launchpad API, GitHub API and public docs. Downloads are in the scratchpad at `dep_survey/tor_network`.
- **GitLab was unreadable.** `gitlab.torproject.org` returned 403 to curl and WebFetch, and I did not try to get around it. C tor and Arti last-commit dates are UNKNOWN from primary sources, as is the pinned Chutney source.
- **Fork used for test tooling.** Arti's test harness and CI file were read from a third-party GitHub fork, `voltrevo/arti` at `2527494a` (2026-10-22). It may differ from upstream, so upstream conclusions from it are INFERRED.
- **Estimates.** Line counts and day counts are my estimates, not measurements.
- **One slip.** A read-only command I wrote included `./configure --help` inside a downloaded tree. The permission system blocked the whole command, nothing ran, and I redid it as text reads.

## 1. C tor, service side

**Decision: ADOPT the TPO package, pinned by hash, with our own archived copy.** Maintainer is The Tor Project; packaging is by Peter Palfrader (package metadata). Last commit date: UNKNOWN (GitLab refused; the GitHub mirror is stale, pushed 2024-03-05).

- **Version.** VERIFIED `ChangeLog` line 1: "Changes in version 0.4.9.16 - 2026-11-04". It calls this "an emergency release affecting only directory authorities".
- **Tarball.** VERIFIED https://dist.torproject.org/: SHA-256 `6180e193152f4525bdca5475168419b2a2035b963c3c60162a187f86e6160e48`, matches `.sha256sum` and the orig tarball in the TPO `.dsc`. The `.asc` signature was not checked.
- **Cadence.** VERIFIED `ChangeLog`: .11 06-25, .12 09-08, .13 09-23, .14 10-07, .15 10-28, .16 11-04.
- **Old versions.** VERIFIED `ChangeLog` for .15: "Tor versions 0.4.8 and earlier will no longer work on the network". So a frozen image has a limited life.
- **Minimum version is 0.4.9.14.** VERIFIED `ChangeLog`: .14 fixes bug 41434, a per-stream memory leak on `unix:` onion targets, which is exactly our setup.

**PoW build flag.** PoW needs `--enable-gpl`. VERIFIED:
- `configure.ac` 416-425: `AM_CONDITIONAL(BUILD_MODULE_POW, [test "x$license_option" = "xGPL" && test "x$enable_module_pow" != "xno"])`.
- `src/feature/hs/hs_config.c` 330-335 refuses the config: "this build of tor does not include the required 'pow' module."
- `src/app/config/config.c` 4533-4535: `--version` prints the GPLv3 notice for such builds.
- https://onionservices.torproject.org/technology/security/pow/ agrees (updated 2026-05-08, marked "Needs update").
- Which source file forces the GPL: UNKNOWN. `src/ext/equix` has no licence file, its sources say "See LICENSE", and the top-level `LICENSE` does not name equix or hashx.

**Packages.**
- TPO `debian/rules` has `--enable-gpl`, `--enable-systemd` and `hardening=+all`. VERIFIED `debtor/noble.diff` 5494-5505; changelog line 1036 records the PoW reason.
- Ubuntu's also has `--enable-gpl`, but it sits at 0.4.9.11 in noble-updates and resolute-updates (Launchpad API, published 2026-08-26). It misses TROVE-2026-053 (isolation), -058 and -012 (onion service), -067 and bug 41434.
- TPO repo: VERIFIED dists include noble and resolute (Ubuntu 24.04 and 26.04, the two Verda CC images). Deb SHA-256 `90aa64065178a458efd008ea597de2e6541d37eee29b448c78d3490f3042a307`.

**Reproducible image.**
- VERIFIED by listing: the TPO pool keeps only the current stable for noble. A pinned hash alone will stop resolving, so archive the deb and its source ourselves.
- Debian's reproducible-builds page says "looks reproducible" for Debian's own builds. For the TPO builds: UNKNOWN. Embedding by hash does not depend on rebuilding.
- Simpler alternative for later: build from the tarball with `--enable-gpl`. That adds a toolchain to reproduce, so not for the PoC.
- Assert at image build (not run here): `tor --version` shows the GPL line, and `tor --list-modules` shows pow.

**Considered, not chosen: Arti as the service.** `launch_onion_service` exists (`client.rs` 1989), and the onion-services feature matrix lists server PoW as experimental for both implementations. Dropping C tor would remove the plaintext-in-C concern. IntroDoS support and service-side maturity were not read, so UNKNOWN. Keep C tor as designed.

## 2. Onion key handling and service options

**File format.** VERIFIED from tor source:
- Files are `hs_ed25519_secret_key`, `hs_ed25519_public_key` and `hostname` in `HiddenServiceDir` (`hs_service.c` 101, `load_service_keys`).
- Secret file is 96 bytes: 32-byte NUL-padded header `== ed25519v1-secret: type0 ==`, then a 64-byte expanded key (`crypto_format.c`; `crypto_ed25519.c` from line 570).
- Expansion is SHA-512(seed) with `sk[0]&=248; sk[31]&=63; sk[31]|=64` (`ext/ed25519/ref10/keypair.c` 27-35).
- tor generates a key only if it is absent, derives the public key from the secret, rewrites a missing `_public_key`, and treats a mismatch as an error (`src/feature/keymgt/loadkey.c` 379-530).
- The directory must be mode 0700 and owned by tor (`lib/fs/dir.c`). So a pre-written key works if tor can write the directory.

**Options.**

| Option | Key code | Control port | `Sandbox` | IntroDoS | Address |
|---|---|---|---|---|---|
| A. tor generates on an empty tmpfs at boot | None | Off | Works (INFERRED) | Yes | New each boot |
| B. Pre-write derived key to tmpfs | About 25 lines | Off | Works (INFERRED) | Yes | Stable until the measurement changes |
| C. `ADD_ONION` | Control client | Required | Man page says unsupported | No | Either |

- **Simplest for a measured VM is A.** The gate binds the SHA-256 of the `hostname` address into `REPORT_DATA`. No crypto crate is needed, because the wallet hashes the same string. This matches `tor_traffic_analysis.md` line 172.
- **B works, but buys little.** An SNP-derived key must mix in the measurement, or any other image on that chip could derive it (INFERRED; the SNP key API belongs to the attestation track). So the address changes at each image rotation anyway, and Section 1 shows that is frequent. Whether a Verda Shutdown then Start stays on the same chip: UNKNOWN.
- **`hostname` access.** The directory is owner-only and `HiddenServiceDirGroupReadable` defaults to 0 (man). A small root step should copy `hostname` to `/run/gate/` for the gate (INFERRED).
- **Do not HUP.** `options_act` reloads keys (`config.c` 2274). Deleting the secret and then sending HUP would generate a new identity (INFERRED).
- **State on tmpfs.** Guard state lives in tor's state file (`statefile.c` 80-83), so tmpfs means new guards each boot (INFERRED). Acceptable because location hiding is not a goal (`tor_traffic_analysis.md` line 131).

**ADD_ONION.** VERIFIED `control_cmd.c` 1636-1645, keywords are Port, Flags, MaxStreams, PoWDefensesEnabled, PoWQueueRate, PoWQueueBurst, ClientAuth, ClientAuthV3. No IntroDoS keyword. Key argument is `ED25519-V3:<b64 of 64 bytes>` (from line 1923). The service dies with the control connection unless `Detach` is set (https://spec.torproject.org/control-spec/commands.html). The man page for `Sandbox` says launching onion services through the control port is not supported.

**Rust crates.**
- tor-interface 0.6.8 (2026-09-09, BSD-3-Clause, blueprint-freespeech/gosling, pushed 2026-09-28, 384 recent downloads): has `Ed25519PrivateKey`, `V3OnionServiceId::from_private_key` and `add_onion`. No key-file writer in the files I grepped. It only helps with option C.
- torut 0.2.1 (2021-10-31, MIT): `TorSecretKeyV3`, `add_onion_v3`, dalek 1 and rand 0.7. Stale.
- libtor 47.13.0+0.4.7.x (2023-02-10): embeds a tor line that no longer works.
- tor-client-lib 0.3.2 (2026-05-03): not inspected.
- If B is ever needed: tor-llcrypto 0.48.0 has `ExpandedKeypair::{public, to_secret_key_bytes, from_secret_key_bytes}`, and tor-hscrypto `HsId` Display gives the address (VERIFIED `ed25519.rs`, `pk.rs`). Test the output against tor's own `hostname` file.

**Client authorization: AVOID.** Per-client keys are linkable (`tor_traffic_analysis.md` line 129). A key shared in a public wallet adds nothing, because descriptors are already encrypted to the address.

**Service options.** VERIFIED `doc/man/tor.1.txt` unless marked.

| Option | Default | Note |
|---|---|---|
| `HiddenServiceMaxStreams` | 0 (unlimited), max 65535 | Per rendezvous circuit, not total, so the gate still needs a global limit |
| `HiddenServiceMaxStreamsCloseCircuit` | 0 | At 0, excess requests are "silently ignored". Enforced at `hs_service.c` 4288-4306. Set to 1 so abuse costs the circuit |
| `HiddenServicePoWDefensesEnabled` | 0 | Disables the puzzle itself when not overloaded, so normal users pay nothing |
| `HiddenServicePoWQueueRate`, `QueueBurst` | 250, 2500 | Rendezvous dispatches per second. Flood cost falls on the CVM's CPU, not the GPU (`tor_traffic_analysis.md` line 129), so measure on 30 vCPUs |
| `HiddenServiceEnableIntroDoSDefense` | 0 | `RatePerSec` 25 and `BurstPerSec` 200, per intro point. An attacker can still flood it. Interaction with PoW under flood: UNKNOWN, test |
| `HiddenServiceNumIntroductionPoints` | 3, max 20 | Leave at default |
| `HiddenServiceExportCircuitID` | Off | Keep off, so the gate never holds a circuit ID |

```
# Untested sketch. Hidden-service, PoW and Sandbox options read in tor.1.txt.
SocksPort 0
ControlPort 0
DataDirectory /var/lib/tor
HiddenServiceDir /var/lib/tor/gate
HiddenServicePort 80 unix:/run/gate/gate.sock
HiddenServiceMaxStreams 8
HiddenServiceMaxStreamsCloseCircuit 1
HiddenServicePoWDefensesEnabled 1
HiddenServiceEnableIntroDoSDefense 1
Sandbox 1
CompiledProofOfWorkHash 0
```

## 3. Local stream delivery

**Decision: HTTP/1.1 over a Unix socket, one request per stream, SSE framing over chunked encoding, no inner TLS for the PoC.**

- **Unix target.** VERIFIED man: `HiddenServicePort` takes `unix:path`; parsed by `hs_parse_port_config` in `hs_common.c`. Unknown ports close the circuit by default (`connection_edge.c` 4063-4112).
- **axum 0.8.9** (2026-04-14, MIT, tokio-rs, last commit 2026-11-06). VERIFIED: `serve/listener.rs` 45-46 `#[cfg(unix)] impl Listener for tokio::net::UnixListener`; `serve` in `serve/mod.rs` 97; `response/sse.rs` has `Sse` and `keep_alive`. Default features (`Cargo.toml` 81-91) include `http1` and `tokio`, and exclude `http2`.
- **hyper 1.12.0** (2026-10-06, MIT, hyperium, last commit 2026-11-02). VERIFIED `client/conn/http1.rs` 150 `handshake`, 230 `send_request`.
- **hyper-util 0.1.21** (2026-09-24, MIT, last commit 2026-11-05). VERIFIED `rt/tokio.rs` 142 `TokioIo::new`; `poll_flush` is forwarded.
- **http-body-util 0.1.5** (2026-08-12, MIT). tokio 1.54.1 (2026-10-21, MIT).
- **Skip HTTP/2.** Tor already multiplexes streams, and HTTP/2 adds shared connection state (INFERRED).
- **Earlier result.** Arti 2.8.0 carried SSE in `spike_tor.md`, 44,100 tokens delivered. Flush behaviour matters: Arti's `poll_write` queues and `poll_flush` sends (`tor_traffic_analysis.md` line 43).

**Inner TLS: not for the PoC, and this reverses an earlier recommendation.** `tor_traffic_analysis.md` line 142 says to treat Tor as an untrusted pipe with an attested channel inside it.

- **Without TLS.** The wallet checks that the address it dialled is the one bound in `REPORT_DATA`. The rendezvous handshake derives shared circuit keys (https://spec.torproject.org/rend-spec/rendezvous-protocol.html: "both parties use the handshake output to derive shared keys"). INFERRED: that is enough for confidentiality against the network and the host.
- **The cost.** C tor holds plaintext, so a remote memory-safety bug in tor could expose prompts. VERIFIED `ChangeLog` for .12 and .13: a use-after-free when a TCP connection succeeds but its TLS handshake fails (line 321), and a "possible remote crash attack" on a client path (TROVE-2026-032). I did not assess whether any of these reach an onion service's memory. They show the class of bug, not a known route to prompts.
- **With TLS later.** Terminate rustls in the gate, pin the key through `REPORT_DATA`, and C tor drops out of the content TCB. Keep the HTTP layer generic over the stream so this is additive (INFERRED about 150 lines, one extra round trip per circuit).

## 4. Arti

**Decision: ADOPT arti-client 0.48.0 with explicit features.**

- **Release.** VERIFIED crates.io: 0.48.0 published 2026-11-04T14:25:59Z, MIT OR Apache-2.0, `rust-version` 1.92, publisher gabi-250, commit `7c038125` (same as arti 2.8.0). Commit date not read. Repository: GitLab tpo/core/arti.
- **Features.** VERIFIED `Cargo.toml`: `default = ["tokio","native-tls","compression"]`. `onion-service-client` and `vanguards` are not defaults, though they are in `full` and in the arti binary's defaults. `rustls`, `static` and `static-sqlite` exist.
- **Silent traps.** Without `onion-service-client`, `.onion` fails at runtime with `ErrorDetail::OnionAddressNotSupported`; with the feature but prefs or config off, it fails with `OnionAddressDisabled` (`address.rs` 342-351). Without `vanguards`, `config.rs` 82-95 uses `DISABLED_VANGUARDS`, with no error.
- **`allow_onion_addrs`.** VERIFIED `config.rs` 130, default true. Per-prefs override is `StreamPrefs::connect_to_onion_services` (`client.rs` 810).
- **Isolation.** VERIFIED `client.rs`: `isolated_client` 1501 ("will never share circuits"), `StreamPrefs::set_isolation` 889, `new_isolation_group` 874, `isolate_every_stream` 910 ("Use with care"), `connect` 1574, `connect_with_prefs` 1585, `with_prefs` 1707. `IsolationToken` is re-exported (`lib.rs` 72) and is a process-local `u64` with `new` (`tor-circmgr isolation.rs` 194-206). So one `isolated_client` per conversation is the least plumbing. A conversation resumed after a daemon restart gets a new circuit (INFERRED).
- **Onion state.** VERIFIED tor-hsclient `state.rs`: `RETAIN_CIRCUIT_AFTER_LAST_USE` 10 minutes (line 104), `RETAIN_DATA` 48 hours (line 124), shared only "subject to isolations" (line 143). So a conversation idle for over 10 minutes pays for a fresh circuit (INFERRED).
- **Bootstrap and directories.** VERIFIED `client.rs`: `create_bootstrapped` 963, `builder` 993, `bootstrap` 1137. `config.rs` 799: `TorClientConfigBuilder::from_directories(state_dir, cache_dir)`; permission checks through `Mistrust`, line 228.
- **Tokio traits.** VERIFIED tor-proto `client/stream/data.rs`: `DataStream` implements futures `AsyncRead` and `AsyncWrite` (724, 745), and tokio's under feature `tokio` (735, 762). This is why no shim is needed.
- **arti-hyper: AVOID.** VERIFIED README of 0.19.0 (2024-06-05, hyper 0.14): "**THIS CRATE IS OBSOLETE AND UNMAINTAINED**", "does not need this shim crate", "will not receive security patches". Use hyper directly: `connect`, then `TokioIo::new`, then `http1::handshake`.
- **reqwest: AVOID on this plane.** VERIFIED `async_impl/client.rs` 2475 `connector_layer` needs a `Service` returning `Conn`, and `connect.rs` 65 keeps the boxed connector `pub(crate)`. INFERRED: a `DataStream` cannot be injected. Keeping reqwest out of the use-plane crate also enforces the two-stack rule.
- **Client vanguards.** Feature `vanguards`. VERIFIED example config 845-859: `auto` means lite, or disabled if the feature is off. Feature matrix lists client vanguards as implemented since 1.2.2.
- **Client circuit padding.** tor-proto `circ-padding` is `__is_experimental`, with no arti-client feature and no mention in tor-hsclient. INFERRED: no circuit padding on onion client circuits in a normal build. Channel padding exists (example config 520-531).

**hs-pow: exists, marked experimental, untested live.**
- VERIFIED source: `pow.rs` selects `pow/v1_stub.rs` without the feature, which "never offers a solution". `pow/v1.rs` has a solver thread. `connect.rs` 986 logs "failing to compute proof-of-work, trying without", and 1093 calls `increase_effort`.
- VERIFIED `arti-client/Cargo.toml` 147-150: `hs-pow-full` includes `__is_experimental`. It pulls `equix` 0.8.0, LGPL-3.0-only (newest is 0.8.1, 2026-11-04).
- VERIFIED https://onionservices.torproject.org/dev/implementations/ (updated 2026-08-19): client PoW "Implemented (1.3.0)". The docs page saying "Not yet, as of July 2025" is stale against source. `red_team.md` line 394 listed this as open, and it can now close once a flood test confirms it.
- Without the feature, the wallet works normally and gets no priority during an attack.

**Maturity and limits.**
- VERIFIED https://arti.torproject.org/FAQs (updated 2026-07-23): "As of January 2026: Arti is ready for use as a proxy or by developers interested in embedding Tor support in their Rust projects"; it "does not have all of the functionality of C tor".
- VERIFIED README: "This library can call `exit(1)` and terminate your process if it receives a network consensus document telling it that it is obsolete"; this "may occur while Arti is running"; "prepared to upgrade it over time, or it might stop working entirely". Lower-level crates are "even more unstable".
- **Reliability.** INFERRED from fork `tests/shadow/run`, which requires only 5 of 10 for `articlient-onion`: "arti-client onion-service transfers have tended to be a bit less reliable ... still seeing some occasional failures" (arti issue 2109). `torclient-onion-artiserver` requires 10 of 10. This fits `spike_tor.md`: 3 of 42 failures plus a 120 s hang. Simulated conditions may exaggerate it.
- **Latency.** `spike_tor.md`: fresh-circuit first byte median 4.33 s against 1.37 s on reuse; cold start 5.3-15.1 s.
- **Audit.** No dedicated audit of arti-client found: UNKNOWN. A search surfaced a 7ASecurity Tor audit, posted January 2026, covering "the code ... that changed during this project" in network-health tooling; its page mentions Arti, but I did not read the report. Absence of a result is not proof of absence.
- **Simpler alternative.** arti-ureq 0.48.0 (2026-11-04) is maintained, but it is a second HTTP stack. INFERRED: blocking, so a poor fit for the daemon.

## 5. Test tooling

**Plainly: there is no simple, good way to run this end to end without the live network.** The realistic option is Chutney, and it costs days rather than minutes. Tiers:

1. **Most tests: no Tor.** Run gate and wallet logic over a transport trait with plain Unix sockets. Cheap and deterministic, but it tests nothing about Tor.
2. **Smoke test: live network.** C tor service plus wallet on a dev machine, as `spike_tor.md` did. This takes minutes to set up but cannot be deterministic.
3. **Chutney, for what must not touch the live network.** PoW floods, `MaxStreams` abuse, and the Sandbox and PoW interaction. Flooding is not acceptable on real relays.

**Chutney evidence.**
- **Use GitLab, not the GitHub mirror.** The mirror calls itself "unofficial" and is frozen apart from a 2026-03-23 README commit. VERIFIED: its `TorNet.py` writes `authorities = [{name, v3ident}]`, while Arti 2.8's example config uses `[tor_network.authorities] v3idents = [[...]]`. INFERRED: mirror output will not load in Arti 2.8.
- **Pin.** Fork `.gitlab-ci.yml` line 44: `CHUTNEY_COMMIT: d6659ac34c36122ed2590714e37269d1b983c336`, "main @ 2026-07-01". It installs through pip from GitLab. I could not read that commit.
- **Upstream's native job is off.** Fork `.gitlab-ci.yml` 678-682: `integration-chutney` has "Job never runs. See arti#810". `integration-chutney-shadow` (line 703) has no such rule, so upstream appears to run Chutney inside Shadow only (INFERRED). Why native is off: UNKNOWN.
- **Network shape.** Fork `tests/chutney/network.py`: 4 authorities plus "at least 6 non-Authority relays", because "A hidden service ... using vanguards-lite needs 2 guards, and 4 vanguards. Authorities are generally disqualified because they are Unmeasured". It has a C tor service node (`hst`, `NodeBackend.TOR`) and Arti clients (`NodeBackend.ARTI`). So `hs-v3-min` (3 authorities, 3 relays) is probably too small (INFERRED).
- **Wallet wiring.** The harness runs the `arti` binary, and our wallet embeds Arti, so it needs a test-only config path. `config.rs` 886-903 shows `tor_network().authorities()` and `set_fallback_caches` on the builder. `allow_local_addrs` is at `config.rs` 925.
- **Still needed.** C tor's `Makefile.am` 300-301 requires Chutney for `test-network`. Its README says it "isn't ready for prime-time". The dev machine is macOS, so run it in a Linux VM (INFERRED).
- **Licence.** VERIFIED `LICENSE`: "You may do anything with this work ... so long as you retain the above notice(s)".
- **Flood test idea.** Set a low `HiddenServicePoWQueueRate`, flood, and look for Arti's debug line `beginning solve` (`pow/v1.rs`).

**Shadow: AVOID for the PoC.** shadow/shadow v3.3.0 (2025-10-16), last commit 2026-11-02, not archived. Licence is BSD-style text in `LICENSE`. Arti's own `tests/shadow` and `integration-e2e-shadow` use it, and that script says it collapses idle time and reduces flakiness. Fork CI builds it from source at commit `412c4700` plus `tgen`. Reconsider it only if native Chutney proves too flaky.

## 6. Linux hardening and Verda networking

**Do not use the Debian unit or defaults as they are.** VERIFIED `debtor/noble.diff` 6278: `tor-service-defaults-torrc` sets `ControlSocket /run/tor/control GroupWritable`, `SocksPort unix:/run/tor/socks WorldWritable`, `SocksPort 9050` and `CookieAuthentication 1`. Line 5635-5671: `tor@default.service` uses `Type=notify`, `NoNewPrivileges`, `PrivateTmp`, `PrivateDevices`, `ProtectHome`, `ProtectSystem=full`. Line 6332: AppArmor allows owner `/var/lib/tor/**` and specific `/run/tor` files, so a socket elsewhere may need a rule (INFERRED). Write our own unit and torrc, and mount tmpfs at `/var/lib/tor`.

**Sandbox versus PoW.**
- VERIFIED `src/lib/sandbox/sandbox.c`: the default action is `SCMP_ACT_ERRNO(EPERM)` (2065); `sb_mprotect` (1336-1350, 1794-1810) has no read-execute case; `connect` is allowed (317); `sb_socket` allows `PF_UNIX` stream and `PF_NETLINK` raw (983-1045).
- VERIFIED `src/ext/equix/hashx/src/compiler_x86.c` 156 `hashx_vm_rx`, and `hs_pow.c` 178-189: `auto` maps to `EQUIX_CTX_TRY_COMPILE`, and 1 maps to `MUST_COMPILE`.
- INFERRED: `Sandbox 1` means interpreted PoW verify, and `CompiledProofOfWorkHash 1` under `Sandbox` fails. The man page says compiled is "about 20x faster".
- VERIFIED systemd.exec: `MemoryDenyWriteExecute=` rejects "mprotect(2) ... with PROT_EXEC set", so it should have the same effect (INFERRED).
- Man page: `Sandbox` "may be incompatible with some versions of libc, and some kernel versions", so test on the exact image. Verify rate under interpreter: UNKNOWN.

**Network confinement.**
- VERIFIED systemd.exec: `PrivateNetwork=` removes host abstract sockets, "however, those located in the file system will continue to be accessible". So the gate can sit in a private namespace and still accept tor's connection on `/run/gate/gate.sock`.
- VERIFIED same page: units can share a namespace with `JoinsNamespaceOf=`. Gate and vLLM can share a private loopback and have no route out. The tor unit stays in the root namespace (INFERRED plan).
- Host-level control is nftables: output policy drop, allow `lo` and `meta skuid` for tor. VERIFIED nft man: "skuid UID associated with originating socket". Input policy drop, nothing listening.
- `IPAddressDeny` is a second layer at most. VERIFIED systemd.resource-control: "These settings will have no effect" without eBPF cgroup support.
- For tor's unit: `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK` (INFERRED from `sb_socket`).

**Verda.** VERIFIED https://docs.verda.com/private-networks/internet-access/ (Beta, updated 27.10.2026):
- Private-only "has no internet access in either direction". So a private-IP VM cannot run an onion service without another machine.
- Public + Private is 1:1 NAT with inbound and outbound access. "Verda does not filter inbound traffic, so run a guest firewall".
- NAT instance is "an ordinary instance you own, billed as one, and it is a single point of failure for the subnet's egress".
- The "Enable internet gateway toggle ... is not yet functional".
- Also: Private Networks with CC is untested (`verda_platform.md` line 240).
- **So choose Public + Private IP** with default-deny nftables. This revises the guess at `verda_browser_notes.md` line 28 that no public IP might suit an onion service.

**"Tor-only egress" is not achievable at boot.** Things needing other egress:

| Need | Destination | Source | Handling |
|---|---|---|---|
| GPU check, `--verifier local` | `rim.attestation.nvidia.com`, `ocsp.ndis.nvidia.com` | `spike_attestation.md` lines 237, 268-269 | Boot stage, or `--rim-url` and `--ocsp-url` to a mirror (untested) |
| `--verifier remote` (Verda's sample) | `nras.attestation.nvidia.com` | `verda_browser_notes.md` line 66; `spike_attestation.md` line 270 | AVOID: documented with an NGC key, which is identity |
| VCEK | `kdsintf.amd.com` | `verda_browser_notes.md` line 70 | Signed data, so it can come through an untrusted path |
| DHCP | Host network | INFERRED | Test under policy drop; static config possible: UNKNOWN |
| Clock | NTP or host | INFERRED, tor needs a roughly correct clock | Secure clock on Verda SNP: UNKNOWN |

## Design impact

1. **Tor source.** Install from `deb.torproject.org`, pinned by SHA-256, with deb and source archived by us. Minimum 0.4.9.14. Do not use Ubuntu universe.
2. **Rotation becomes routine.** Tor has had six releases in 19 weeks. The manifest must allow several measurements with expiry dates, and redeploying must be a normal operation. Set a rule for which releases force it: .16 touched only directory authorities and can be skipped.
3. **Drop the derived onion key from the PoC.** tor generates the key on tmpfs; bind the SHA-256 of `hostname` in `REPORT_DATA`. The address cannot be known at image build, so add a small signed address record separate from the release manifest, delivered the same way to every wallet. Keep the 25-line key writer as a documented fallback.
4. **Own torrc and unit.** `SocksPort 0`, `ControlPort 0`, no control socket, no HUP. Do not use Debian's defaults.
5. **Start with `Sandbox 1`, `MemoryDenyWriteExecute=yes` and `CompiledProofOfWorkHash 0`.** Measure interpreted verify under a Chutney flood. Remove the sandbox pair if tor saturates under flood.
6. **Set `HiddenServiceMaxStreams 8` with `CloseCircuit 1`, and add a global concurrency limit in the gate**, because `MaxStreams` is only per circuit. Tune the PoW queue on 30 vCPUs, not for GPU throughput.
7. **Set wallet Cargo features explicitly:** `tokio`, one TLS backend, `compression`, `onion-service-client`, `vanguards`. Add a test that fails if either of the last two is missing, since both fail quietly or at runtime.
8. **Run Arti only in the daemon, under supervision,** so `exit(1)` cannot take out the TUI mid-session. The daemon needs a first-byte timeout and retry on a new circuit.
9. **Define credit behaviour on retry.** Retry is mandatory given the failure rates, so EC-ACT needs a rule for a spend whose stream died before the first byte. Reusing a spend across circuits would link them (INFERRED; belongs to the credit track).
10. **Use hyper `http1` directly on `DataStream`.** The use-plane crate must not depend on reqwest, and the purchase crate must not depend on arti-client, so Cargo enforces the two-stack rule.
11. **Gate serves HTTP/1.1 over a Unix socket, one request per stream, SSE framing.** No HTTP/2.
12. **Decision for the design owner: no inner TLS in the PoC, which reverses `tor_traffic_analysis.md` line 142.** C tor is in the plaintext TCB until rustls is added in the gate. Require that before real user prompts.
13. **Deploy Verda as Public + Private IP with default-deny nftables,** not private-only and not through a NAT instance.
14. **Reword "tor-only egress" to "tor-only after a measured boot stage".** The stage allows only DHCP, DNS and HTTPS for one attestation UID, then loads the tor-only ruleset before tor starts. Later AMD CRL refreshes must arrive as signed data through an untrusted path, or the wallet's maximum age must cover the uptime.
15. **Publish tor's source with the image,** because the packaged tor is GPLv3 and the TPO pool drops old versions. Keep the gate a separate process, which INFERRED keeps it out of the GPL scope. Enabling `hs-pow-full` also needs an LGPL review for the wallet binary. Neither is legal advice.
16. **Test plan.** Budget 2-3 days for Chutney at the pinned commit, in a Linux VM, with 4 authorities and 6 relays. Flood tests run only there.
