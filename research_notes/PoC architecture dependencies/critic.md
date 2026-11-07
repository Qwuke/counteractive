# Critic review of the PoC dependency notes

Reviewed 2026-11-06 by a reader who did not write the notes. Scope: `attestation_image_deploy.md`, `credentials_purchase.md`, `engine_client.md`, `tor_network.md`, checked against primary sources and against `Anonymous metered private inference/`. Tags: VERIFIED (read or run, source given), INFERRED (my reasoning), UNKNOWN.

**Result.** Function names, versions and licences in the notes mostly reproduce at line level, and I found no invented API. The real faults sit between the notes: a dependency conflict that breaks the wallet's lockfile, a spike that did not test what a note says it tested, an unresolved inner-channel decision, and a voucher replay that appears only when two notes are read together. NVIDIA's terms also add a licence gate that no note has.

## 1. Corrections

Crate paths refer to crates.io tarballs of that version. "Probe" means a manifest of the notes' recommended wallet crates, on which I ran only `cargo generate-lockfile`, `cargo metadata` and `cargo tree`. Nothing was built.

| # | Claim | File | What I found | Source |
|---|---|---|---|---|
| C1 | `rusqlite` 0.41.1 with SQLCipher, ADOPT | `engine_client.md` L21, L157; `credentials_purchase.md` L19, L272 | **Cannot share a lock with `arti-client` 0.48.0.** VERIFIED: `tor-dirmgr` has a non-optional `rusqlite >=0.36.0, <0.41.0`; rusqlite 0.41.1 needs `libsqlite3-sys` ^0.39, which declares `links = "sqlite3"`. The probe failed: "package `libsqlite3-sys` links to the native library `sqlite3`, but it conflicts with a previous package which links to `sqlite3` as well". With `rusqlite = "0.40"` it resolves to 0.40.2 and libsqlite3-sys 0.38.2, shared with tor-dirmgr (VERIFIED `cargo tree`). The `links` check covers the whole lock, so the single workspace at `engine_client.md` L182 holds the storefront to 0.40.x too (INFERRED). The SQLCipher feature also unifies into Arti's directory cache: `openssl-sys` sits under libsqlite3-sys, rusqlite and tor-dirmgr (VERIFIED); Arti's cache presumably stays unkeyed (INFERRED). `credentials_purchase.md` L165 applied this rule to cdk, but no note checked Arti. | `tor-dirmgr-0.48.0/Cargo.toml.orig:93`; `libsqlite3-sys-0.39.1/Cargo.toml:19`; probe |
| C2 | "Toolchain floor is Rust 1.89" | `engine_client.md` L187 | The floor is 1.92. arti-client 0.48.0 declares `rust_version` 1.92, as `tor_network.md` L150 says; equix, hashx and tor-* crates match. VERIFIED. | crates.io API |
| C3 | Smoke test "C tor service plus wallet on a dev machine, as `spike_tor.md` did" | `tor_network.md` L182; leaned on at L138, L172 | **False.** The spike ran `arti proxy` as the service and a second `arti proxy` as the client, with a Python SOCKS client and TCP to the generator; "The C tor fallback was not needed and was not downloaded". So C tor service, embedded `arti-client`, hyper over `DataStream` and a `unix:` target have never run together here. The 3 of 42 failures at L172 came from an Arti service, so we hold no measurement of the chosen pairing. | `spike_tor.md` L33, L51-52 |
| C4 | "Tor versions 0.4.8 and earlier will no longer work on the network" | `tor_network.md` L44 | The quote exists for .15, but .16 reverts it: "Revert back the required protover version for both "Desc" and "Microdesc" from 3 to 2 which is causing <= 0.4.8.x clients to exit(1)". The evidence is stale; the conclusion that frozen images have a limited life still holds. VERIFIED. | tor 0.4.9.16 `ChangeLog` top entry and L41 |
| C5 | Consensus `exit(1)` handled only as an Arti and wallet risk | `tor_network.md` L171, L242 | C tor has the same path: `exit(1); // XXXX bad exit: should return from main.` after `networkstatus_check_required_protocols`. VERIFIED. An onion service runs in client mode (INFERRED). A frozen measured tor can therefore stop with no image change, and the note has no operational response for it. | tor 0.4.9.16 `src/feature/nodelist/networkstatus.c:1829-1842` |
| C6 | `hs-pow-full` "pulls `equix` 0.8.0" | `tor_network.md` L18, L165 | A fresh lock takes equix 0.8.1 and hashx 0.10.1, both LGPL-3.0-only, plus dynasm and dynasmrt 5.1.0, MPL-2.0. The note names only equix. Without the feature none of the three appears (VERIFIED `cargo tree -i`). | probe `Cargo.lock`, `cargo metadata` |
| C7 | "mkosi builds root and hash partitions (EROFS) with `Verity=hash`" | `attestation_image_deploy.md` L167, L170 | Incomplete. The man page scopes `Verity=` to extension images, and `make_disk`'s default definitions create only ESP, BIOS and root, with no verity. A disk image needs our own `RepartDirectories` with `Verity=data` and `Verity=hash`. Low severity; the decision stands. VERIFIED. | mkosi v27.1 `mkosi.1.md:1416-1429`; `mkosi/__init__.py:3610-3678` |
| C8 | nvattest "Apache-2.0"; whether RIM and OCSP fall under the service terms is "UNKNOWN" | `attestation_image_deploy.md` L14, L108 | Resolved: they do, and production use is not free. See section 4. The same terms say NVIDIA collects the IP address and GPU identity, which no note records. VERIFIED. | NVIDIA terms, section 4 |
| C9 | crypto-bigint 0.5.5 counted against voprf; one RUSTSEC exception named | `credentials_purchase.md` L85, L103, L301; `engine_client.md` L29 | The recommended wallet has it anyway: crypto-bigint 0.5.5, elliptic-curve 0.13.8, p384 0.13.1, `sev` 8.0.0 `crypto_nossl`. RUSTSEC-2026-0387 is patched only from 0.7.6. rsa 0.9.10 (RUSTSEC-2023-0071, `patched = []`) also arrives through `sev`, tor-llcrypto, tor-key-forge and ssh-key-fork-arti. VERIFIED `cargo tree`. So `cargo-deny` should fail on its first wallet run unless both are excepted (INFERRED; I did not run it). Use looks verify-only (INFERRED). | probe; RustSec advisory-db |
| C10 | axum `Listener` "lets an Arti-backed listener plug in"; `TokioIo` "INFERRED, not read" | `engine_client.md` L138-139 | Stale against `tor_network.md` L130: the gate takes connections from C tor on a Unix socket and has no Arti. Arti stays client-side, where hyper is enough. `tor_network.md` L135 marks the `TokioIo` point VERIFIED. | notes as cited |
| C11 | "Socket permissions allow only the gate" | `engine_client.md` L232 | vLLM binds the Unix socket with no chmod, and I saw no chmod or umask under `vllm/entrypoints` (VERIFIED, grep). The mode comes from the process umask, so the unit needs `UMask=` or a restricted parent directory (INFERRED). | vLLM 0.33.0 `vllm/entrypoints/launchers/launcher.py:358-361` |
| C12 | vLLM licence "Apache-2.0" | `engine_client.md` L7, L77 | True of vLLM, but the environment brings proprietary NVIDIA wheels. torch 2.14.0 requires `cuda-toolkit==13.0.3`, `nvidia-cudnn-cu13==9.24.0.43` and others. Current `nvidia-cuda-runtime` 13.5.37 and `nvidia-cublas` 13.10.0.20 declare `LicenseRef-NVIDIA-Proprietary` (VERIFIED). The exact pinned wheels were not read; same terms INFERRED. | PyPI JSON |
| C13 | EHBP "Nothing on Verda", "Solves a router problem we do not have" | `attestation_image_deploy.md` L23, L150 | The reasoning assumes an inner channel that ends in the gate, which `tor_network.md` L130 removes. Once inner encryption is required (L16, L246), EHBP is a candidate, not a non-starter. See section 3. | `SPEC.md`, `tor_network.md` |

## 2. Contradictions

| # | Topic | One side | Other side | Consequence |
|---|---|---|---|---|
| X1 | Inner channel | `tor_network.md` L84, L130, L140-144, L246: no inner TLS in the PoC, and this reverses `tor_traffic_analysis.md` L142 | `attestation_image_deploy.md` L236: "Prompt secrecy rests on the session key in REPORT_DATA"; "the wallet checks attestation inside the tunnel". L150: "the inner channel ends in the gate" | The attestation note's sealing argument rests on a key that the Tor note removes. With no session key, C tor holds plaintext. L236 needs rewriting once the owner decides. |
| X2 | `REPORT_DATA` contents | `tor_network.md` L84, L237: SHA-256 of `hostname` | `attestation_image_deploy.md` L118: `H(evidence)`; L236: session key. `red_team.md` L180 and `spike_attestation.md` L182: per-session nonce | Four claimants and no defined layout for 64 bytes. A report with only the address hash is static per boot and carries no client nonce, which the earlier research said liveness needs. |
| X3 | Gate network | `tor_network.md` L209-210: gate in `PrivateNetwork`, no route out; L248 gives the boot stage to "one attestation UID" | `attestation_image_deploy.md` L71: "the gate ... fetches" KDS; L242: "the gate needs direct egress" | Only workable if a separate pre-gate unit fetches and hands over files (INFERRED). Neither note says so. |
| X4 | Spent vouchers across image updates | `credentials_purchase.md` L299: spent-nonce store "for each key's life"; L280 defers storage as UNKNOWN | `attestation_image_deploy.md` L232-233, L247: sealed under MEASUREMENT, a new image loses the key. `tor_network.md` L236: rotation is routine | **If the store is sealed the same way, each image update forgets spent nonces while the voucher key survives in the storefront, so redeemed vouchers redeem again (INFERRED).** L233 covers lost balances, not this. Host rollback (L235) stays separately open. |
| X5 | Voucher public keys in the gate | `credentials_purchase.md` L299: SPKIs ship in the release manifest for wallets; "the gate holds public keys only" | No note says how the gate gets them: in the image, or by verifying the manifest | In the image, a voucher key rotation means a new measurement (INFERRED). |
| X6 | Verda network mode | `attestation_image_deploy.md` L199: "legacy public IP" | `tor_network.md` L25, L221, L247: "Public + Private IP" | `verda_browser_notes.md` L28 lists these as distinct. Whether Private Networks work with CC is untested (`verda_platform.md` L240), and Verda's doc places Public + Private with Private Networks (beta). Legacy public IP is the one without that dependency (INFERRED). |
| X7 | Onion PoW | `tor_network.md` L12, L122: service PoW ADOPT, in "the only filters ahead of the credit check" | L18, L241: `hs-pow-full` off in wallets | Without the feature the client is a "Stub client for the `v1` scheme which never offers a solution" (VERIFIED `tor-hsclient-0.48.0/src/pow/v1_stub.rs`, selected at `pow.rs:3`). In a flood, real wallets send no proof while an attacker can solve, so PoW may hurt real users (INFERRED). L18 defers this but L12 does not state the limit. |
| X8 | Gate to engine link | `tor_network.md` L210: shared private loopback through `JoinsNamespaceOf=` | `engine_client.md` L232: Unix socket, `PrivateNetwork=yes` | A filesystem socket crosses namespaces (`tor_network.md` L209), so the shared loopback is unneeded. Drop it. |

## 3. Components nobody considered, and existing combinations

**Combination.** I found no project combining blind vouchers, per-token metering, an onion service and SNP GPU attestation. One further search and `existing_systems.md` agree. That is absence of evidence, not proof.

| Project | Licence | Overlap | Verdict |
|---|---|---|---|
| [Possums](https://github.com/ajbt200128/possums) | No licence file found, so not reusable without permission (INFERRED) | Mostly Rust, last commit 2026-10-29. Tor mode through SOCKS, onion identity bound in a nonce-signed endorsement, TLS peer keys checked before headers. In `existing_systems.md` L44, not in the dependency notes. | Closest in shape. Read it as design input; ask the author about a licence before copying. |
| [Onera](https://github.com/onera-app/onera) | AGPL-3.0 (README, LICENSE present) | SEV-SNP, vLLM, Noise NK from browser to enclave, OpenAI-compatible | New to the notes. Design reference only; no code reuse under a permissive wallet. |
| [LLM-Tor](https://github.com/prince776/LLM-Tor) | AGPL-3.0 | Payment plus blind tokens, clearnet proxy through a Tor exit; TypeScript and Go; last commit 2026-03-21 | Design reference only. |
| [Routstr](https://github.com/Routstr/routstr-core) | GPL-3.0 | Metered inference proxy | AVOID as code. |
| [OpenPCC](https://github.com/openpcc/openpcc) | Apache-2.0 | Go, private inference | Already in the earlier research. No change. |

**Pieces that would replace or shorten our own code**

| Component | Status | What it could replace | Verdict |
|---|---|---|---|
| Trustee `deps/verifier/src/snp/mod.rs` | Apache-2.0, Rust, handles Turin with `fmc`; `fetch_vcek_from_offline_store` :188, `verify_report_tcb` :582, `get_processor_generation` :687. VERIFIED | The model for the wallet's policy layer. Notes call Trustee "good reading" and lean on Genoa-only tinfoil-rs (`cert_chain.rs:74,87,298-307,527-536`) | **Make it the primary reference.** It also avoids the AGPL derivation question over tinfoil-rs (`attestation_image_deploy.md` L21). Do not depend on it: 862-package lock per the note. |
| [Project Oak](https://github.com/project-oak/oak) `oak_session`, `oak_attestation_verification` | Root LICENSE Apache-2.0, per-directory not checked. Last commit 2026-11-06. `src/platform.rs` handles Milan, Genoa and Turin (L112-123). `oak_session` has `attestation`, `handshake`, `session_binding` modules. Attested Noise per [FOSDEM 2025 slides](https://fosdem.org/2025/events/attachments/fosdem-2025-5739-attested-noise-protocol-for-low-tcb-trusted-execution-environments/slides/237306/Noise_Pro_D9TrNKZ.pdf). Bazel (`MODULE.bazel`), no `Cargo.toml` in `oak_attestation_verification/`, neither crate on crates.io. VERIFIED | Inner channel plus report binding | Reference only. Depending on it means vendoring out of a Bazel tree. I read file headers, not the protocol. |
| [EHBP](https://github.com/tinfoilsh/encrypted-http-body-protocol) `tinfoil-ehbp` 0.5.0 | MIT. HPKE X25519, streaming chunked bodies, response keys derived from the request context. Crate describes itself as a "Rust client", is built on reqwest and is not on crates.io; I saw no Rust server. VERIFIED (`SPEC.md`, `rust/Cargo.toml`, `src/lib.rs`) | Inner encryption with no handshake round trip (INFERRED), and the same mechanism as the later Tinfoil backend | Candidate when inner encryption is added. Limits: gate side is ours, the wallet must use hyper not reqwest, headers stay visible to C tor, bodyless requests get unencrypted responses (`SPEC.md` L147). Static-key forward secrecy not assessed. |
| `snow` 0.10.0 | Apache-2.0 OR MIT, 2025-07-19. Audit status UNKNOWN | Noise NK keyed from `REPORT_DATA`, avoiding the X.509 custom verifier in `tor_network.md` L144 (INFERRED). Onera uses NK; Confer uses Noise Pipes (`existing_systems.md` L220) | Candidate. |
| `rustls` 0.23.46 | Already in the wallet lock through Arti and reqwest. VERIFIED | The note's own plan, listed as "not surveyed" (L16) | Default choice for the boring option (INFERRED): no new crate in the wallet. |
| SNPGuard | MIT, v0.1.3, last commit 2024-11-03; README: "technical demo and NOT intended to be used in any kind of production" | SNP image tooling | Reference only. In `tee_gpu_attestation.md`, absent from the dependency notes. |

Seen but not read further, so no verdict: nearai/private-ml-sdk (MIT, last commit 2025-10-20), microsoft/attested-ohttp-client (2026-04-13), flashbots/cvm-reverse-proxy (v0.1.9, 2025-11-20), opensecretcloud/opensecret.

## 4. Licence conflicts

Not legal advice. The questions here are for counsel.

**NVIDIA attestation: conflict for any paid service.** The code is Apache-2.0, but the terms for the services and suite are separate.

- VERIFIED [cloud services licence page](https://docs.nvidia.com/attestation/cloud-services/latest/license.html), updated 2026-10-06: without an Enterprise Product license, use is "solely for the purposes of development of a confidential computing service, not a commercial offering / redistribution. A commercial Enterprise Product license must be obtained before offering the software within a paid commercial service."
- VERIFIED [Attestation Cloud Suite R&D terms](https://www.nvidia.com/en-us/agreements/cloud-services/service-specific-terms-for-cloud-attestation-dev/), 2025-01-06, covering "Reference Integrity Manifest, Certification Revocation Services and Remote Attestation Services": "internally for test, development and research purposes, not in production." Also: "NVIDIA will collect IP address and information about Customer's GPU (including device unique identity key pair and device certificates that uniquely identify Customer's GPU)".
- VERIFIED [Confidential Computing product terms](https://www.nvidia.com/en-us/agreements/enterprise-software/product-specific-terms-for-confidential-computing/), 2026-09-04: "Development use of NVIDIA Confidential Computing is free."; "A commercial Enterprise Product license must be obtained before offering or distributing a Customer Product."
- The PoC fits as development (INFERRED). Price, and whether Verda's contract covers tenants: UNKNOWN.

**NVIDIA driver and CUDA: conflict for distributing an image.**

- VERIFIED [driver licence in Ubuntu's `debian/copyright`](https://changelogs.ubuntu.com/changelogs/pool/restricted/n/nvidia-graphics-drivers-580/nvidia-graphics-drivers-580_580.126.09-0ubuntu0.24.04.2/copyright): deploy "for your own use ... on infrastructure you own or lease" (1.1c); redistribution of unmodified binaries with the agreement (1.1d); no reverse engineering (2.2); "you may not sell, rent, sublicense, distribute or transfer the SOFTWARE or provide commercial hosting services with the SOFTWARE" except as granted (2.7). Which licence covers Verda's actual driver: UNKNOWN. 2.7 needs reading against a paid service.
- CUDA EULA v13.5 (https://docs.nvidia.com/cuda/eula/index.html): redistributable files only, no stand-alone distribution, no reverse engineering. Read through a summary tool only, not verbatim, so confirm before relying on it.
- 2.2 may also constrain image auditors (INFERRED).

**GPL and LGPL**

| Item | Licence | Effect |
|---|---|---|
| tor, as packaged | GPLv3 (`--enable-gpl`) | Source obligation if distributed. `tor_network.md` L249 covers this, but a binary image also triggers it for the kernel and other GPL userland, which the note omits (INFERRED). Running on rented Verda without publishing the image is not the concern (INFERRED). |
| equix, hashx | LGPL-3.0-only, static in Rust | Only with `hs-pow-full`. Fine if wallet source is public and rebuildable; a problem for closed distribution (INFERRED). Interacts with X7. |
| dynasm, dynasmrt | MPL-2.0 | Only with `hs-pow-full`. File-level copyleft (INFERRED no impact if unmodified). |
| option-ext 0.2.0 | MPL-2.0 | Always present, through `dirs-sys`. Same. |

**AGPL.** None in the recommended stack. Do not copy from `tinfoil-go/verify/`, cvmimage, Onera or LLM-Tor. tinfoil-rs's possible derivation from AGPL Go (`attestation_image_deploy.md` L21) stays unresolved, so treat it as read-with-care.

**Wallet graph.** VERIFIED `cargo metadata`, 755 packages across all platforms: no GPL or AGPL. Other non-MIT/Apache entries needing notices: CDLA-Permissive-2.0 webpki-root-certs, BSL-1.0 xxhash-rust, WTFPL terminfo, plus bundled SQLCipher and vendored OpenSSL (`openssl-src` 300.6.1+3.6.3). r-efi and termina offer a choice of licence. `arti-client-0.48.0/Cargo.toml.orig:71` lists "Features which may introduce unnecessary licensing restrictions" among those `full` leaves out. `deny.toml` must allow LGPL only under the feature flag.

**Python image closure.** No licence scan by me; see section 6.

## 5. Spot checks that reproduced

All VERIFIED by me from the primary source named.

| Claim | File | Source |
|---|---|---|
| tor 0.4.9.16; tarball SHA-256 `6180e193...`; TPO deb `0.4.9.16-1~noble+1` | `tor_network.md` L5 | dist.torproject.org, TPO `Packages` |
| PoW module requires `--enable-gpl` | `tor_network.md` L47-48 | `configure.ac` 57-62, 417-424 (note says 416-425) |
| Which file forces the GPL is UNKNOWN; `src/ext/equix` has no licence text | `tor_network.md` L52 | tarball |
| `Sandbox`, `CompiledProofOfWorkHash`, PoW and IntroDoS defaults | `tor_network.md` L107-109, L204 | `tor.1.txt` |
| arti-client 0.48.0 date, features, not-default onion and vanguards | `tor_network.md` L150-151 | crate, `Cargo.toml` |
| `blind-rsa-signatures` 0.18.0 API, `Secret` public with derived `Debug`, no zeroize | `credentials_purchase.md` L51 | `src/lib.rs` |
| `rsa` rc.21 date; RUSTSEC-2023-0071 `patched = []` | `credentials_purchase.md` L6, L72 | crate, advisory-db |
| `sev` 8.0.0 API, `crypto_nossl` `Verifiable` impl, parser notice | `attestation_image_deploy.md` L7, L54 | `src/firmware/guest/types/snp.rs` |
| sev-snp-measure vCPU signature 26/0/0 | `attestation_image_deploy.md` L10 | `vcpu_types.py:44` |
| nvattest and libnvat 2.0.0 package hashes, default URLs, OCSP fails closed | `attestation_image_deploy.md` L14, L106 | NVIDIA `Packages`, `utils.cpp`, `nv_x509.cpp:544-548` |
| sigstore-rs leaves inclusion and SET checks as TODO | `attestation_image_deploy.md` L19 | `src/bundle/verify/verifier.rs:236,240` |
| tinfoil-rs Genoa-only; tinfoil-go `verify/` AGPL | `attestation_image_deploy.md` L21-22 | `cert_chain.rs`, `verify/LICENSE` |
| Stripe `allowed_payment_method_types` present, email sentence | `credentials_purchase.md` L196, L226 | Stripe OpenAPI `2026-10-28.endive` |
| vLLM 0.33.0, `torch==2.14.0`, `cuda-toolkit==13.0.3`, flashinfer pin | `engine_client.md` L37, L42, L77 | PyPI JSON, sdist |
| vLLM usage, metadata and UDS flags as cited | `engine_client.md` | `api_utils.py:273-285`, `envs.py`, `arg_utils` |
| SQLCipher feature text, `SQLITE_TEMP_STORE=2` | `engine_client.md` L157 | rusqlite `README` 123-126 (note says 122-126), `build.rs:192` |
| Open WebUI fifty-user clause; Qwen3-0.6B commit `c1899de289a0` | `engine_client.md` L170, L109 | LICENSE, Hugging Face |

Minor: `nv_x509.cpp` lives under `nv-attestation-sdk-cpp/src/`.

## 6. Not re-checked

- RFC quotes, AMD ABI quotes, Chutney and Shadow claims, Verda CC docs beyond the internet-access page and OpenAPI, tor GitLab reachability, reqwest's connector claim, the Python closure licence scan.
- Whether anything compiles. Resolver only. `blind-rsa-signatures` 0.18.0 against rsa rc.21 stays UNKNOWN (`credentials_purchase.md` L55).

**Method and deviations.** Downloads went to `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/dep_survey/critic/`. No downloaded code was run. One curl loop wrote temporary licence files to `/tmp` outside that directory and removed them. Some NVIDIA pages were first read through a summary tool; quotes above were then confirmed with curl, except the CUDA EULA.

## 7. Design impact

1. **Pin `rusqlite = "0.40"` across the workspace** until `tor-dirmgr` widens its range, and put the workspace lock in CI from the first commit.
2. **Set `rust-toolchain.toml` to 1.92.**
3. **Make the first integration test C tor service, embedded `arti-client`, hyper and a `unix:` target,** because no spike has run that pairing. Measure failure rates there.
4. **Owner decision: inner channel.** No inner encryption is acceptable for the PoC only. Before real prompts pick rustls, Noise NK or EHBP; rustls is the default because it adds no wallet crate (INFERRED). Rewrite `attestation_image_deploy.md` L150 and L236 to match.
5. **Define `REPORT_DATA` once:** a 64-byte hash, with domain separation, over version, onion address, `H(GPU evidence)`, wallet nonce and any channel key. Decide whether a report is per session or per boot, since the nonce requirement forces per session.
6. **Fix voucher replay across image updates.** PoC rule: tie each voucher key to an image release, so a new image rejects old keys and unredeemed test vouchers are voided. Keep the rollback limit documented. Define how the gate gets voucher public keys.
7. **Put boot-time KDS, RIM and OCSP fetches in a separate unit** with its own UID, run before the tor-only ruleset. The gate stays in `PrivateNetwork` and reads files.
8. **Add a liveness alarm and redeploy path for C tor exit on consensus.** Restart will not help; only a new image will (INFERRED).
9. **Resolve PoW:** enable `hs-pow-full` in the wallet after the flood test and LGPL review, or stop listing PoW as a filter that protects real wallets.
10. **Choose one Verda mode.** Use legacy public IP unless Public + Private is tested on a CC SKU.
11. **Add an NVIDIA licence gate before any paid service,** with Enterprise terms in writing and Verda's position asked for. Record the IP and GPU identity collection in the threat notes.
12. **Publish recipe, lock, hashes and measurements, not the binary image,** unless counsel clears distribution. Auditors rebuild and fetch NVIDIA parts themselves.
13. **Add `cargo-deny` exceptions** for RUSTSEC-2023-0071 and RUSTSEC-2026-0387 in the wallet, dated, with a note on verify-only use once that is checked.
14. **Use Trustee's SNP verifier as the wallet's Turin reference** instead of tinfoil-rs.
15. **Add `UMask=` or a restricted directory for the vLLM socket,** and drop the `JoinsNamespaceOf=` plan.
