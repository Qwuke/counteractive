# 02. Third-party dependencies

This document lists what we adopt, what we write ourselves, and what we rejected, with the reason for each. Versions were read from crates.io, PyPI, GitHub and vendor pages on 6 November 2026, and the list was revised on 7 November 2026 to match the decisions in [`ARCHITECTURE.md`](../../ARCHITECTURE.md#2-decisions). Work-saved and line figures are my estimates, not measurements. Detailed evidence is in [`research_notes/PoC architecture dependencies/`](../../research_notes/PoC%20architecture%20dependencies/).

**The full set hasn't been compiled together.** The one check against it was a dependency resolve ([section 7](#7-what-the-resolver-showed)), made before the decisions. What the local prototype builds is recorded in [ARCHITECTURE section 14](../../ARCHITECTURE.md#14-validation-status). **This list isn't an allow-list.** M1 generates `deny.toml` from the real lock.

## 1. Adopted: credentials and purchase

| Dependency | Version | Licence | Saves us | Main risk | Wrapped by |
|---|---|---|---|---|---|
| `act-rs` (git, library name `act`) | rev `0fe1d8f7`, crate version 0.1.0 | Apache-2.0 | The whole EC-ACT protocol on P-256: blind issuance, spends and refunds. Months of cryptography. | Draft protocol with unfinished security arguments; little outside review | `credit` |
| `ihat-rs` (git, under `act-rs`) | 0.1.0, as locked | MIT OR Apache-2.0 | Range and arithmetic proofs | Least reviewed part of the graph | Not used directly |
| `hkdf`, `chacha20poly1305` | Not yet pinned | MIT OR Apache-2.0 | HKDF-SHA256 and XChaCha20-Poly1305 for sealing stored refunds | Low | `gate` sealing |
| `sha2` | 0.11.1 | MIT OR Apache-2.0 | SHA-256 and SHA-512 for order references, secret hashes, contexts and report data | Low | Throughout |
| Stripe Checkout (hosted) and API, sandbox | API `2026-10-28.endive` | Hosted service | Card UI, 3-D Secure, SAQ A scope: weeks plus PCI work | Stripe always holds card identity and email; stored value is a restricted category, so live keys need its approval | `storefront/stripe.rs` |

`act-rs` was tested on a laptop. At L=24: spend message 3,499 B, prove about 5.7 ms, verify about 7 ms, reject a duplicate about 158 µs. An audit of 113 registry packages against OSV found no advisories. The pinned commit's GitHub signature couldn't be verified. Full results are in [`spike_act_rs.md`](../../research_notes/Anonymous%20metered%20private%20inference/spike_act_rs.md).

## 2. Adopted: Tor and network

| Dependency | Version | Licence | Saves us | Main risk | Wrapped by |
|---|---|---|---|---|---|
| C `tor` (from `deb.torproject.org`) | 0.4.9.16 (minimum 0.4.9.14) | BSD source, GPLv3 as packaged with `--enable-gpl` | The onion service, proof-of-work, introduction limits. Not realistic to build. | Six releases in 19 weeks; each one we take changes the measurement. Can `exit(1)` on a consensus decision. | `image/` config |
| `arti-client` | 0.48.0 (Arti 2.8.0) | MIT OR Apache-2.0 | The entire Tor client | Can `exit(1)` on consensus; onion reliability; features fail quietly if missing; needs Rust 1.92 | `gate-conn`, behind its `arti` feature |
| `hyper` | 1.12.0 | MIT | HTTP/1.1 over any byte stream | We own timeouts and the connection task | `backend`, `gate-conn` |
| `hyper-util`, `http-body-util` | 0.1.21, 0.1.5 | MIT | `TokioIo`, `TowerToHyperService`, body helpers | Low | Same |
| `axum` | 0.8.9 | MIT | Routing, streaming bodies, SSE, body limits | Breaking changes are queued on main; on 0.8.9 we must set header timeouts ourselves | `gate/api`, `storefront/api`, `walletd/local_api` |
| `rustls` | 0.23.46 | Apache-2.0 OR ISC OR MIT | TLS 1.3 with hybrid X25519MLKEM768 by default on `aws-lc-rs`, for the inner channel on both planes. Already in the wallet graph. | Custom verifier uses the `dangerous()` builder; `aws-lc-sys` needs a C toolchain | `gate/tls.rs`, `storefront/tls.rs`, `gate-conn` |
| `tokio-rustls` | 0.26.6 in the lock; source read at 0.26.4 | MIT OR Apache-2.0 | Async TLS over Unix sockets, TCP and Arti streams | Low | Same |
| `rcgen` | 0.14.10 in the lock, source read at 0.14.7; `aws_lc_rs` instead of default `ring` | MIT OR Apache-2.0 | Self-signed per-boot certificate for the gate and the storefront | Default features pull a second crypto library into the gate. The resolver showed this. | `gate/tls.rs`, `storefront/tls.rs` |
| `reqwest` | 0.13.5, `rustls`, `.no_proxy()` | MIT OR Apache-2.0 | HTTPS client with system DNS, for the storefront's calls to Stripe | System proxy is on by default | `storefront/stripe.rs` only. Neither wallet process nor the gate uses it. |
| `aws-lc-rs` | 1.19.0 in the lock | ISC AND (Apache-2.0 OR ISC) | The crypto provider under `rustls` and `rcgen` | C build | Provider only |

**Arti features must be explicit:** `tokio`, `rustls`, `compression`, `onion-service-client`, `vanguards`. Without `onion-service-client`, `.onion` fails at runtime. Without `vanguards`, Arti silently disables them. A test fails if either is missing. [read]

**`hs-pow-full` stays behind our own Cargo feature, off by default.** It's marked experimental and pulls `equix` and `hashx` (LGPL-3.0-only) and `dynasm` (MPL-2.0). The cost of leaving it off: under a flood, real wallets send no proof while an attacker can solve puzzles, so service-side proof-of-work may hurt real users during an attack. We state this limit and decide after a flood test. [read for the stub behaviour, inferred for the effect]

## 3. Adopted: attestation, release, image, deployment

| Dependency | Version | Licence | Saves us | Main risk | Wrapped by |
|---|---|---|---|---|---|
| `sev` | `=8.0.0`, `snp`, `crypto_nossl`, no default features | Apache-2.0 | Report parsing, chain and signature checks, guest ioctls, AMD roots: 2 to 3 weeks | Parser API break announced; **we must write policy, dates, TCB and pin checks ourselves** | `attest` |
| `snpguest` | 0.11.0 | Apache-2.0 | Bring-up debugging | Not a library. Used on the first instance and in the bring-up image only. The final image's collector uses `gate snp-ids` instead. | Manual use |
| AMD KDS | Spec 57230 (unread) | AMD service | Wallet never contacts AMD | Rate limit unknown | Boot collector |
| `sev-snp-measure` | 0.0.13 | Apache-2.0 | Launch digest prediction: 4 of 4 real Tinfoil reports reproduced, none yet from Verda | Verda's vCPU signature is disputed (26/0/0 or 26/2/1) | `xtask measure` |
| `sev::measurement` | 8.0.0 | Apache-2.0 | Second implementation in Rust, as a CI cross-check | No Turin test in the crate; cannot express 26/2/1 | `xtask measure` |
| `nvattest`, `libnvat` | 2.0.0.1790976895-1 | Apache-2.0 for code. **Service terms separate.** | SPDM, RIM, OCSP and policy appraisal: months | Needs egress at boot; OCSP failure fails closed; R&D-only service terms | Boot collector |
| `minisign` (C) and `minisign-verify` | 0.12, 0.3.0 | ISC, MIT | Offline signature check in about 10 lines, zero dependencies | Key held by one person; no enforced log | `manifest` |
| `actions/attest`, Rekor | v4.2.2 | MIT | Public log entry per release | Requires a public repo; wallet doesn't check it | CI |
| mkosi | v27.1 (time-boxed to 2 days) | LGPL-2.1-or-later | Snapshots, tools tree, partitions: 1 to 2 weeks | Its verity boot path doesn't fit Verda; needs our own `RepartDirectories` | `image/mkosi` |
| `mmdebstrap` | 1.4.3-6 | MIT-style | Fallback if mkosi fights back | More hand-written stitching | `image/` |
| `modelwrap` | v0.3.0 | MIT | Reproducible dm-verity model packs: about 1 week | Go and Docker at build time; mount side is ours | `image/model` |
| Verda REST API | OpenAPI v1 | n/a | Only surface with all network options | No attestation API; instance behaviour for CC plus custom images is unknown, including inbound ports and IP stability | `xtask deploy` |
| `verda-cli` | v1.10.0 | Apache-2.0 | Manual start, shutdown and list | No network flags | Manual use |

The `attest` policy layer follows **Trustee's `deps/verifier/src/snp/mod.rs`** (Apache-2.0, handles Turin) as its reference, without depending on it. `tinfoil-rs` is Genoa-only and possibly derived from AGPL Go, so read it with care and don't copy from it.

## 4. Adopted: engine and client

| Dependency | Version | Licence | Saves us | Main risk | Wrapped by |
|---|---|---|---|---|---|
| vLLM | 0.33.0, pinned by PyPI hash | Apache-2.0 | The GPU engine: scheduler, streaming, per-chunk usage, token IDs. Months. | Large Python and CUDA trusted base; releases about every 2 weeks; nothing upstream on CC mode | `backend::vllm` |
| PyTorch and NVIDIA wheels | `torch==2.14.0`, `cuda-toolkit==13.0.3` | BSD for torch, proprietary for NVIDIA wheels | CUDA 13.0 from wheels, no system toolkit | Proprietary licences on redistribution | `image/engine/uv.lock` |
| `uv` | 0.13.7 | MIT OR Apache-2.0 | Hash-locked Python environment | FlashInfer kernels sit outside the lock; start with them disabled | `image/engine` |
| Qwen3-0.6B | commit `c1899de289a0` | Apache-2.0 | One small permissive model, native on both engines | Thinking on by default; the gate sets `enable_thinking:false` | Catalogue |
| `llama-server` | Pinned `b` tag | MIT | Optional laptop engine with the same socket interface, for real metering | Differs from vLLM: no exact IDs while streaming | `backend::llamacpp` |
| SmolLM2-135M | Q4_K_M | Apache-2.0 | 105 MB development model | Weak output, fine for tests | Dev config |
| `rusqlite` | `0.40` (**not** 0.41) | MIT; SQLCipher BSD-style | Durable encrypted stores: about 1 week | Pinned by the `links` conflict in [section 6](#6-pins-and-conflict-rules); vendored OpenSSL build | `gate/store`, `wallet-core/journal` |
| `tokenizers` | 0.23.2, `onig` only | Apache-2.0 | Real HF tokenizer in Rust | `onig` is C; used only for a diagnostic | `wallet-core/recount` |
| `minijinja`, `minijinja-contrib` | 3.0.5 | Apache-2.0 | Chat template rendering | First thing to drop if golden tests are awkward | `wallet-core/recount` |
| `keyring` | 4.2.0 | MIT OR Apache-2.0 | OS keychain on three platforms | No store on headless Linux, so there's a passphrase fallback | `wallet-core/keys` |
| `secrecy`, `zeroize` | 0.10.3, 1.9.1 | Apache-2.0 OR MIT | Redacted secrets, wipe on drop | Can't cover copies in SQLite or TLS buffers | Throughout |
| `clap`, `directories` | 4.6.7, 6.0.0 | MIT OR Apache-2.0 | CLI, per-OS paths | Low | `wallet-cli`, `wallet-core` |
| `cargo-deny` | 0.20.2 | MIT OR Apache-2.0 | Licences, bans, sources and advisories in one check | Config upkeep; needs dated exceptions on first run | CI |
| `assert_cmd` | 2.2.2 | MIT OR Apache-2.0 | CLI tests | Low | `tests/e2e` |
| Chutney | commit `d6659ac3` | Permissive | A private Tor network: about a week of work | Upstream's native Arti job is off; 2 to 3 days to wire up | `tests/chutney` |

### Supporting and build-time tools

These do real work without changing the design, so they get less detail. Versions come from the lock or the base image.

| Tool | Used for | Where |
|---|---|---|
| `tokio` | Async runtime | All services |
| `tracing` | Logging, never with request bodies | All services |
| `thiserror` | Typed errors | Library crates |
| `dropbear-initramfs` | The RAM-only flash stage, on the stock image only | `deploy/` |
| `veritysetup` | Hash trees at build, and opening them at boot | Image |
| `diffoscope` | Explaining a double-build mismatch | Release pipeline |
| Python 3.12 | vLLM runtime | Image |
| systemd-networkd | DHCP during the boot stage | Image |
| `nvidia-persistenced` | Keeps the GPU's SPDM keys alive. NVIDIA's guide requires `--uvm-persistence-mode` in CC. | Image |

## 5. What we write ourselves

Line counts are rough estimates, excluding tests.

| Piece | Lines | Why not a library |
|---|---|---|
| Stripe client: create and retrieve Checkout Sessions | 100 | Two calls. `async-stripe` is in a long RC, and strict parsing could stop fulfilment when Stripe adds a field. |
| OpenAI subset structs and bounded SSE splitter | 160 | Type crates silently drop the vLLM fields we need. `eventsource-stream` has no buffer limit. |
| Attestation policy layer | 300 | `sev` deliberately leaves these checks to the caller. |
| Evidence staple | 100 | Specific to our bundle format |
| Inner TLS verifier and server acceptor, shared by gate and storefront | 150 | Glue over `rustls`. Fewer lines than any wrapper. |
| Result sealing | 40 | Glue over `hkdf` and `chacha20poly1305` |
| Model pack mount | 30 | `veritysetup` with fixed parameters |
| Flash script | 30 | Verda's scripts have no stated licence, and writing our own is easy |
| Verda deploy, in `xtask` | 150 | The SDK and Terraform drop network fields |
| Local page | 300 | Open WebUI's branding clause and missing balance display. No third-party JS. |

Two test crates are ours too, so tests run the real interfaces with no GPU, card or network ([SPEC](../../prototype/SPEC.md)):

- **`fake-engine`:** a deterministic OpenAI-compatible engine on a Unix socket, with vLLM's `/tokenize` shape and fault hooks for a stall, a dropped stream and a long output.
- **`fake-stripe`:** the two Checkout Session endpoints the storefront calls, plus a test pay call, so the storefront's real HTTP client runs against a local server.

Reusing the design, not the code:

- **Trustee:** policy checks.
- **Possums:** closest in shape, but no licence file was found, so read only and ask the author.
- **OpenPCC:** value grid and refund ideas.

## 6. Pins and conflict rules

| Rule | Why | Evidence |
|---|---|---|
| Rust 1.92 or later in `rust-toolchain.toml` | `arti-client` 0.48.0 requires it | [read] |
| `rusqlite = "0.40"` everywhere | `tor-dirmgr` needs `<0.41`, and rusqlite 0.41's `libsqlite3-sys` conflicts through `links = "sqlite3"`. One lock means one version across the workspace. | [resolver] |
| `sev = "=8.0.0"`, no default features | Default features break Apple Silicon builds. `openssl` plus `crypto_nossl` is a compile error. | [read] |
| `arti-client` default features off | Avoids `native-tls`. Features listed explicitly. | [read] |
| `rcgen` default features off, `aws_lc_rs` on | Otherwise `ring` joins the gate's graph alongside `aws-lc-rs`. | [resolver] |
| `rustls` built with an explicit provider | Avoids any process-default ambiguity. | [inferred] |
| vLLM, torch and other Python packages installed with `--require-hashes` | Reproducible image | [read] |
| C tor archived with source | The TPO pool drops old debs, and GPLv3 applies to what we distribute. | [read] |

### Required `cargo-deny` exceptions

Each exception carries a date and a reason, and is reviewed before live money.

- **RUSTSEC-2023-0071 (`rsa` 0.9.10, Marvin).** No patched version exists. `rsa` 0.9.10 arrives through `sev` and Arti in the wallet and gate, and its use there looks verify-only. Nothing we write signs with RSA. [inferred, confirm before first release]
- **RUSTSEC-2026-0387 (`crypto-bigint` before 0.7.6).** Version 0.5.5 arrives through `sev`. Same verify-only reasoning. [inferred]

## 7. What the resolver showed

Before the decisions, I wrote stub manifests for seven crates and ran `cargo generate-lockfile` and `cargo tree`. Six remain (`credit`, `attest`, `gate`, `storefront`, `walletd`, `wallet-cli`). The seventh held a blind-RSA layer that decision 6 removed, and at the time the storefront signed with it and `wallet-cli` used `reqwest`. No code was compiled or run.

| Result | Value |
|---|---|
| Packages in the full lock | 674 |
| Linux target, normal edges | gate 191, storefront 147, `walletd` 530, `wallet-cli` 265. The storefront and `wallet-cli` counts predate decision 6. |
| `reqwest` in `walletd` | Absent |
| `arti-client` in `wallet-cli` | Absent |
| Coexisting versions | `crypto-bigint` 0.5.5 and 0.7.10; `rsa` 0.9.10 beside a release candidate that has since left with decision 6 |
| Strong copyleft (GPL or AGPL) | None in the union |
| Other notices needed | MPL `option-ext`, CDLA `webpki-root-certs`, vendored OpenSSL |
| Unexpected | `ring` enters the gate through `rcgen` defaults, now fixed by a pin |

Whether the full set compiles on the target is **unknown**. `xtask`, `gate-conn`, `crash-point`, `fake-engine` and `fake-stripe` weren't in this resolver run, so the whole-workspace build is part of M4's exit.

## 8. Licence gates

Not legal advice. These are questions for counsel before any paid service.

| Item | What applies | Effect |
|---|---|---|
| NVIDIA attestation services (RIM, OCSP) | Terms say "internally for test, development and research purposes, not in production" | **The PoC fits as development. A commercial licence is needed before a paid service.** NVIDIA also states that it collects IP and GPU identity. |
| NVIDIA CC product terms | Development free; a commercial Enterprise licence is required before offering a product | Same gate. Whether Verda's contract covers tenants is unknown. |
| NVIDIA driver and CUDA | Redistribution limits, no commercial hosting except as granted | **Publish the recipe, lock and hashes, not the binary image.** Auditors rebuild. |
| tor GPLv3 | Source obligation if distributed | Archive source. The gate stays a separate process. |
| Kernel and other GPL userland | Same, if a binary image is distributed | Another reason to publish recipes only |
| `hs-pow-full` | LGPL-3.0-only, static in Rust | Fine if wallet source is public and rebuildable. Reviewed before enabling. |

## 9. Rejected

| Candidate | Reason |
|---|---|
| Cashu / CDK | Alpha, and 343 crates |
| RFC 9577 HTTP auth flow | Credentials are issued at purchase and spent by POST, not presented per request. |
| OHTTP (`ohttp`) | Tor already separates IP from request. No gain here. |
| OpenPCC as code | Go only, TPM-based attestation. Read as design input. |
| `async-stripe` | Long RC, strict parsing risk. Revisit at 1.0. |
| `sqlx`, PostgreSQL | One instance and no need for a server. 162 crates against 22. |
| `ADD_ONION` and control-port crates | Needs a control port, rules out `Sandbox`, has no IntroDoS keywords |
| Tor client authorization | Per-client keys link users. |
| `arti-hyper` | Obsolete and unmaintained, by its own statement |
| `arti-ureq` | Second HTTP stack |
| Shadow | Heavy and Linux-only, for a question Chutney already answers |
| Noise via `snow`, EHBP on Verda | Both viable, but TLS adds no wallet crate. EHBP stays for the Tinfoil pooled path. |
| `sigstore-rs` in the wallet | Bundle verifier leaves inclusion and SET checks as TODOs |
| Own signature format with `ed25519-dalek` | minisign is simpler and has tooling. |
| Trustee or KBS, HashiCorp Vault | Moves custody without hiding the key; large; Vault is BUSL |
| Verda LUKS unlock | SSH in the image; fails open on signature check |
| dstack, Constellation, Contrast, CoCo, cvmimage | Each assumes another host, stack or licence |
| Packer, Nix, Yocto, Buildroot | Not reproducible, or too heavy for Ubuntu-only NVIDIA packages |
| Verda SDK, Terraform | Drop network fields; no start or stop |
| `nvTrust` Python SDK and local verifier | Deprecated by NVIDIA |
| `async-openai`, `openai-api-rs` | Drop vLLM fields; closed finish-reason enums |
| Open WebUI, LibreChat, LobeHub, AnythingLLM | Licence, weight, or telemetry. Users may still point any OpenAI client at `walletd`. |
| `cargo-audit`, `cargo-vet`, `testcontainers` | Duplicate, too early, or needs Docker for nothing |
| Stripe Billing credits | Explicitly not usable as stored value; bound to a Customer |
