# PoC dependencies: attestation, image, deployment

Surveyed 6 November 2026. No implementation yet. Tags: VERIFIED (where it was read), INFERRED (my reasoning), UNKNOWN (not settled). Effort figures are INFERRED estimates.

| component | decision | version | licence | what it saves us | main risk |
|---|---|---|---|---|---|
| virtee `sev` crate | WRAP, pin `=8.0.0` | 8.0.0, 2026-05-27 | Apache-2.0 | Report parsing, chain and signature checks, guest ioctls, derived keys, AMD roots. About 2-3 weeks | Parser API break announced; wallet must still write policy, date, TCB and pin checks |
| snpguest | ADOPT, bring-up tool only | 0.11.0, 2026-10-26 | Apache-2.0 | Debugging, plus a reference for VCEK and TCB checks | Not a library |
| AMD KDS plus staple | ADOPT the service, BUILD-OURSELVES the staple (about 100 lines) | Spec 57230 rev 1.05 (unread) | AMD service | Wallet never contacts AMD | Rate limit UNKNOWN; Verda host certs UNKNOWN |
| sev-snp-measure | ADOPT in CI | 0.0.13, 2026-05-22 | Apache-2.0 | Measurement prediction, already 4/4 on real reports | Verda vCPU signature conflict (26/0/0 or 26/2/1) |
| `sev::measurement` | WRAP as CI cross-check | 8.0.0 | Apache-2.0 | Second implementation, in Rust | No Turin test in the crate; cannot express 26/2/1 |
| attestation-rs | AVOID, read as reference | 0.5.0, unpublished | MIT file, Apache-2.0 manifest | Nothing today | NVIDIA path is NRAS-only; licence conflict |
| dstack, Constellation, Contrast, CoCo, cvmimage, Buildcage | AVOID | see section | see section | Nothing that fits Verda | Each assumes another host, stack or licence |
| nvattest / libnvat (NVAT) | ADOPT in the image | 2.0.0.1790976895-1, 2026-10-06 | Apache-2.0 | SPDM, RIM, OCSP and policy appraisal. Months | Needs clearnet egress; OCSP failure fails closed |
| nvTrust Python SDK, Local GPU Verifier | AVOID | 2.7.3, 2026-05-04 | BSD-3 classifier | Nothing | Deprecated by NVIDIA |
| Wallet-side SPDM check | Defer, BUILD-OURSELVES later | n/a | n/a | Not needed for PoC | Adds parser and Blackwell work for little gain |
| minisign, `minisign-verify` | ADOPT for the manifest | crate 0.3.0, 2026-09-25; C tool 0.12 | MIT; ISC | Offline verify in about 10 lines, no dependencies | Pinned key held by one person; no log enforcement |
| `actions/attest`, Rekor | ADOPT in CI, audit-only | v4.2.2, 2026-08-04 | MIT | Public log entry for each release | Needs a public repo; wallet does not check it yet |
| sigstore-rs | AVOID in the wallet | 0.15.0, 2026-11-05 | Apache-2.0 | Nothing for us today | Bundle verifier leaves inclusion and SET checks as TODOs |
| ed25519-dalek, own format | AVOID | 3.0.0, 2026-07-06 | BSD-3-Clause | Little over minisign | Own format, no audit tooling |
| tinfoil-rs | Reference now, later backend | 0.2.3, 2026-11-06 | Apache-2.0 | Worked Rust code for SNP checks and Rekor inclusion | Genoa-only; possible derivation from AGPL Go |
| tinfoil-go | AVOID for now | v0.17.0-rc.8, 2026-11-05 | Apache-2.0 root, AGPL-3.0 `verify/` | Nothing for Verda | AGPL; fleet-specific |
| EHBP | AVOID on Verda, later backend | 0.5.0, 2026-11-06 | MIT | Nothing on Verda | Solves a router problem we do not have |
| tinfoilsh/cvmimage | AVOID, do not copy | v0.15.3-rc.1, 2026-11-04 | AGPL-3.0 | Nothing | AGPL; booting needs Tinfoil host access |
| modelwrap | ADOPT as a build-time packer | v0.3.0, 2026-09-01 | MIT | Reproducible dm-verity model packs. About 1 week | Go and Docker in the build; mount side is ours |
| mkosi | ADOPT for rootfs and verity partitions | v27.1, 2026-09-28 | LGPL-2.1-or-later | Snapshots, tools tree, repart, seed. 1-2 weeks | Verda's boot layout defeats its verity cmdline path |
| mmdebstrap plus scripts | Fallback | 1.4.3-6 in noble | MIT-style | Simplest rootfs with a stated bit-for-bit claim | More hand-written stitching |
| Packer, Nix, Yocto, Buildroot | AVOID | not pinned | not checked | Nothing for this PoC | Not reproducible, or large build for Ubuntu-only NVIDIA packages |
| Verda REST API plus own script | BUILD-OURSELVES (about 150 lines) | OpenAPI v1 | n/a | Only route to all network options | Private Networks is beta |
| verda-cli | ADOPT for manual ops | v1.10.0, 2026-11-06 | Apache-2.0 | Start, shutdown, list | No network flags |
| Verda Python SDK | AVOID for create | 1.25.0, 2026-10-23 | LICENSE Apache-2.0, classifier MIT | Little | Drops `private_ip` and `public_ip` |
| Terraform provider | AVOID | v1.1.4, 2026-09-24 | Apache-2.0 | Nothing | No start or stop, no network fields |
| SNP derived key sealing | ADOPT for the PoC | ABI 1.58 via `sev` | n/a | About 50 lines, no extra service | Chip-bound; every image update loses the key |
| Old-to-new enclave handoff | BUILD-OURSELVES, deferred | n/a | n/a | The only upgrade path that hides the key | Two live instances; trust rests on the release key |
| Verda LUKS attested unlock | AVOID | docs 27.10.2026 | Docs (scripts' licence UNKNOWN) | Upgrades work | Operator holds the passphrase; needs SSH in the image |
| Trustee / KBS | AVOID | v0.22.0, 2026-09-17 | Apache-2.0 | Nothing net | Whoever runs KBS can read the key; 862-package lockfile |
| HashiCorp Vault | AVOID | not pinned | BUSL-1.1 | Nothing | Same custody problem |

## Method and deviations

- Sources were crate tarballs, repo tarballs, Atom feeds, raw files, one read of the Verda OpenAPI file, one GET of AMD's Turin chain, and the AMD ABI PDF. Text was pulled from that PDF with a local PDFKit script.
- Nothing was built, run or installed from downloaded trees. No sign-in. No contact with Tinfoil or Verda staff. Tinfoil's public docs and repos were read only.
- Deviation: one early command wrote a Verda docs fetch to `/tmp/ignore`, outside the scratchpad. It was harmless and has been removed.
- AMD's KDS spec 57230 could not be read. docs.amd.com returned only a title page (rev 1.05, 2026-09-03), so KDS limits stay UNKNOWN.

## 1. virtee `sev` crate: WRAP

- Release: 8.0.0, 2026-05-27, Apache-2.0 LICENSE, 2.7M downloads. Owners tylerfanelli, larrydewey; published by DGonzalezVillal. Last commit 2026-09-16. VERIFIED (crates.io, tarball, CODEOWNERS, Atom feed). https://github.com/virtee/sev
- Guest API, Linux only: `firmware::guest::Firmware::open`, `get_report`, `get_ext_report`, `get_derived_key`. VERIFIED (`src/firmware/guest/mod.rs`).
- Derived keys: `DerivedKey::new(root_key_select, guest_field_select, vmpl, guest_svn, tcb_version, launch_mit_vector)`. `GuestFieldSelect` has `set_measurement`, `set_guest_policy`, `set_tcb_version` and others. `launch_mit_vector` must be `Some` for message version 2, the default. VERIFIED (`src/firmware/guest/types/snp.rs`). The doc example `get_derived_key(1, request)` is stale.
- Which fields bind: see section 9. VERIFIED against AMD ABI 1.58 Tables 18-20.
- Verification: `(&Chain, &AttestationReport).verify()` and `&Chain.verify()`. `sev::certs::snp::builtin::turin::{ark, ask}` holds compiled-in roots. VERIFIED (`src/certs/snp/`).
- Gaps the wallet must write: `Chain::verify` compares the ARK to no pin, checks no dates or CRL, does not compare VCEK extensions with report TCB and chip ID, and applies no policy or TCB floor. VERIFIED by reading. References: `snpguest/src/verify.rs`, `tinfoil-rs/src/verifier/attestation/sev/validation.rs`.
- macOS client: `sev = { version = "=8.0.0", default-features = false, features = ["snp", "crypto_nossl"] }`. tinfoil-rs uses the same features with 8.0.0 in `Cargo.lock`. Its `test.yml` runs `cargo build --locked` on `macos-latest`, commented as `aarch64-apple-darwin`, and the last three main runs succeeded. VERIFIED (repo files, unauthenticated GitHub API). I did not compile it.
- Trap: issue #369 says the default `sev` feature blocks non-x86_64 builds. Any dependency that enables default features would break Apple Silicon. VERIFIED (issue). `openssl` and `crypto_nossl` together are a compile error. VERIFIED (README).
- Limits:
  - Source notice: "This legacy eager-parsing model will be replaced in the next breaking release by a two-stage attestation report API". Hide the crate behind our own module.
  - Unknown report versions parse as V5 (#374). New layouts fail parsing because reserved bytes must be zero (#343). This fails closed, so it is an availability risk.
  - V6 report support is still PR #407. VERIFIED.
- Docs conflict: the crate comment says TCB "Must not exceed CommittedTcb"; ABI 1.58 Table 19 says "LaunchTcb". Follow the ABI.

## 1b. snpguest: bring-up tool only

0.11.0, 2026-10-26, Apache-2.0, last commit 2026-10-26. Subcommands `certificates`, `display`, `fetch ca|vcek|crl`, `key`, `report`, `verify`, `ok`. It pins `sev =8.0.0` with OpenSSL. VERIFIED (tarball). Verda's docs still pin v0.10.0. Use it on the first instance, not in the gate or wallet. https://github.com/virtee/snpguest

## 2. AMD chain distribution

- Extended report: "The certificate data returned is being provided by the hypervisor through the SNP_SET_EXT_CONFIG". VERIFIED (Linux `Documentation/virt/coco/sev-guest.rst`). `Firmware::get_ext_report` returns `None` certs when `certs_len` is 0. VERIFIED.
- Does Verda supply certs? UNKNOWN. Its docs only show `snpguest report` plus `snpguest fetch vcek` from KDS. VERIFIED (`docs_cc.md`). Test with `snpguest certificates` on the first instance.
- The image cannot carry the VCEK, because it varies by chip and TCB. VERIFIED (`snpguest/src/fetch.rs` URL uses hwid and SPLs).
- Plan, INFERRED: the gate tries `get_ext_report`, otherwise fetches `https://kdsintf.amd.com/vcek/v1/Turin/{hwid}?fmcSPL=..&blSPL=..&teeSPL=..&snpSPL=..&ucodeSPL=..` once and caches it by chip and TCB on the data partition. The VCEK is self-authenticating, so the fetch path needs no trust.
- Rate limits: no number found, so UNKNOWN. attestation-rs treats KDS-style vendors as returning 429 and 503 with a 10-second pause, which is that project's choice and not AMD's statement. VERIFIED (`collateral/store.rs`). One fetch per boot should be safe. INFERRED.
- AMD roots pin: compile into the wallet. Builtin Turin certs in `sev` 8.0.0 match a live `GET /vcek/v1/Turin/cert_chain` by DER SHA-256:
  - ARK `1f084161a44bb6d93778a904877d4819cafa5d05ef4193b2ded9dd9c73dd3f6a` (valid to 2048)
  - ASK `5b77ef5fe7a7a004fd9032668fba9d0fda22f88c4442069a479636a6ae3b3185`
  - VERIFIED (computed today; Trustee's copy agrees). Add our own fingerprint test so a crate bump cannot change them silently.
- CRL: KDS serves `/vcek/v1/Turin/crl`. VERIFIED (`fetch.rs`). The crate does not check it. Staple it and enforce after the first end-to-end run. INFERRED.

## 3. Measurement prediction

- sev-snp-measure: 0.0.13, 2026-05-22, Apache-2.0, author Dov Murik. Last commit 2026-05-28. VERIFIED (PyPI, repo). https://github.com/virtee/sev-snp-measure
- Command shape: `--mode snp --vcpus 30 --vcpu-type EPYC-Turin --ovmf .. --kernel .. --initrd .. --append ..`. Defaults are QEMU VMM and guest features 0x1. Verda's own `measure_boot.sh` passes family, model and stepping and leaves those defaults. VERIFIED (`cli.py`, script). The count of 30 comes from SKU `1RTXPRO6000.30V.CC`. INFERRED.
- Evidence: 4/4 real AMD-signed Tinfoil measurements reproduced and 44/44 negative controls diverged. VERIFIED (`review_2026_11_06/attestation_measurements.txt`). Verda itself has not been compared to a real report.
- vCPU type: `CPU_SIGS['EPYC-Turin'] = cpu_sig(26,0,0)`, and Verda's encrypted sample says "family 26 / model 0 / stepping 0 ... NOT the host silicon's model 2 / stepping 1". VERIFIED. Other Verda text says 26/2/1. Compute both and let the first real report decide.
- Rust alternative: `sev::measurement::snp::snp_calc_launch_digest(SnpMeasurementArgs{..})`, with `CpuType::EpycTurin`, `VMMType::QEMU`, and `GuestFeatures` default 0x1. It handles the OVMF hashes table. VERIFIED. `CpuType` cannot express 26/2/1, and `tests/measurement.rs` has only EpycV4 cases. VERIFIED. Run it in CI and fail on disagreement. The wallet needs no measurement code: it compares against the signed value.
- End-to-end tools: none fits direct kernel boot on Verda. Details:
  - Constellation: "no longer actively maintained". Drop. VERIFIED (README).
  - Contrast: v1.26.1, Business Source License 1.1, Kubernetes. AVOID. VERIFIED (LICENSE, README).
  - Confidential Containers: Kata and Kubernetes based. AVOID. INFERRED from Contrast's README and Trustee's guest-components model.
  - dstack: Verifier and KMS v0.6.3 (2026-11-02), Apache-2.0. README lists "Bare metal AMD SEV-SNP | Host support; requires an SNP-capable guest image" and needs control of the host stack. AVOID. VERIFIED (README); the host-control reading is INFERRED.
  - cvmimage: see section 6.
  - attestation-rs: 0.5.0, last commit 2026-11-05, 19 stars, not on crates.io, `sev = "7"`, root LICENSE MIT against manifest Apache-2.0. Its NVIDIA path only checks NRAS tokens and states it "does not bind that answer to the supplied bundle's device-evidence bytes". AVOID. Its collateral and CRL logic is worth reading. VERIFIED.
  - Buildcage: an organisation created 2026-07-16 for GitHub Actions egress allowlists. Not a measurement tool. `tinfoilsh/buildcage` is 404. Drop. VERIFIED.

## 4. NVIDIA

- nvattest and libnvat: NVIDIA/attestation-sdk, tag 2026.10.02, CMake version 2.0.0, Apache-2.0, last commit 2026-10-07. VERIFIED. https://github.com/NVIDIA/attestation-sdk
- nvTrust README: "The Python SDK and the Local GPU Verifier are deprecated." AVOID both. VERIFIED. https://github.com/NVIDIA/nvtrust
- Package: `nvattest` and `libnvat` `2.0.0.1790976895-1` from `https://developer.download.nvidia.com/compute/cuda/repos/{ubuntu2404,ubuntu2604}/x86_64/`. The index still lists 1.2.2 too. VERIFIED (`Packages`, read today).
- Noble hashes:
  - `nvattest` `77eaec7644b6a221147245a1c0643bac7317265815e3483ef12f283fc91badb0`
  - `libnvat` `d6ae20d7f0d90c62c5e99bcac29cb66f3d9e18c7ee2c4de3aee40134edd70353`
- Licence text in the redist archive begins "NVIDIA Attestation SDK - Copyright NVIDIA Corporation - Apache 2.0 License". VERIFIED (first lines of `libnvat/LICENSE.txt`).
- Reproducible install: this repo is not on snapshot.ubuntu.com, so hash-pin the debs in a local directory and use mkosi `PackageDirectories=`. INFERRED. Tinfoil builds 1.2.2 from source with patches in AGPL Nix, which shows a harder route exists. VERIFIED (`nix/nvattest.nix`).
- `--verifier local` without NRAS: defaults are `--verifier local`, `--rim-url https://rim.attestation.nvidia.com`, `--ocsp-url https://ocsp.ndis.nvidia.com`. NRAS is not contacted in this mode. VERIFIED (`nv-attestation-cli/src/utils.cpp`, `attest.cpp`). So the guest still needs egress.
- OCSP: `collect_ocsp_responses` returns the error when `get_ocsp_response` fails, so an unreachable OCSP responder fails the attest. VERIFIED (`nv_x509.cpp:544-548`). `allow_trust_outpost_ocsp.rego` checks status and validity without nonce match, which allows a caching mirror via `--ocsp-url`. VERIFIED. Not needed for the PoC.
- RIM: `--rim-store dir --rim-dir` exists in code and `attest.cpp` calls `nvat_rim_store_create_filesystem`, but the latest command reference omits those flags. VERIFIED. Use the remote RIM service first. Records are signed, so the path needs no trust. INFERRED.
- Service terms: Trustee's README says modes that depend on a remote NVIDIA service assume a licensing agreement, with "provisions for use in research and development". VERIFIED there. Whether RIM and OCSP fall under it: UNKNOWN. Read https://docs.nvidia.com/attestation/cloud-services/latest/license.html before launch.
- OS: NVAT docs say CC attestation is "only supported on Ubuntu 22.04 and 24.04", yet NVIDIA ships `ubuntu2604` packages and Verda's encrypted flow is 26.04 only. VERIFIED. Use 24.04, Verda's stated default. We do not use the encrypted flow.
- Ready state: "Current `nvattest` versions only attest the GPU and no longer set the ready state". Set `nvidia-smi conf-compute -srs 1` ourselves only on a pass. VERIFIED (`docs_cc.md`).
- Evidence export: `nvattest collect-evidence --format json` gives `{"evidences":[{"arch","nonce","evidence","certificate"}],...}`. VERIFIED (CLI source). The CLI also accepts `--gpu-evidence-source file`, so an auditor can likely re-appraise a stapled blob. INFERRED.
- Rust client option:
  - Raw SPDM signature, nonce and chain can be checked off-instance: 8/8 in Python. VERIFIED (`spike_attestation.md`).
  - Trustee's `deps/verifier/src/nvidia/` has Rust SPDM parsing, `verify_signature` and `validate_nonce`, Apache-2.0, on OpenSSL X509. Its Local mode is "Only Hopper GPUs", "Revocation is not supported", and users supply reference values. VERIFIED (README, source). RTX PRO 6000 is Blackwell. INFERRED.
  - NVIDIA's own Rust crates are FFI over `libnvat`, not on crates.io, Linux in practice. VERIFIED.
- **Recommendation:** the wallet accepts the guest's appraisal as part of the measured image.
  - At boot: `nvattest attest --device gpu --verifier local` with a pinned Rego policy. On any failure the gate exits and no report is ever served.
  - Bind `H(evidence)` in REPORT_DATA and staple the blob for auditors.
  - The wallet checks that hash only.
  - Later option: signature, nonce and chain in the wallet. That proves it is a real GPU, not that firmware is good.
- Limit: guest time comes from the host, so OCSP freshness inherits that. INFERRED; Verda's guest features 0x1 suggest no Secure TSC. Host-forced failure only costs availability.

## 5. Release signing

**Recommendation for the PoC:** sign a canonical JSON manifest with minisign. Pin the public key in the wallet. Verify offline with `minisign-verify`. Also run `actions/attest` on the manifest in a public repo, so each release lands in public-good Rekor for auditors.

- minisign-verify: 0.3.0, 2026-09-25, MIT, 20.6M downloads, owner jedisct1, "zero-dependencies". API: `PublicKey::from_base64`, `Signature::decode`, `PublicKey::verify(bin, &sig, allow_legacy)`, `Signature::trusted_comment`. With `allow_legacy=false` only prehashed signatures pass. VERIFIED (`src/lib.rs`, README). Signer: minisign C 0.12, ISC. VERIFIED. https://github.com/jedisct1/rust-minisign-verify
- sigstore-rs: 0.15.0, 2026-11-05, Apache-2.0. Drop it from the wallet.
  - `src/lib.rs`: "this library is still experimental. Its API can change at any time."
  - `bundle/verify/verifier.rs:236,240`: `// TODO(tnytown): Merkle inclusion; sigstore-rs#285` and `// TODO(tnytown) SET verification; sigstore-rs#285`, although #285 is closed. `Verifier` holds no Rekor keys. `verify_digest_with` checks certificate, SCT, policy, signature and entry consistency. VERIFIED. So a fabricated log entry that matches the other materials should pass. INFERRED by reading, not run.
  - `LogEntry::verify_inclusion` and legacy `cosign::bundle::Bundle::new_verified` exist but are not wired into that verifier. The crate's lockfile has 512 packages. VERIFIED. https://github.com/sigstore/sigstore-rs
- cosign: v3.1.4, 2026-10-20, Apache-2.0. A 2026-11-05 commit is titled "Require new bundle format for sign-blob". Good for auditors, and it is Go, so not in the wallet. VERIFIED (feeds, LICENSE).
- `actions/attest`: v4.2.2, MIT. `attest-build-provenance` v4 "is simply a wrapper" and "new implementations should use `actions/attest`". Outputs include `bundle-path`. VERIFIED. https://github.com/actions/attest
- Constraint: "If the repository is private/internal, it will use the GitHub private Sigstore instance", and Free, Pro and Team plans only get attestations on public repos. So the release repo must be public or the log is not public. VERIFIED (README).
- ed25519-dalek: 3.0.0, BSD-3-Clause, 265M downloads. It works but saves little and leaves us a home-made format. minisign is the simpler choice.
- **Deferred:**
  - Wallet-enforced Rekor inclusion. tinfoil-rs has a Rust path adapted from sigstore-rs with an embedded `trusted_root.json` ("avoids TUF network calls and provides offline verification capability"). Its `merkle/`, `checkpoint.rs` and `rekor.rs` total about 2,700 lines with tests, and `sigstore/` about 5,200. VERIFIED (`wc -l`, source).
  - Log monitoring, witnesses, key rotation, threshold signing.
- Honest limit: until the wallet enforces inclusion, the log is audit-only. Whoever holds the minisign key can give one wallet a private manifest. This matches `red_team.md:487`, "Log now; claim nothing from it yet".

## 6. Tinfoil: read only, later backend

- tinfoil-rs: 0.2.3, 2026-11-06, Apache-2.0 (root LICENSE and manifest, no per-directory licence). Not on crates.io; git dependency. VERIFIED. https://github.com/tinfoilsh/tinfoil-rs
  - `verifier/attestation/sev/{cert_chain,validation,report,signature}.rs` are a good model for the wallet's checks.
  - `cert_chain.rs` hard-codes Genoa (`/vcek/v1/Genoa/`, `ARK-Genoa`) and fetches via `kds-proxy.tinfoil.sh`. It cannot verify Verda's Turin as written. VERIFIED.
  - Its trust flow fetches releases from GitHub (`sigstore/mod.rs`) and is tied to Tinfoil's predicate types (`attestation/types.rs`). VERIFIED.
  - Risk: comments describe parity with AGPL `tinfoil-go`, and I cannot exclude derivation. Get a licence view before copying code. INFERRED.
- tinfoil-go: v0.17.0-rc.8. Root Apache-2.0, `verify/LICENSE` AGPL-3.0. AVOID. VERIFIED.
- `tinfoilsh/verifier`: 404 today. Older notes citing its AGPL licence are out of date. Drop. VERIFIED.
- EHBP: 0.5.0, 2026-11-06, MIT, spec `SPEC.md`, Rust crate `tinfoil-ehbp` in `rust/`. HPKE-encrypted bodies with clear headers for routing. VERIFIED. https://github.com/tinfoilsh/encrypted-http-body-protocol. On Verda the inner channel ends in the gate and no router sits in between, so it adds nothing. It matters for a Tinfoil backend. INFERRED. tinfoil-rs still pins v0.4.1. VERIFIED.
- cvmimage: AGPL-3.0, Nix only, custom kernel, no systemd or shell. AGENTS.md: "Booting the image on AMD SEV-SNP or Intel TDX hardware needs Tinfoil host access". The build itself needs no Tinfoil host, but it is not usable as-is here. Do not copy from it. VERIFIED.
- modelwrap: v0.3.0, 2026-09-01, MIT, last commit 2026-09-08, Go. ADOPT the packer.
  - Output: `<rev>.mpk` (deterministic EROFS plus dm-verity tree) and `.info` as `ROOTHASH_OFFSET_VERITYUUID`. VERIFIED (README, SPEC.md).
  - Usable on Verda: yes for packing. It runs from a Hugging Face pin or `--model-dir` in a digest-pinned container, with no Tinfoil service. Tinfoil docs confirm: "Anyone can rebuild a pack from the pinned revision and declared schema with the open-source modelwrap tool". VERIFIED.
  - Our part, INFERRED: dd the pack to a Verda volume, put root hash, offset and repo in the measured image, run `veritysetup` with `--no-superblock` and the SPEC's fixed parameters, mount read-only. About 30 lines. `/tinfoil/mwp/...` is only a path convention.
  - Note: SPEC assumes attested config, and an artifact "can still contain malicious model code or data". Load safetensors only. A new model means a new measurement. INFERRED.
- Tinfoil Containers docs, read today: config must live in a public repo; GPU needs "Contact us"; V3 features (private secrets, encrypted volumes, attested keys, attested SSH, fresh attestation) are beta. VERIFIED (docs.tinfoil.sh overview, feature status). The GPU step would need contact, so that backend waits.

## 7. Image build

- mkosi: v27.1, 2026-09-28, LGPL-2.1-or-later, systemd project, last commit 2026-11-05. Not on PyPI; install from a pinned git tag. VERIFIED. https://github.com/systemd/mkosi
- Options confirmed in `mkosi.1.md`:
  - `Distribution=ubuntu`, `Snapshot=` (Ubuntu uses `snapshot.ubuntu.com` per `distribution/ubuntu.py:95`)
  - `SourceDateEpoch=`, `Seed=` ("deterministic UUIDs and other partition metadata")
  - `ToolsTree=`, `ToolsTreeSnapshot=`, `RepartDirectories=`, `PackageDirectories=`
  - `SplitArtifacts=` with `kernel`, `initrd`, `partitions`, `roothash`
  - `Verity=hash`, `Ssh=never`, `Bootable=no` ("no unified kernel images ... no ESP"), `Bootloader=none`
- Mismatch: mkosi adds `roothash=` to the kernel command line and otherwise relies on UKIs. Verda reads `vmlinuz` and `initrd.img` from the volume and boots directly, and the cmdline appears host-derived (`verda_platform.md:120`, `encrypted_env_sample.txt`). So mkosi's verity boot path does not apply. VERIFIED for the docs; host origin of the cmdline remains UNKNOWN.
- Mapping to Verda, based on `encrypted_00_build_image.sh` and `encrypted_attested_initramfs.sh`, INFERRED until tested:
  1. mkosi builds root and hash partitions (EROFS) with `Verity=hash`, `Seed=` and fixed epoch, and writes `roothash`.
  2. A post-output script appends a small reproducible cpio holding the root hash to the initrd. Concatenated initrds are normal; mkosi does this for its own modules initrd. VERIFIED for mkosi.
  3. The script builds `/boot` as ext4 with a fixed UUID, `vmlinuz` and `initrd.img` symlinks, and the `boot/lib/systemd` marker. Verda's script comment: "marker: makes Verda pick /boot as 'root'". VERIFIED.
  4. The initrd opens verity and swaps root, modelled on Verda's `local-bottom/zzswaproot`.
  5. Layout mirrors Verda's: bios, esp, boot, then root, hash, and a data partition the gate creates at first boot.
- Flash: mkosi has no part in it. Reimplement in about 30 lines: install `dropbear-initramfs` and a pause hook on a stock CC instance, Shutdown and Start, then `gzip -d | dd of=/dev/vda`, power off, Start. VERIFIED (`01_setup_initramfs_ssh.sh`, `02_flash.sh`). Licence on those scripts is UNKNOWN, so write our own. SSH exists only in this RAM stage and the final image has none. Attestation covers the final boot, so this stage needs no trust. INFERRED.
- Risk: Verda's extractor looks for `sbin/init|lib/systemd`. VERIFIED (env sample comment). An unencrypted root may be picked instead of `/boot`, changing `root=UUID` and so the measurement. Support for EROFS is UNKNOWN. First thing to test.
- Reproducibility: mkosi says these settings make builds "more reproducible" and gives no guarantee. VERIFIED. Require two clean CI builds to match bit for bit, compared with diffoscope. Expect work on maintainer scripts and caches. INFERRED.
- NVIDIA modules:
  - Prefer Ubuntu's prebuilt `linux-modules-nvidia-<ver>-(server-)open-<kver>-generic` from `restricted`, which comes through snapshots. VERIFIED (noble and resolute `Packages`; 580-server through 615 listed).
  - AVOID NVIDIA's `nvidia-dkms-*-open`, because compile at install is a reproducibility hazard. INFERRED.
  - Verda's docs sample shows driver 580.126.09. VERIFIED. The stock image's exact kernel and packages are UNKNOWN. Copy them from `dpkg -l` on the first instance, since a RIM must exist for the driver in use.
- mmdebstrap, the simpler alternative: README says "bit-by-bit reproducible" output if `SOURCE_DATE_EPOCH` is set. 1.4.3-6 in noble. VERIFIED (README, Launchpad). Then `mkfs.erofs -T0`, `veritysetup format`, `sgdisk` by hand. Timebox mkosi to 2 days with `Bootable=no`; if kernel extraction or the double build fights us, switch.
- Other alternatives:
  - Packer or Verda's virt-customize flow: fastest first boot, not reproducible (`apt-get` from live mirrors). Use only for the first experiment. VERIFIED (`00_build_image.sh`).
  - Nix: works, as cvmimage shows, but `docs/build.md` is 1,400 lines and Ubuntu payloads need fixed-output wrapping. Too heavy. VERIFIED.
  - Yocto, Buildroot: no surveyed path for Ubuntu-only NVIDIA CC packages. Not read in depth. INFERRED.

## 8. Deployment automation

- REST API is the only full surface. VERIFIED (https://api.verda.com/v1/openapi.json, read today):
  - `POST /v1/instances` with `DeployInstancePublicDto`: `instance_type`, `image`, `ssh_key_ids`, `location_code`, `os_volume`, `contract`, `is_spot`, `private_ip`, `public_ip`, `network`, `subnet`.
  - `PUT /v1/instances` with action `start`, `shutdown`, `force_shutdown`, `delete` and others. `POST /v1/instances/action` is deprecated. Auth is `POST /v1/oauth2/token`.
  - No attestation API: zero hits for SEV, SNP or attest in the spec.
  - `GET /v1/images` can return category `confidentialComputing` (changelog 2026-09-15).
- Python SDK `verda` 1.25.0: `InstancesService.create(...)` builds a fixed payload with no `private_ip` or `public_ip`. `InstancesService.action(id_list, action, ...)` takes any string. LICENSE is Apache-2.0 while the classifier says MIT. VERIFIED (`verda/instances/_instances.py`). https://github.com/verda-cloud/sdk-python
- verda-cli v1.10.0: `verda vm create --instance-type --os --location`, `verda vm start|shutdown|delete`, `verda vm action --action force_shutdown`. `--os` also takes a detached OS volume ID. No network flags in non-test `cmd/vm` code. VERIFIED. https://github.com/verda-cloud/verda-cli
- Go SDK `verdacloud-sdk-go`, Apache-2.0: no `private_ip` in `pkg/verda/instances.go` at main. VERIFIED, other files not checked.
- Terraform v1.1.4: no network attributes, and Update errors with "Instances cannot be updated for now". No start or stop. Drop. VERIFIED (`resource_instance.go`, docs). https://github.com/verda-cloud/terraform-provider-verda
- Private-IP-only: `private_ip:"auto", public_ip:"none"`, but "no internet access in either direction", beta, and it cannot be changed after deploy. VERIFIED (docs.verda.com/private-networks). The gate needs egress, so this needs a NAT instance. PoC choice: legacy public IP with `nftables` in the measured image, inbound dropped. "Verda does not filter inbound traffic". VERIFIED.
- CC SKU: all surfaces pass `instance_type` as a free string, so `1RTXPRO6000.30V.CC` should work. API behaviour for CC images and for a customized volume ID: UNKNOWN. Image slug: UNKNOWN (needs token).
- Restart: "an in-guest `reboot` ... will **not** bring the instance back up. ... use the Verda console (or API): Shutdown, then Start". Shutdown still bills. VERIFIED.
- Minimal scripts to write:
  1. `deploy.py`, about 80 lines with `requests`: token, create, poll, action.
  2. `flash.sh`, about 30 lines.
  3. `verify_live`: wallet's attestation path against the new onion.
  4. `build_release.sh`: mkosi, then both measurement tools, manifest, minisign, attest.

## 9. Secrets at rest

AMD ABI 56860 rev 1.58 settles the key behaviour. This replaces `red_team.md:232-233`, which said "ABI not read by any note".

- Keys are rooted "in a VM Root Key (VMRK) that is supplied as part of the launch flow (and migrates with the guest) or in the VCEK, which is machine specific" (§4.10, p.34). VMRK comes from a Migration Agent (§4.11). Verda offers none. INFERRED. So the root is VCEK.
- Always mixed: root key, VMPL, Host Data, ID or author key, field selection. Optional: TCB version, guest SVN, measurement, family ID, image ID, policy, mitigation vector (Table 18, p.55-56).
- Consequences, INFERRED from that table:
  - Without the MEASUREMENT bit and without an ID block, any image on that chip derives the same key.
  - With an author key in an ID block, sealing could follow the release signer and survive updates. That is the right primitive, but whether a Verda tenant can supply an ID block is UNKNOWN. `author_key_digest` on a real report will show today's state.
  - The host can break derivation through Host Data or `MaskChipKey` (p.26, p.57). That costs availability, not secrecy.
- TCB: "The VCEK can be computed for any TCB_VERSION less than or equal to the CurrentTcb ..., allowing for migrations of secrets" (p.22). Setting bit 5 with a fixed floor should survive firmware updates. INFERRED; behaviour with bit 5 clear is UNKNOWN.

| Option | Survives Shutdown/Start | Survives image update | Hidden from operator | Work |
|---|---|---|---|---|
| Derived key, VCEK, MEASUREMENT set | Yes if same chip (host stability UNKNOWN) | **No** | Yes | About 50 lines |
| Derived key, no measurement | Same chip | Yes | **No** | Do not use |
| Verda LUKS, operator sends passphrase | Yes, by hand each boot | **Yes**, verifier updates expected value | **No** | Dropbear in image, breaks no-SSH |
| Trustee/KBS | Yes | Yes, by policy | **No** unless KBS is in its own CVM, which has the same problem | 3 services |
| Vault | Yes | Yes | **No** | Plus unseal and licence |
| Handoff, old enclave to next signed image | Needs live peer | Yes | Yes, until a bad release | About 300 lines, two instances for an hour |

- LUKS: `encrypted_03` also fails open on the signature check and takes APPEND from the guest (`spike_attestation.md`). `tee_gpu_attestation.md:109` says it would "contradict the promise". AVOID.
- Trustee: v0.22.0, Apache-2.0, active. Default backend is local files at `/opt/confidential-containers/kbs/repository`, with an admin token; lockfile has 862 packages. VERIFIED. https://github.com/confidential-containers/trustee. It would only move custody. Its `deps/verifier/src/snp` is good reading.
- Vault: BUSL-1.1, licensor IBM. VERIFIED (LICENSE). No SNP auth method found, not searched exhaustively. INFERRED.
- **Recommendation for the PoC:** generate the key inside the gate. Seal it with AEAD under `get_derived_key` (VCEK root, MEASUREMENT, GUEST_POLICY, TCB_VERSION with a fixed floor, `launch_mit_vector` `Some(0)`). Store the blob on the plain data partition. Do not use spot.
  - Any new image means a new issuer key and voided test balances. Accept this for the PoC and document it.
  - Build handoff before any real balance exists, reusing the wallet's verifier crate. `red_team.md:239` notes a two-week C tor cadence, so this cannot stay.
- Not solved by any option: sealing gives no freshness, so the host can restore an old blob and nullifier store. I found no counter in the ABI sections read. Spent credits could replay. INFERRED.
- Key importance: `skM` loss or theft means forged or lost money. Prompt secrecy rests on the session key in REPORT_DATA, which never persists. The onion key needs no sealing for prompt secrecy either, since the wallet checks attestation inside the tunnel. INFERRED. This reduces, but does not remove, the pressure in `red_team.md:184`.

## Design impact

1. **Root hash moves into the initrd.** Not on the cmdline: boot partition `/boot` with Verda's marker, initrd opens verity from an appended cpio and swaps root.
2. **Staple becomes report, VCEK, CRL, NVIDIA evidence.** The wallet uses compiled-in Turin ARK and ASK, and at most compares any supplied copies for equality.
3. **"Tor only" narrows.** User traffic stays Tor only. At boot the gate needs direct egress to KDS and to NVIDIA RIM and OCSP. Set this in a measured `nftables` policy, drop inbound, and use a public IP rather than private-only.
4. **Wallet gets its own check layer on `sev`:** ARK pin, dates, VCEK TCB and chip ID against the report, policy, VMPL, committed equals current, TCB floor. Put it in a crate shared with the gate for later handoff.
5. **Manifest pins host inputs:** OVMF SHA-256 (Verda's `fea1855b...`), exact cmdline, vCPU count and signature, guest features, TCB floor, driver and policy versions. If Verda changes OVMF or the cmdline, all wallets refuse until we publish a new manifest. This fails closed and is a real availability risk.
6. **Release build runs in a public repo,** or the log is not public.
7. **Wallet does not enforce the log in the PoC.** Say so wherever transparency is claimed.
8. **Issuer key does not survive image updates in the PoC.** Handoff is a required post-PoC item, and each new image voids balances until then.
9. **Base OS is Ubuntu 24.04,** with prebuilt `linux-modules-nvidia-*` copied from Verda's stock image choice, and no DKMS.
10. **Drop from the plan:** Terraform, Verda LUKS unlock, Trustee, sigstore-rs in the wallet, the Python NVIDIA SDK, and any `tinfoilsh/verifier` reference.

First-instance checks, each of which could change the above:

1. Which CPU signature matches a real report.
2. `snpguest certificates` output.
3. Whether a marker in `grub.cfg` moves the cmdline, and which partition the extractor picks with an EROFS root.
4. `chip_id` and a derived key across Shutdown and Start.
5. `author_key_digest` and `id_key_digest` on a real report.
6. Stock image `uname -r` and `dpkg -l`.
7. nvattest behaviour with OCSP blocked.
