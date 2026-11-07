# Spike: verifying attestation without being on the machine

Run 2026-11-06, about 13:37 to 14:10 EST. Throwaway validation experiment, not the PoC. No instance rented, no accounts, no API keys.

Evidence root (all `output/`, `scripts/` and `downloads/` paths below are relative to this):
`/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/attestation`

## Bottom line

**Can a remote client verify attestation off the machine? Yes for the CPU side and for the GPU evidence signature, shown on real hardware reports. Not yet shown on Verda itself.**

**Whom must it contact? In Tinfoil's design, only the enclave.** AMD, Sigstore and GitHub data arrive stapled, with roots embedded in the client. The client never contacts NVIDIA, but only because it does not appraise GPU evidence itself, so a client that does will need RIMs and revocation data stapled too. A Verda deployment could copy this design, but it has not been shown working there.

- **The strongest result: 4 of 4 real AMD-signed `MEASUREMENT` values were reproduced byte for byte on this laptop** from public kernel, initrd, OVMF and cmdline with `sev-snp-measure` 0.0.13. All 44 negative controls diverged. Those reports come from Tinfoil's fleet (two chips, Genoa and Turin), not Verda. This validates the method on real silicon. It does not validate Verda's host inputs.
- **On Verda, the off-instance digest works mechanically but has never been compared with a real report.** Exact Verda flags gave one digest over 10 runs and 26 of 26 mutations changed it. Whether the digest matches depends on host inputs that only an instance can confirm, listed in the test plan below.
- **Offline verification works.** `tinfoil-go` accepted two captured documents with the network blocked by a sandbox, and behaved as expected on all 36 cases. A forged channel key with every unsigned hash recomputed was rejected by the AMD-signed `REPORT_DATA`.
- **The Tinfoil verifier cannot be pointed at Verda as shipped.** Its default policy looks up each chip in a signed inventory of 25 Tinfoil machines. The caller-supplied override refused the public document format, so that route is untested. Also note `verify/` is **AGPL-3.0**, while the rest of `tinfoil-go` is Apache-2.0.
- **The SDK is safe over a SOCKS proxy. The CLI's verify command is not.** SDK: 8 of 8 passed with only loopback allowed, so no direct connection and no local DNS. CLI: failed under the same sandbox, because it dials the enclave directly after fetching over the proxy. Real Tor was not tested.
- **GPU evidence is portable.** 8 of 8 real SPDM signatures, nonces and chains verified off-instance with no NVIDIA contact, and the root matches NVIDIA's published file. RIM and OCSP were not tested, so this is not a complete GPU appraisal.
- **Do not copy Verda's sample verify script as is.** Its AMD signature check fails open, so it can send the LUKS passphrase after a bad signature.

## Provenance

Web and repository content was treated as data. Advisories were queried on OSV and GitHub on the run date.

| Item | Canonical source and how established | Publisher | Version or commit | Licence | Checksum or signature | Advisories | Use |
|---|---|---|---|---|---|---|---|
| Verda OVMF (AMD, TDX) | `github.com/verda-cloud/edk2`, linked from docs.verda.com | Verda (`support@verda.com`; org not GitHub-verified, created 2025-11-03) | branch `amdsev-large-bars` at `0c0bd94a`, unsigned commit | BSD-style edk2 fork (not separately reviewed) | SHA-256 matches docs, GitHub asset digest and SHA256SUMS | Not checked | Data, never run |
| Verda helper scripts (12) | docs.verda.com | Verda | fetched on run date; `tdx-attest.py` matches docs v0.6.0 hash | Not stated | `scripts.sha256` recorded | n/a | Read only, never run |
| `sev-snp-measure` | `github.com/virtee/sev-snp-measure`; PyPI owners dubek and fitzthum | VirTEE | v0.0.13, `5b6a79e8` | Apache-2.0 | Wheel `feda1aeb...`; wheel diff against tag clean; no PyPI provenance attestation | None (PyPI, OSV) | Run; upstream tests 32 of 32 |
| `cryptography`, `cffi`, `pycparser` | PyPI, as dependencies | pyca, cffi and pycparser maintainers | 50.0.2, 2.1.1, 3.12 | Apache/BSD | pip TLS only | No `cryptography` advisories found | Run |
| Ubuntu kernel and initrd | cloud-images.ubuntu.com | Canonical | 26.04, 7.0.0-38-generic | Various | SHA256SUMS GPG-verified with UEC key `D2EB...7C81`; gpg trust "unknown" | n/a | Data |
| Tinfoil CLI | `github.com/tinfoilsh/tinfoil-cli`, linked from docs.tinfoil.sh | Tinfoil (org not GitHub-verified, created 2024-04-29) | v0.19.19, `04a49c69` | Apache-2.0 | SHA-256 matches `checksums.txt`; **Sigstore attestation verified** with correct identity, wrong identity and wrong artifact rejected | None (OSV, GHSA) | Run, prebuilt; `install.sh` not used |
| `tinfoil-go` | `github.com/tinfoilsh/tinfoil-go`, linked from docs.tinfoil.sh; **no `tinfoilsh/verifier` repo exists (404)** | Tinfoil | **v0.17.0-rc.8 (pre-release)**, `66e16b29` | Root Apache-2.0, **`verify/` AGPL-3.0** | Go sumdb `h1:Kkv9q1gb...` | None (OSV, GHSA) | Run in harness |
| `sigstore-go` | `github.com/sigstore/sigstore-go`, transitive dependency | Sigstore | v1.2.2 | Apache-2.0 | Go sumdb | 3 past advisories, all fixed at or before 1.2.1; OSV returned none at 1.2.2 | Run in harness |
| Go toolchain | proxy.golang.org via `GOTOOLCHAIN` | Google | go1.27.3 | BSD | Go sumdb | Not checked | Run |
| Transitive Go modules | Module path equals repository; 399 `go.sum` lines | Various | Pinned in `go.sum` | Various | Go sumdb | **Not individually audited** | Compiled with CGO off; Go has no install scripts |
| Tinfoil OVMF, kernels, initrds | `tinfoilsh/edk2` release; `images.tinfoil.sh/cvm`, URL from `measure-image-action` source | Tinfoil | OVMF v0.0.4; images v0.14.15 and v0.14.17 | n/a | SHA-256 match the Sigstore statements in the documents | n/a | Data |
| `measure-image-action`, `cvmimage` source | `github.com/tinfoilsh/...`, linked from docs | Tinfoil | **`main`, unpinned** | AGPL-3.0 for the action | None | n/a | Read only |
| NVIDIA root certificates | `github.com/NVIDIA/nvtrust`, path from NVIDIA's verifier tree | NVIDIA | **`main`, unpinned** | Apache-2.0 | None offered | n/a | Data |
| AMD chains | `kdsintf.amd.com`, fetched twice (`Turin`, `Genoa`) | AMD | Run date | n/a | Compared with the copies in `tinfoil-go` | n/a | Data |

**Disclosures against the brief:**
- Early read-only `gh api` metadata calls used the user's existing GitHub login. I then switched to unauthenticated curl and an empty `GH_CONFIG_DIR`. Nothing was written to GitHub.
- A plain `go version` with default settings may have touched Go's local telemetry counters in `~/Library/Application Support/go/telemetry`. Those files show a 13:37 modification time, and a sibling spike started at the same moment, so I cannot say which caused it. I left them alone. All later Go runs used a redirected `HOME`, `GOENV=off` and `GOTELEMETRY=off`.
- `gh attestation verify` failed because the local `gh` is too old for the current trust root. `sigstore-go` replaced it.
- Python's default user agent got a 403 from `images.tinfoil.sh`, while curl got a 200 from the same public URLs, so I used plain curl.

## Method

1. **Part A.** Read the Verda docs and scripts without running them, extracted the exact measurement flags, and ran `sev-snp-measure` with them on public inputs. Then took real AMD-signed reports from public Tinfoil endpoints and tried to reproduce their `MEASUREMENT` from public artifacts (`scripts/a3_real_report_repro.py`). I added this step because it is the only way to test the method against real silicon without renting an instance.
2. **Part B.** Ran the CLI against public endpoints with no API key, under a logging proxy and `lsof` polling. Captured raw documents. Then ran `tinfoil-go`'s `verify` package offline under `sandbox-exec` with all network denied, with an in-process dial as a control. Then ran the SDK and CLI under a loopback-only sandbox through a logging SOCKS5 and CONNECT proxy, so a direct connection or local DNS lookup could not succeed.
3. **Independent check.** Verified report signatures with Python `cryptography` alone (ECDSA P-384 over the first 0x2A0 bytes), with no Tinfoil code, against chains fetched from AMD.
4. **Part C.** Verified real SPDM evidence from a Tinfoil document off-instance, then read NVIDIA and Verda documents for the rest. Did not contact NRAS or the RIM service.

## Results, Part A: Verda launch measurement

### Org and firmware

| Check | Result |
|---|---|
| `verda-cloud` is Verda's | Yes. Footer of docs.verda.com, verda.com, and `support@verda.com`. Not GitHub-verified |
| AMD OVMF SHA-256 | `fea1855b6c98...3a81`, 4,194,304 B, matches docs, asset digest and SHA256SUMS |
| TDX firmware SHA-256 | `f17a622af0e6...95a6`, matches |
| Reproducible rebuild | Not attempted. `PROVENANCE.txt` gives CI run IDs and rebuild steps |
| TDX `MRTD` from public TDVF, own calculation | `eff2f480219e...145c`, **exact match with the docs value** (assumes the docs sample is from a real instance) |

### Exact Verda command

From `measure_boot.sh` lines 61 to 70:

```
sev-snp-measure --mode snp --vcpus "$VCPUS" --vcpu-family "$VCPU_FAMILY" --vcpu-model "$VCPU_MODEL" \
  --vcpu-stepping "$VCPU_STEPPING" --ovmf "$OVMF" --kernel "$KERNEL" --initrd "$INITRD" \
  --append "$APPEND" --output-format hex
```

`APPEND` comes from `/proc/cmdline` with `initrd=initrd` stripped. `VCPUS` and the CPU signature come from `nproc` and `/proc/cpuinfo`. The flags omit `--guest-features`, so the tool default of `0x1` applies.

### Where the inputs come from

| Input | Chosen by | Public off-instance? |
|---|---|---|
| OVMF | Verda, public pin | Yes. Docs tell users to `scp` it from the instance, but the client should pin the GitHub hash instead |
| Kernel, initrd | Image owner | Default image: no public source found. Own image: yes, if the operator publishes them |
| Cmdline | **Verda, via QEMU `-append`**. Sample: `console=ttyS0 root=UUID=3a07f580-... ro net.ifnames=0 biosdevname=0 fsck.mode=auto fsck.repair=yes pci=realloc,nocrs` | Template in docs. `root=UUID` is re-derived by Verda from the disk. Whether customers can change it is unknown |
| vCPU count | Verda, per SKU | Must be published per SKU |
| CPU signature | Verda, via `-cpu` | **Docs conflict.** Sample says 26/2/1; `encrypted_03` says the guest sees 26/0/0. These give different digests |
| `guest_features` | QEMU version | Assumed `0x1`, unconfirmed on Verda |

No input is inherently instance-only. In practice the default image's kernel, initrd and `root=UUID` are, unless the operator builds and publishes its own image. A changed host input breaks verification rather than silently passing it, as long as the client pins the cmdline. That is an availability risk, not an integrity risk.

### Off-instance digest with Verda OVMF

Inputs: Verda OVMF, signed Ubuntu 26.04 kernel and initrd, documented cmdline, 30 vCPUs, 26/2/1.

| Test | n | Result |
|---|---|---|
| Baseline with exact Verda flags | 3 runs | `a253486f24a3...6014`, identical |
| Determinism | 10 runs | 1 distinct value, mean 0.071 s |
| Mutations (kernel, initrd and OVMF bit flips, initrd +1 byte, cmdline variants including `init=` and roothash, vCPU, signature, guest features, VMM type, appended cpio) | 26 | **26 of 26 changed the digest**; 27 distinct values with baseline |
| SEV hashes table support | 1 | Supported, table GPA `0x810c00`, section `0x810000` size `0x1000` |
| Negative control: firmware with hashes section removed | 1 | Tool refuses with an error, rc=1 |

**This does not show a match against a Verda report.** The docs' truncated sample `5D8F582C...` cannot be compared because its inputs are unknown.

### Real-report reproduction (Tinfoil fleet)

Exact flags from Tinfoil's public `measure-image-action`: `--mode snp --vcpus N --vcpu-type EPYC-v4 --vmm-type QEMU --ovmf --kernel --initrd --append --guest-features 0x1 --output-format hex`.

| Enclave | Report ver | Chip | vCPUs | Image | Inputs hash-match statement | Real `MEASUREMENT` | Computed |
|---|---|---|---|---|---|---|---|
| router | 3 | Genoa `1af1aa6c` | 8 | v0.14.15 | Yes | `fb8eabe68d3b...022a` | **Match** |
| kimi-k3 (8 GPU) | 5 | Turin `a4fc1087` | 32 | v0.14.17 | Yes | `7fb2b796a4b3...e5c4` | **Match** |
| websearch | 3 | Genoa `1af1aa6c` | 8 | v0.14.15 | Yes | `8f84943f347a...d04e` | **Match** |
| pii-filter | 3 | Genoa `1af1aa6c` | 32 | v0.14.15 | Yes | `093ae06ffa2f...bcd9` | **Match** |

- **Sample: 4 reports, 2 physical chips, 2 chip families.** Report signatures independently verified 4 of 4 and tampering any signed field fails 6 of 6 per report on router and kimi.
- **44 of 44 negative controls diverged** (11 per report): vCPU+1, trailing cmdline space, roothash nibble, config-hash nibble, `init=` injected, signature 26/2/1, signature 26/0/0, `EPYC-Genoa`, guest features `0x0` and `0x21`, Verda's OVMF.
- **The CPU signature in the digest need not match the silicon.** `EPYC-v4` reproduced on both Genoa and Turin, and 26/2/1 and 26/0/0 did not, even on Turin. So the value is whatever `-cpu` says, which supports reading Verda's signature as host-chosen.

### Conclusion for Part A

**Can a third party compute the expected measurement purely from public artifacts?** Yes on Tinfoil, demonstrated. On Verda, only if all of these hold, and none is shown yet:
1. The operator builds its own image and publishes kernel, initrd and exact cmdline including `root=UUID`.
2. The vCPU count and CPU signature per SKU are known.
3. `guest_features` is what the tool assumes.

**What extends trust to the rootfs:** a dm-verity root hash inside a measured input.
- **Cmdline:** Tinfoil's measured cmdline carries `roothash=` and `tinfoil-config-hash=`. Flipping one nibble of either changed the digest in 4 of 4 reports. Verda appears to control `-append`, so this may not be available.
- **Initrd (recommended for Verda):** an appended roothash cpio also changed the digest. Verda's encrypted sample has a root-swap hook (`zzswaproot`) that could carry a verity setup.
- **Not tested:** that a kernel actually enforces verity at boot. Model weights are not measured by boot either, so they need their own hash pinned in a measured config, as Tinfoil does.

### Weaknesses in Verda's sample scripts

| # | Weakness | Status |
|---|---|---|
| a | `encrypted_03` line 98: `snpguest verify attestation ... \| grep -q "VEK signed" && echo ...` is not fatal under `set -e`. The script continues with an unverified report and sends the passphrase. Steps 5 and 6 then only compare fields of that report | Semantics reproduced with a stub (`output/A2_setE_reproduction.txt`). Verda's script not run |
| b | `encrypted_03` derives `APPEND` from the guest, so a host-injected `init=` matches itself | Read only; digest sensitivity to `init=` tested |
| c | Docs say to `scp` OVMF from the instance | Read only |
| d | No check of `POLICY` debug bit or minimum TCB | Read only, grep found none |
| e | `measure_boot.sh` never verifies the AMD signature; it is an in-guest self-check | Read only |
| f | `snpguest` fetched without checksum; `pipx install sev-snp-measure` unpinned | Read only |

Also worth reporting to Verda: a script comment says the tool "re-adds" `initrd=initrd`, but `sev_hashes.py` hashes `append+NUL`. Stripping is still correct.

## Results, Part B: Tinfoil verifier

### CLI runs

| Run | Condition | n | Result |
|---|---|---|---|
| R1, R1b | Direct, `-e inference.tinfoil.sh -r tinfoilsh/confidential-model-router -j`, no key | 6 | All pass, 0.78 to 1.29 s. One host, one IP |
| R2 | CONNECT proxy | 1 | Pass, **but with a direct bypass connection and local DNS** |
| R3 | SOCKS5, allow enclave only | 1 | Pass, **same bypass** |
| R4 | Deny-all proxy | 1 | Fails closed, exit 1 |
| R5 | No `-e` | 1 | Pass, contacts `atc.tinfoil.sh` and `router-0.tinfoil.sh` |
| R6, R7, R8 | Wrong repo, wrong digest, non-enclave host | 3 | All rejected |
| R9 | GPU enclave `kimi-k3-inf20...` | 1 | Pass, one host, 219 KB, no NVIDIA, AMD or Sigstore contact |

**What it checks, from code:** report signature and VCEK chain to embedded AMD roots, CRL, `MEASUREMENT` against the Sigstore statement, Sigstore identity and digest, machine inventory, guest policy, platform info, TCB minimums, freshness witness at most 7 days old, `REPORT_DATA` binding, and TLS SPKI against the live connection.

**Exercised by a rejection below:** signature, Sigstore identity and digest, freshness, `REPORT_DATA`. **Not exercised, code only:** CRL, machine inventory, guest policy, platform info, TCB minimums. TLS SPKI was only seen matching (R1), and I did not test a mismatch.

### Offline verification

`sandbox-exec` denied all network. The control dial to `1.1.1.1:443` returned "operation not permitted". Two documents, 18 cases each, 36 in total, **0 unexpected in the final run**. Genuine documents took 7 to 10 ms.

| Case | Rejected at |
|---|---|
| Genuine, and JSON round trip | Accepted |
| Wrong nonce | Envelope |
| Bit flips in crypto material or device evidence (format break), semantic changes, trailing space | Envelope parse or hash |
| **Forged crypto material with all unsigned hashes recomputed** | **`REPORT_DATA` policy check** |
| **Forged device evidence with all unsigned hashes recomputed** | **`REPORT_DATA` policy check** |
| Report `MEASUREMENT` or `REPORT_DATA` bit flip | AMD signature |
| Wrong repo, wrong digest pin, wrong tag pin | Sigstore identity or digest |
| Clock +30 days, clock -5 years | Freshness witness |

**Corrections during testing:** the first tamper cases only proved a parser rejects garbage, so I added the semantic and forged cases. One semantic case on the router document was then accepted, because that document has no device evidence and my mutation changed nothing. That was a harness flaw, not a verifier flaw. I fixed it and reran.

**What this does not show:** a captured document stays valid for its nonce until the witness expires. Liveness requires a fresh client nonce, so the client still has to contact the enclave each session.

### SDK and CLI under a loopback-only sandbox

| Run | Client | Route | n | Result |
|---|---|---|---|---|
| S1 | SDK | Direct, no sandbox | 1 | Pass, 575 ms, `/health` 200 |
| S2 | SDK | No proxy, sandboxed (control) | 1 | **Fails, as it should** |
| S3 | SDK | `socks5h://`, sandboxed | 5 | **5 of 5 pass**, verify 429 to 1134 ms, `/health` 200 |
| S4 | SDK | `socks5://`, sandboxed | 1 | Pass, proxy saw the hostname |
| S5 | SDK | HTTP CONNECT, sandboxed | 1 | Pass |
| S6 | SDK | `socks5h://`, GPU enclave, sandboxed | 1 | Pass, 748 ms |
| S7 | **CLI** | `socks5h://`, sandboxed | 1 | **Fails**: fetches over the proxy, then `dialing enclave: lookup ... no such host` |

S7 confirms the cause seen in R2 and R3: the CLI's `attestation verify` calls `tls.Dial` directly for the fingerprint comparison.

Attestation traffic per verification: about 2.3 KB up and 83 KB down for the router, 219 KB down for the GPU enclave.

### Every host a client contacts

| Host | When | What it reveals | Staple, cache or Tor |
|---|---|---|---|
| Enclave (`inference.tinfoil.sh`, `*.containers.tinfoil.dev`) | Every verification (observed) | Client IP, timing, random nonce, SDK headers. Hostname also names the model | Cannot be stapled, as it is the party being verified. **Over SOCKS: tested with SDK. Over real Tor: untested** |
| Local DNS resolver | Direct mode only (observed) | Hostname | Eliminated by `socks5h` (tested) |
| `atc.tinfoil.sh` | Only when no `-e` is given (observed in R5) | Client IP, intent to use Tinfoil | Avoidable by pinning a host (tested) or caching the list |
| `router-0.tinfoil.sh` | Router selected by `atc` (R5) | As for enclave | As for enclave |
| `kdsintf.amd.com` | **Not contacted** (tested) | Would reveal chip ID and TCB, so the server fleet the client is looking at | Stapled: VCEK and CRL in document, AMD roots embedded |
| Sigstore, GitHub | **Not contacted** (tested) | Would reveal repo interest | Stapled: bundles in document, trust root embedded |
| NVIDIA OCSP, RIM, NRAS | **Not contacted** (tested) | Would reveal GPU identifiers and driver versions | Client skips these entirely; see GPU section |

**Limitations of this table:**
- macOS `trustd` may make OCSP requests outside the process when validating the WebPKI certificate. `lsof`, `GODEBUG` and the sandbox cannot see them, and `tcpdump` needs root. A Linux client is needed to rule this out.
- Tinfoil's docs also describe `github-proxy.tinfoil.sh`, `kds-proxy.tinfoil.sh` and a bundle proxy. These were not observed with these versions. The JS SDK defaults were not checked.
- The default CLI also checks for updates in interactive terminals. It was not observed in these non-interactive runs. Set `TINFOIL_NO_UPDATE_CHECK=1`.

### Tied to Tinfoil's fleet?

**As shipped, yes.** Three things bind it:
- **Machine inventory.** `platform-endorsements.json` v0.1.10 maps 25 hardware IDs to policies (12 `amd-turin-prod`, 1 `amd-genoa-prod`, 1 `amd-genoa-dev`, 11 TDX). A machine not in the map is an error. All 4 observed reports came from listed chips. **The reject path itself was not exercised**, as I have no valid report from an unlisted chip.
- **Fixed signers.** Platform endorsements must come from `tinfoilsh/cvmimage`, and freshness from `tinfoilsh/freshness-witness`.
- **Pinned IDs.** `runtimeRepoID` 902195777 and `tinfoilOrganizationID` 168487856.

The workload `--repo` part is generic, so any GitHub-signed repo fits there.

**A self-hosted Verda image needs more.** `verify.WithEmbeddedReferences{Config, ConfigReference, Runtime, Platform}` and `WithPinnedRegisters` exist for this, but they require `attestation-collaterals/v3`. On the public document, my attempt returned `configured trust requires https://tinfoil.sh/predicate/attestation-collaterals/v3`. So the server would have to emit that format, and the route is **untested**.

**A smaller alternative is likely enough for the PoC.** `scripts/b_snp_sig_check.py` plus `sev-snp-measure` covers signature, chain and measurement in about 50 lines. CRL, TCB and policy checks would still need adding.

**The AGPL-3.0 licence on `verify/` matters** if the PoC embeds it in a distributed client. Get this checked before relying on it.

### GPU evidence in the Tinfoil verifier

**The client binds GPU evidence but does not appraise it.**
- **Bound by test:** `device_evidence` is hashed into `REPORT_DATA`, and forged evidence was rejected 2 of 2 times.
- **Not appraised, read only:** a code search found no NVIDIA appraisal code in the client. The only access is a `DeviceEvidenceItems()` accessor.
- **Trust is transitive:** per source, the measured image runs `nvattest attest --device <d> --verifier local` at boot and aborts on failure. Evidence is then collected per request with the client's nonce. This is read only. A `DummyAttestation` config path also exists, and its setting is covered by the config hash on the measured cmdline.

## Results, Part C: nvattest evidence

### Tested on real evidence (8 GPUs in one Tinfoil document; no Verda, no `nvattest` run, no NVIDIA contact)

Document 213,019 B, "BLACKWELL", GB110, driver 595.71.05.

| Check | Result |
|---|---|
| SPDM nonce equals client nonce | 8 of 8 |
| SPDM P-384 signature over transcript | 8 of 8 valid |
| Certificate chain links (5 certs) | 8 of 8 |
| Tampered transcript rejected | 8 of 8 |
| Root equals NVIDIA's published `verifier_device_root.pem` | **Yes**, SHA-256 `102bf659d541...7b48` |
| `REPORT_DATA` covers this evidence | Yes |

**Not done:** RIM comparison of measurement blocks, OCSP, NVSwitch evidence. Without RIM comparison this shows genuine NVIDIA hardware answered a fresh nonce. It does not show the firmware is the expected firmware.

### From documents only (no test)

**Export.** NVIDIA documents this. `nvattest collect-evidence --device gpu --nonce <hex>` emits JSON with an `evidences` array of `{arch, nonce, evidence, certificate}` plus `result_code` and `result_message`. The docs say the output "can be saved and used with `attest --gpu-evidence-source=file`". That matches the fields I parsed from Tinfoil's document.

**Verify elsewhere.** `nvattest attest --device gpu --gpu-evidence-source file --gpu-evidence-file F --nonce <hex> --verifier local`. The docs say the correct nonce "must be supplied". Unknown: whether the binary runs on a machine with no GPU or NVML, and on macOS.

**What a remote client needs from NVIDIA:**

| Item | Source | Reveals if fetched | Staple or cache |
|---|---|---|---|
| Device Identity CA root | Pin (`102bf659...`, tested) | Nothing | Pinned |
| RIM signing root | Pin (`12977b5115ac...f5d1`, compared hash only) | Nothing | Pinned |
| Driver and VBIOS RIMs | `rim.attestation.nvidia.com` | Client IP, driver and VBIOS versions | Signed, so a server or mirror could supply them. `--rim-url` overrides the base URL. Not tested |
| OCSP for chains | `ocsp.ndis.nvidia.com` | Client IP, certificate identity | Signed and time-limited. `--ocsp-url` overrides the base URL. No documented way to pass a stapled response to `nvattest`, so a mirror or own verifier is needed. Not tested |
| NRAS | `nras.attestation.nvidia.com` | Evidence, plus account identity through the key | **Avoid.** The documented example uses an NGC service key, which is identity. The default `--verifier local` should not need it. Verda's sample uses `--verifier remote` in-guest |

Staleness is a policy choice: a host can withhold a newer revocation, so the client needs a maximum age. `attest-v2` and CoRIM are documented as experimental. Its `--no-verify-revocation` flag should not be used in production. Verda's docs also require compliance with NVIDIA's Product-Specific Terms for Confidential Computing, which I did not review.

**Binding CPU report, GPU evidence and channel key** (the formula is Tinfoil's, shown working in the tests above):

```
client:  random nonce N
guest:   E = nvattest collect-evidence --nonce N
         K = {TLS SPKI fingerprint, HPKE key}
         REPORT_DATA = SHA-256(label || N || H(K) || H(E)) || 32 zero bytes
client:  1. AMD signature and chain; POLICY and TCB minimums
         2. MEASUREMENT == pinned value computed from public artifacts
         3. recompute REPORT_DATA
         4. E: SPDM signature, nonce == N, chain to pinned root, RIM match, revocation
         5. live TLS SPKI == fingerprint in K
```

**Known gap:** nothing in the evidence proves cryptographically that this GPU is attached to this CVM. A dishonest guest could relay evidence from another GPU, so this rests on the measured image being honest. The measured image must also refuse to set GPU ready state unless attestation passed. Verda's docs say `nvattest` no longer sets it, and the guest must run `nvidia-smi conf-compute -srs 1`.

## On-instance test plan

Only an instance can settle these. Each has a pass condition.

| # | Test | Pass condition |
|---|---|---|
| 1 | Boot own image, get report, compare with off-instance digest using public OVMF | Byte-for-byte equal |
| 2 | Resolve CPU signature: try 26/2/1 and 26/0/0 | Exactly one matches; record it per SKU |
| 3 | Confirm `guest_features` with `0x1` | Digest matches, or find the right value |
| 4 | Cmdline customisable? Edit GRUB and any console options, diff `/proc/cmdline` | Know whether `roothash=` can go on the cmdline |
| 5 | `root=UUID` derivation and `/boot/lib/systemd` marker behaviour | Same UUID across boots for own image |
| 6 | Stability over at least 10 Shutdown and Start cycles, across hosts and SKUs; record vCPU per SKU | Finite known set of digests |
| 7 | `/boot/OVMF.amdsev.fd` SHA-256 against the GitHub release | Equal, and report matches public file |
| 8 | Report fields: `POLICY` debug, SMT, VMPL, TCB, `platform_info`, `host_data`, version | Debug off, values recorded as policy |
| 9 | VCEK available in-guest for stapling (extended report), and outbound reach to KDS and RIM | Server can build a stapled bundle |
| 10 | Boot with dm-verity root hash in initrd; corrupt one rootfs block | Measured digest pinned, corrupted read fails |
| 11 | Kernel update behaviour | Know when and how the digest changes |
| 12 | `nvattest collect-evidence --nonce N`, then verify with `--gpu-evidence-source file` off-instance | Passes with the correct nonce, fails with a wrong one |
| 13 | RIMs available for Verda's driver and VBIOS; GB20X and B200 behaviour; NVSwitch evidence | RIM match on each SKU |
| 14 | Ready-state: refuse workload until attestation passes | CUDA fails before, works after |
| 15 | QEMU or host updates over time | Monitored; digest set updated on change |
| 16 | Read NVIDIA Product-Specific Terms for CC | Terms permit the plan |

## Limitations

**How the laptop differs from the target:**
- macOS arm64, no SEV-SNP or TDX hardware, no GPU, no QEMU launch. Every digest computed here for Verda is a prediction.
- Real-report reproduction used Tinfoil's OVMF, kernel and `-cpu EPYC-v4`. Verda's OVMF has large-BAR changes, and its CPU settings differ.
- The target would be Linux under Tor. Here a local logging proxy stood in for Tor, so circuit latency, exit behaviour and failure handling were not tested. The tested traffic volume is small, but timing under Tor is unknown.

**What the results do and do not show:**
- Reproduction shows `MEASUREMENT` is a function of public inputs on real hardware. It does not show a Verda host uses the same VMSA inputs.
- Offline verification shows the document is authentic for its nonce. It does not show liveness.
- Sample sizes are small, and all live Tinfoil tests ran in one window of about 20 minutes. Results describe `tinfoil-go` v0.17.0-rc.8, a pre-release, and Tinfoil's docs mark V3 CLI attestation as beta.
- Three model enclaves (deepseek, gpt-oss, llama3) returned no response. The docs say these hosts restrict inbound connections, and I did not try to get around that.
- The `MEASUREMENT` of an image proves only what was booted. It says nothing about image contents unless someone audits them, nor about AMD or NVIDIA hardware being sound.
- `sandbox-exec` is deprecated, though the controls showed it working here.
- `measure-image-action`, `cvmimage` and the NVIDIA root files came from unpinned `main`. Read or compared only.
- The Turin chain file from AMD and the copy in `tinfoil-go` have different file hashes (`cc9a52fb...`, `3bc97ef8...`) but the same two DER certificates. The Genoa files are byte-identical.

## Claims

| Claim | Status | Evidence file |
|---|---|---|
| `verda-cloud` org is Verda's | confirmed by test | `downloads/verda_docs/docs_cc.html` |
| Published Verda AMD and TDX firmware hashes match docs | confirmed by test | `output/A1_firmware_hashes.txt` |
| Verda firmware rebuilds reproducibly | inconclusive (not attempted) | `downloads/firmware/amd/PROVENANCE.txt` |
| Verda AMD OVMF supports the SEV hashes table | confirmed by test | `output/A3_firmware_inspect.txt`, `output/A3_negative_control.txt` |
| Exact Verda flags are deterministic off-instance (n=10, n=3) | confirmed by test | `output/A3_verda_flags_run.txt`, `output/A3_sensitivity_log.txt` |
| A one-bit or one-byte change to kernel, initrd or cmdline changes the digest (26 of 26) | confirmed by test | `output/A3_sensitivity.csv` |
| `sev-snp-measure` reproduces real AMD-signed measurements from public artifacts (4 of 4) | confirmed by test | `output/A3_real_report_repro.txt`, `output/B/B_snp_sig_independent.txt` |
| Wrong inputs do not reproduce them (44 of 44 diverged) | confirmed by test | `output/A3_real_report_repro.txt` |
| VMSA CPU signature follows `-cpu`, not silicon (Tinfoil) | confirmed by test | `output/A3_real_report_repro.txt` |
| Off-instance digest matches a real Verda report | inconclusive (no instance) | `output/A3_verda_flags_run.txt` |
| A third party can compute Verda's expected digest purely from public artifacts | inconclusive (method works; host inputs unconfirmed) | `downloads/verda_docs/docs_cc.md`, `output/A3_real_report_repro.txt` |
| TDX `MRTD` from public TDVF equals the docs value | confirmed by test | `output/A3_tdx_mrtd.txt` |
| A root hash in cmdline or initrd changes the digest | confirmed by test | `output/A3_sensitivity.csv`, `output/A3_real_report_repro.txt` |
| dm-verity enforced at boot on Verda | inconclusive (no boot) | none |
| Verda customers can change the measured cmdline | inconclusive (read only; no boot) | `downloads/verda_docs/scripts/` |
| Verda's sample continues after a failed AMD signature check | confirmed by test (stub of the shell construct; Verda's script not run) | `output/A2_setE_reproduction.txt` |
| Tinfoil CLI verifies the public endpoint with no API key (n=7) | confirmed by test | `output/B/R1_direct.json`, `output/B/R9_gpu_enclave.json` |
| CLI release is Sigstore-attested to its tag and commit | confirmed by test | `output/B/B_cli_sigstore_verify.txt` |
| `tinfoil-go` verifies a captured document with network denied (2 documents) | confirmed by test | `output/B/B_offline_verify_sandboxed.txt` |
| Forged channel keys or device evidence are rejected, even with unsigned hashes recomputed (4 of 4) | confirmed by test | `output/B/B_offline_verify_sandboxed.txt` |
| Report signature verifies independently of Tinfoil code, and tampering breaks it (4 reports) | confirmed by test | `output/B/B_snp_sig_independent.txt` |
| With a pinned host, the verifying process contacts only the enclave (n=6) | confirmed by test (process only; macOS `trustd` not observable) | `output/B/R1_lsof.txt`, `output/B/proxy_open.log` |
| No AMD, Sigstore, GitHub or NVIDIA contact, including on a GPU enclave | confirmed by test (same caveat; strongest for the SDK under the loopback-only sandbox, run S6) | `output/B/proxy_sdk.log`, `output/B/proxy_open.log`, `output/B/R9_gpu_enclave.stderr` |
| SDK works over SOCKS or CONNECT with no direct connection or local DNS (8 of 8) | confirmed by test | `output/B/B_sdk_proxy_runs.txt`, `output/B/proxy_sdk.log` |
| CLI `attestation verify` keeps all traffic on the proxy | refuted by test | `output/B/S7_cli_socks5h_sandbox.out`, `output/B/R3_socks.lsof` |
| Verification works over real Tor | inconclusive (not tested) | none |
| Default verifier rejects machines outside Tinfoil's inventory | inconclusive (read only; 4 of 4 observed chips are listed; reject path not run) | `output/B/B_fleet_inventory.txt` |
| `tinfoil-go` can verify a self-hosted Verda image with custom references | inconclusive (refused on public document format; needs v3 collateral) | `output/B/B_embedded_platform_try.txt` |
| Client verifier appraises GPU evidence itself | inconclusive (read only; no appraisal code found) | `downloads/tinfoil_src/tinfoil-go/verify/` |
| GPU evidence is bound to the CPU report by `REPORT_DATA` | confirmed by test | `output/B/B_offline_verify_sandboxed.txt`, `output/B/C_gpu_evidence_check.txt` |
| Real SPDM GPU evidence verifies off-instance: signature, nonce, chain (8 of 8 GPUs, no NVIDIA contact) | confirmed by test (Tinfoil's evidence, not Verda's) | `output/B/C_gpu_evidence_check.txt` |
| GPU evidence chain ends at NVIDIA's published root | confirmed by test | `downloads/nvidia_docs/verifier_device_root.pem`, `output/B/C_gpu_evidence_check.txt` |
| `nvattest` evidence exports from Verda and verifies off-instance with RIM and OCSP | inconclusive (documents only) | `output/C_nvattest_doc_extract.txt`, `downloads/verda_docs/docs_cc.md` |
| A client can avoid an NVIDIA service key by using the local verifier | inconclusive (documents only) | `output/C_nvattest_doc_extract.txt` |
| GPU is cryptographically proven attached to this CVM | inconclusive (no such proof found; relies on measured image) | `downloads/tinfoil_src/`, `output/B/C_gpu_evidence_check.txt` |
