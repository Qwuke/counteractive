# Trusted execution and NVIDIA GPU confidential computing as the trust root

Evidence basis, as of 2026-11-06. This is a source-only investigation and nothing was run. "Cited Findings" means read from the linked source; "(secondary)" marks items not confirmed at a primary source. "Inferences" are my reasoning. "Gaps" are unknowns. Attack results are the authors' published experiments and were not reproduced here. Many NVIDIA docs pages are undated; where so, the version is given instead.

## What does NVIDIA officially support on Blackwell for the two target SKUs?

### Takeaway
Both target SKUs are officially CC-capable, but only through bounce buffers. RTX PRO 6000 is single-GPU only, and Verda offers it on SEV-SNP only. B200 supports single-GPU and multi-GPU passthrough, on SNP or TDX. "Protected PCIe" is Hopper-only, so it does not apply to B200, and no NVIDIA release note I read says TEE-IO has shipped.

### Cited Findings
- **Platform matrix (Supported Platforms v1.0.0, undated):** CPUs are Intel TDX (Emerald Rapids, Granite Rapids) or AMD SEV-SNP (Milan, Genoa, Turin). RTX PRO 6000 and B200 run on either. CC single-GPU applies to both; CC multi-GPU applies to B200. Host is Ubuntu 26.04 and guest is Ubuntu 24.04 LTS. — [NVIDIA Supported Platforms](https://docs.nvidia.com/confidential-computing/virtual-machines/v1.0.0/supported-platforms.md)
- **RTX PRO 6000 first supported in R580 TRD1 (RN-12523-001_r580_02, Aug 2025):** "This release introduces SPT support for RTX PRO 6000 Blackwell Server Edition... A bounce buffer stages encrypted data transfers between the GPU device and CVM. Transactions between the GPU and external video memory DRAM are encrypted. Key Rotation is not currently supported in this mode for Blackwell." VBIOS 98.02.81.00.01. — [R580 notes PDF](https://docs.nvidia.com/580trd1-trusted-computing-solutions-release-notes.pdf), [R580 md](https://docs.nvidia.com/confidential-computing/release-notes/r580-trd1.md)
- **Blackwell multi-GPU arrived in R590 TRD1:** "This release introduces support for Blackwell Multiple GPU Passthrough with a Bounce Buffer (MPT CC) mode" on HGX B200 and B200-850 8-GPU AC SKUs. RTX PRO 6000 stays SPT CC only (VBIOS 98.02.9E.00.01). TEE-IO is not mentioned. — [R590 notes](https://docs.nvidia.com/confidential-computing/release-notes/r590-trd1.md)
- **R595 TRD1 (undated; CUDA 13.2, driver 595.58.03):**
  - RTX PRO 6000 Server Edition and Server Edition LC are SPT CC only, VBIOS 98.02.AF.00.02.
  - B200 (AC, B200-850 AC, PC) has SPT CC and MPT CC, VBIOS 97.00.E4.00.XX, with Local GPU Verifier 2.6.3+ for MPT.
  - MPT passes "up to eight GPUs" to a CVM, and these communicate "over encrypted NVLink connections".
  - Host kernels are Intel Ubuntu 25.10 / 6.17+ and AMD Ubuntu 25.04 / 6.14+.
  - The GPU-Ready bit is "always set when devtools mode is enabled".
  - — [R595 notes](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md), [release index](https://docs.nvidia.com/confidential-computing/release-notes.md)
- **PPCIe is Hopper-only in those notes, and it is the weaker mode:** "GPU-GPU communications over the NVLink or NVSwitch interconnect are not encrypted." — [R595 notes](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md)
- **TEE-IO is future tense in NVIDIA's whitepaper:** "In the future, as an alternative to the bounce buffer method, inline encryption can be used with systems that have Blackwell B100/B200 GPUs and TDISP/IDE-compatible CPUs." (WP-12554-001 v1.3, Aug 14 2025). — [NVIDIA whitepaper](https://docs.nvidia.com/nvidia-secure-ai-with-blackwell-and-hopper-gpus-whitepaper.pdf)
- **TEE-IO ecosystem:** Intel says "Nvidia plans to support TEE-IO on Blackwell". AMD SEV-TIO guest patches were still under review in Feb 2026. — [Intel TDX Connect](https://community.intel.com/t5/Blogs/Tech-Innovation/Data-Center/Announcing-Intel-TDX-Connect-Support-on-Intel-Xeon-6/post/1668423), [LKML](https://lkml.iu.edu/2602.3/02156.html), [LWN](https://lwn.net/Articles/1060211/)
- **NVIDIA's stated security goals (whitepaper v1.3):**
  - In scope: software, basic physical, rollback and cryptographic attacks.
  - "Protection against basic physical attacks Ensure that interposers on buses such as PCIe and DDR memory cannot leak data or code."
  - Out of scope: "Sophisticated physical attacks" and DoS.
  - AES-256-GCM on all GPU ingress and egress. Performance counters disabled in CC-On. BMC limited to "anonymous card health-only".
  - — [NVIDIA whitepaper](https://docs.nvidia.com/nvidia-secure-ai-with-blackwell-and-hopper-gpus-whitepaper.pdf)
- **The host chooses CC mode:** `nvidia_gpu_tools.py --set-cc-mode=<on|off|devtools>`. "You can't set the GPU mode from a host that boots with Secure Boot enabled." — [GPU mode guide](https://docs.nvidia.com/confidential-computing/virtual-machines/deployment-guide/gpu-mode.md)
- **Licensing:** "To use NVIDIA Confidential Computing in production, you must have a valid license." — [NVIDIA licensing](https://docs.nvidia.com/confidential-computing/virtual-machines/licensing.md)
- **Confidential Bare Metal is the wrong product for this design:** "CBM does not isolate tenant end-user data from the model owner or the model owner's approved software", and its CPU memory encryption is "Confidentiality only; no integrity or replay protection". — [CBM threat model](https://docs.nvidia.com/confidential-computing/bare-metal/v1.0.0/threat-model.md)
- **Verda's SKUs:** RTX PRO 6000 is single-GPU on SEV-SNP only. B200 and B300 are multi-GPU on SEV-SNP and TDX. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/), [Verda blog, Oct 14 2026](https://verda.com/blog/confidential-computing-b300-b200)

### Inferences
- Choosing RTX PRO 6000 on Verda also chooses SEV-SNP. A TDX-versus-SNP choice exists only on B200.
- The assignment's phrase "multi-GPU Protected PCIe" on B200 is wrong for Blackwell. The mode is MPT CC.
- A one-small-model PoC does not need MPT. SPT on either SKU is the simpler trust surface, with no NVLink or fabric evidence to appraise.
- Because the host sets CC mode, the guest and client must learn the mode only from attestation. Devtools mode must be rejected explicitly, since the ready bit does not distinguish it.
- VBIOS values moved in each RTX PRO release (R580, R590, R595). A client or enclave policy pinned to RIMs needs a plan for fleet VBIOS and driver churn.

### Gaps
- Whether RTX PRO 6000 GDDR7 "encrypted" DRAM has integrity or freshness. The sources say only "encrypted".
- Whether RTX PRO key rotation is still absent in R595. R580 says it is absent, and R590 does not address it for Blackwell.
- The Secure AI Compatibility Matrix and the licence's Product-Specific Terms were not read, so whether a Verda tenant's use is covered, and on what terms, is unconfirmed.
- NVIDIA lists guest Ubuntu 24.04 LTS, while Verda's encrypted image flow is Ubuntu 26.04 only. Whether that combination is officially supported is unconfirmed.

## How does a guest, and then a remote client, verify CPU and GPU evidence and bind them to the channel key?

### Takeaway
There is no cryptographic CPU-GPU binding in the evidence on bounce-buffer Blackwell. The binding is transitive: a measured guest appraises the GPU, and the client trusts that guest through CPU attestation. If CPU evidence can be forged, GPU evidence proves nothing, and TEE.fail demonstrated this end to end on H100.

### Cited Findings
- **NVIDIA's own limits:** GPU attestation "does not validate the CPU TEE or the tenant's workload"; CPU evidence "does not establish the state of the attached GPUs"; attestation "does not establish that approved code is free of vulnerabilities". The ready state is set by a guest user and is "not evidence that the release policy is satisfied". "A valid signature alone does not establish that a firmware version is acceptable." The prescribed order is to verify CPU, verify every GPU, confirm approved guest software, then release over a protected channel. — [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md)
- **Tenant in the TCB:** the trust boundary includes guest software and "anyone with access to protected assets inside the guest", while the host OS, hypervisor and VMM are outside. — [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md)
- **Verda guidance:** CPU and GPU "are attested separately" and no binding is described. The verifier must choose the nonce, reference values are pinned by you, and `OUT_OF_DATE` should never be accepted. Tools are `snpguest` v0.10.0, `measure_boot.sh`, `tdx-attest` v0.6.0 and `nvattest attest --device gpu --verifier remote`. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)
- **The relay attack (H100, not Blackwell):** "the attestation does not identify what the GPU is running". The authors ran vllm-proxy outside TDX with a local NVIDIA 3060, forged a TDX quote and fetched GPU attestation on demand from a rented H100: "As NVIDIA does not bind the H100 to identities of specific VMs, our malicious VLLM-PROXY successfully passes both TDX and CC attestations." — [TEE.fail paper §8.3](https://tee.fail/files/paper.pdf)
- **Driver source (`spdm.c`, main branch):** the libspdm certificate callback is a stub returning NV_TRUE ("Verifier is responsible for verifying the certificate chain"), and `spdmVerifyDeviceCertChain_IMPL` separately verifies against "the FSP L1 certificate built into RM". The session starts with `SPDM_KEY_EXCHANGE_REQUEST_NO_MEASUREMENT_SUMMARY_HASH`. — [spdm.c](https://raw.githubusercontent.com/NVIDIA/open-gpu-kernel-modules/main/src/nvidia/src/kernel/gpu/spdm/spdm.c)
- **Meta (Trail of Bits, Aug 26 2025, with fix review):**
  - TOB-WAPI-28 (High, Resolved): "WhatsApp TEEs do not currently generate or verify any attestation data from the NVIDIA GPUs". The exploit scenario is a malicious PCI passthrough device posing as a CC GPU.
  - The fix runs the NVIDIA attestation utility in the CVM before the service starts, with RIMs baked into the build. Runtime RIM provisioning and revocation checks are planned.
  - TOB-WAPI-7: client_random nonce in the SNP report.
  - TOB-WAPI-8: VCEK extension fields validated against reported TCB.
  - — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **Tinfoil:** the boot sequence runs nvattest, and "If the CPU fails to verify the GPU's attestation, it aborts the boot process and the enclave does not start." It states "clients verifying the CPU attestation are also transitively verifying the GPU configuration." — [Tinfoil attestation architecture](https://docs.tinfoil.sh/verification/attestation-architecture.md)
- **Tinfoil source:** `CollectGPUEvidence` takes a 32-byte caller nonce and `RequiresNVSwitchEvidence` is never true on Blackwell. V2 REPORT_DATA is a 32-byte TLS fingerprint plus a 32-byte HPKE key; v3 derives REPORT_DATA from endorsed-section hashes and the nonce. — [gpu.go](https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/tinfoil/internal/attestation/gpu.go), [attestation.go](https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/tinfoil/internal/attestation/attestation.go)
- **Apple:** the threat model assumes an attacker can gain physical access to a node. Apple images components "before each server is sealed and its tamper switch is activated", and requires non-targetability: "An attacker should not be able to attempt to compromise personal data that belongs to specific, targeted Private Cloud Compute users without attempting a broad compromise of the entire PCC system." — [Apple PCC blog](https://security.apple.com/blog/private-cloud-compute/)

| System | CPU root | GPU handling | Channel binding | Physical story |
|---|---|---|---|---|
| Meta | SNP, VCEK, TOFU host enrolment | CVM appraises at boot; RIMs baked in | TLS client_random in report | Host allowlist, log; "cryptographically verifiable host attestation" planned |
| Tinfoil | SNP or TDX | Enclave appraises at boot and aborts on failure | TLS and HPKE keys in REPORT_DATA | FAQ concedes hardware attacks are the top risk |
| Apple | Own silicon | No discrete GPU-CC seam (my inference) | Node key in attested flow | Imaging, tamper switches; not replicable on rented hardware |

### Inferences
- The PoC should follow the Meta/Tinfoil pattern. The enclave appraises GPUs and refuses to serve otherwise, and the client verifies CPU evidence, which commits to the measured image and to channel keys. The client should not independently fetch GPU tokens, since a separate GPU token adds little unless it is bound to the same report.
- Adding a hash of the GPU evidence to REPORT_DATA would help audit, but it would not create hardware binding. Only TEE-IO/TDISP-style device binding could do that.
- The relay needs a forged CPU quote, so it is reachable where CPU attestation keys can be extracted (TDX under TEE.fail or DDRop), or via an unpatched SNP firmware forgery. Blackwell was not tested in that paper, but I found nothing in NVIDIA's docs suggesting the VM-identity gap differs on bounce-buffer Blackwell.

### Gaps
- Whether the SPDM session the driver establishes is cryptographically tied to the evidence nvattest appraises. I did not trace the HAL code (`spdmGetCertificates_HAL`) or nvattest's evidence handling. A malicious passthrough device is the case this matters for.
- Whether Tinfoil v3 commits GPU evidence into REPORT_DATA. That code is in tinfoil-go, which I did not read beyond the README.

## Who controls the measurement on a rented CC instance, and what must hold for a third party to verify a tenant-chosen image?

### Takeaway
Verda lets the tenant supply kernel and initramfs through direct boot, but pins firmware on SEV-SNP ("You cannot bring your own"). So third-party verification is feasible in principle, but on Verda SNP the ACPI tables stay unmeasured and must be defended in the guest kernel. Meta and Tinfoil both solved that problem by replacing the firmware, which Verda SNP does not allow.

### Cited Findings
- **Verda boot chain:** QEMU direct kernel boot (`-kernel/-initrd/-append`) with SEV-SNP kernel hashes, where SHA-384 of OVMF, kernel, initramfs and cmdline enters the launch measurement. Kernel and initrd come from the tenant's OS volume. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)
- **Verda firmware:** SEV-SNP uses one public OVMF build (`verda-cloud/edk2`, branch `amdsev-large-bars`, release `verda-amdsev-largebars-stable202605`). TDX uses TDVF (`inteltdx`, `verda-inteltdx-stable202605`, no Secure Boot), with kernel and initramfs in RTMR1 and RTMR2. Verify by SHA-256 or rebuild from commit. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)
- **Verda custom image flow:** `00_build_image.sh`, `01_setup_initramfs_ssh.sh`, `02_flash.sh` with RAM-only `dropbear`. The encrypted variant is LUKS2 with a per-boot SSH key bound into a SNP report, and the operator unlocks after every restart. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)
- **Verda silence:** the docs do not cover who sets `-append`, ACPI, SMBIOS, fw_cfg, SMT, ciphertext hiding, chip IDs, physical security or Proof of Cloud. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)
- **BadAML (secondary):** CVE-2026-100839 against Edgeless Contrast before 1.18.0 on SEV-SNP. ACPI contents are not measured there, whereas "Metal-QEMU-TDX is not affected because ACPI table contents are measured into RTMR 0 by OVMF on Intel TDX". The fix sandboxes the AML interpreter. — [SentinelOne](https://www.sentinelone.com/vulnerability-database/cve-2026-100839/), [Endor](https://www.endorlabs.com/vulnerability/ghsa-g9ww-x58f-9g6m), [kernel patch proposal](https://patchew.org/linux/20260411224412.2236817-1-40759640+bfoing@users.noreply.github.com/)
- **Meta's answer:** TOB-WAPI-17 (High, Resolved): "CVMs now use a custom bootloader based on the Project Oak Stage0 firmware project. The expected ACPI SSDT digest is included in the SEV-SNP launch digest measurement." — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **Tinfoil's answer (Oct 25 2026):** replaced OVMF (over 1M lines of C) with about 300 lines of assembly via IGVM. ACPI and page tables are generated at build time, there is no host-written AML, and RAM size and CPU count are unmeasured host inputs that firmware checks. — [Tinfoil blog](https://tinfoil.sh/blog/2026-10-25-less-is-more)
- **Tinfoil image practice:** "Every byte of the final image is explicitly declared"; Nix build; config hash on the cmdline; dm-verity root; read-only disks. — [cvmimage](https://github.com/tinfoilsh/cvmimage), [attestation architecture](https://docs.tinfoil.sh/verification/attestation-architecture.md)
- **Reproducibility is a recognised finding:** TOB-WAPI-18 (High, Partial). — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **Client-enforceable SNP fields:** PlatformInfo has `smt_enabled` 0, `tsme_enabled` 1, `ciphertext_hiding_enabled` 4, `alias_check_complete` 5, `tio_enabled` 7. Policy has `smt_allowed` 16, `debug_allowed` 19, `single_socket_required` 20, `ciphertext_hiding` 24, `page_swap_disabled` 25. The report has `reported_tcb`, `chip_id`, `report_data`, `measurement`, and `key_info` (VCEK or VLEK). — [virtee/sev](https://raw.githubusercontent.com/virtee/sev/main/src/firmware/guest/types/snp.rs)
- **A vendor marketing claim to treat carefully:** "Even Verda's own infrastructure cannot read plaintext data inside a running CVM". — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)

### Inferences
- A third-party client must be able to recompute or look up the following, all from a public, reproducible build:
  - SNP: OVMF hash, kernel, initrd, cmdline, vCPU count and type, policy, and a TCB floor.
  - TDX: MRTD, RTMR0-3 with event-log replay, and a TCB status floor.
  - A dm-verity root committed through the measured cmdline or initrd, covering the model weights too.
- **Tinfoil portability:** Tinfoil's newer IGVM image likely cannot run on Verda SNP as is, because firmware is pinned. In the modular-backend design, "Tinfoil" is better modelled as a separate backend (Tinfoil's service or fleet) than as Tinfoil's image on Verda.
- **Guest-side ACPI hardening:** on Verda SNP, the only RTX PRO 6000 option, ACPI defence has to live in the measured guest kernel (AML sandboxing, or an allowlist of expected tables). This work is not done for you. On Verda TDX, ACPI is probably covered by RTMR0, though that also means host-side variation will change RTMR0 and needs pinning.
- **Tenant-as-adversary:** the promise is that the service operator cannot read content, and the operator here is the tenant. Verda's dropbear and LUKS unlock flows are fine for development but contradict the promise in production. The production image needs no SSH or admin path and public, verity-checked weights, otherwise a passing attestation proves little about operator access.

### Gaps
- Who controls `-append` on Verda, and whether Verda injects anything into it. If Verda does, the measurement changes per instance or cannot be pinned.
- Whether Verda's measurement is stable across instances and boots (vCPU count, topology).
- Whether Verda's launch policy can set `page_swap_disabled`, whether SMT is off, and whether ciphertext hiding is on. The docs are silent.
- Whether the upstream Linux AML hardening patch has landed. I only saw a proposal.

## Which known attacks apply, and do they forge attestation?

### Takeaway
Every current server TEE lacks memory freshness, and low-cost DDR5 interposers break both TDX and SEV-SNP. TDX attestation is forgeable with physical access, and SNP attestation has been forgeable through firmware bugs that are now patched. No public attack was found against Blackwell GPU attestation keys, but that is absence of evidence, and RTX PRO's off-package GDDR7 is not covered by the on-package reasoning that protects HBM.

### Cited Findings

| Attack | Attacker position | Applies to | Forges attestation? | Patch status |
|---|---|---|---|---|
| [TEE.fail](https://tee.fail/) (Oct 2025) | DDR5 interposer under $1000 plus root | TDX and SGX on Xeon Silver 4509Y; SNP on EPYC 9015 with Ciphertext Hiding | TDX yes: "first end-to-end provisioning certification key extraction attack on a TDX system in a fully trusted UpToDate status". SNP: OpenSSL ECDSA key recovered; no VCEK extraction claimed | None. Intel: out of threat model. [Paper](https://tee.fail/files/paper.pdf) |
| [DDRop](https://ddropattack.eu/) (Sep 14 2026, CCS 2026) | "brief, one-time physical access... such as during routine maintenance", $159 BOM, plus root and BIOS | TDX 1.5.16, Xeon Silver 4509Y; SNP 1.58.4, EPYC 9555; Scalable SGX | TDX yes, case study forges reports and forces debug mode. SNP: relocation primitive only; forgery shown for TDX | No fix. "comprehensive mitigations require both cryptographic integrity and freshness protections". [Paper](https://ddropattack.eu/ddrop.pdf), [Intel notice](https://www.intel.com/content/www/us/en/security-center/announcement/intel-security-announcement-2026-08-11-001.html), [AMD-SB-3048, unread](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3048.html) |
| [Battering RAM](https://batteringram.eu/) (Sep 2025) | Interposer under $50 | DDR4 only; SGX, SNP | SNP yes via digest replay | Not fixable; DDR4 only |
| [WireTap](https://wiretap.fail/) | Interposer | SGX on DDR4 | SGX key | None |
| [BadRAM](https://badram.eu/) | SPD access, about $10 | SNP, DDR4 and DDR5 | Yes via replay | CVE-2024-21944, AMD-SB-3015; boot check, `alias_check_complete`; TDX and Scalable SGX not exploitable |
| [Heckler](https://www.usenix.org/conference/usenixsecurity24/presentation/schl%C3%BCter) / [WeSee](https://arxiv.org/html/2404.03387v1) (2024) | Malicious hypervisor | SNP and TDX (Heckler); SNP (WeSee) | No, guest takeover | Guest kernel patches; [one CVE noted as remaining](https://usenix.org/system/files/usenixsecurity24_slides-schluter.pdf) |
| [CacheWarp](https://www.usenix.org/conference/usenixsecurity24/presentation/zhang-ruiyi) | Hypervisor | SEV-ES and SNP Zen 1-3; [not Genoa](https://www.bleepingcomputer.com/news/security/new-cachewarp-amd-cpu-attack-lets-hackers-gain-root-in-linux-vms/) | Not stated | CVE-2023-20592, Milan microcode |
| [RMPocalypse](https://www.amd.com/en/resources/product-security/bulletin/AMD-SB-3020.html) (Oct 2025) | Admin or hypervisor | SNP Zen 3-5 | ["all integrity guarantees... void"](https://thehackernews.com/2025/10/rmpocalypse-single-8-byte-write.html) (secondary) | CVE-2025-0033, AMD-SB-3020 |
| [StackWarp](https://cispa.de/en/stackwarp) (Jan 2026) | Software, needs SMT | SNP Zen 1-5 | Guest takeover; attestation not mentioned | [Hot-loadable microcode](https://misc0110.net/files/stackwarp_sec26.pdf); CVE-2025-29943 ([secondary](https://www.theregister.com/2026/01/15/stackwarp_bug_amd_cpus/)) |
| [Fabricked](https://xca-attacks.github.io/fabricked/) (Apr 2026) | UEFI plus hypervisor, no physical access | SNP Zen 5 confirmed, Zen 3/4 assumed; not TDX or CCA | Yes: "FABRICKED also forges attestation reports" | CVE-2025-54510, [AMD-SB-3034](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3034.html). TCB floor values below are secondary |
| [BREAKFAST](https://xca-attacks.github.io/breakfast) (S&P 2026) | Hypervisor, confused-deputy PSP | SNP EPYC 9135 and EPYC 9124 | Yes: ["fake attestation and enable debug on production CVMs"](https://www.shwetashinde.org/publications/breakfast_oakland26.pdf) | CVE-2025-61971, CVE-2025-61972; fix versions unread |
| [One Glitch](https://arxiv.org/abs/2108.04575) (2021) | Physical voltage glitch | Zen 1-3 | Yes, VCEK seeds | Trail of Bits says extracted VCEKs circulate; Zen 4/5 status unknown |

- **Fabricked patch levels (from a search summary of the bulletin; direct fetch timed out, so verify before use):** TurinPI 1.0.0.8 with TCB[SNP] at least 0x5, GenoaPI 1.0.0.H at least 0x1C, MilanPI 1.0.0.J at least 0x1D plus mitigation bit 4; CVSS 5.9. — [AMD-SB-3034](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3034.html), [Thomas-Krenn](https://www.thomas-krenn.com/en/wiki/AMD_Security_Vulnerabilities_-_April_2026)
- **Fabricked and Intel:** "a similar vulnerability has been found internally on Intel systems" (INTEL-SA-00960). — [Fabricked site](https://xca-attacks.github.io/fabricked/)
- **DDRop scope limit:** the evaluation "relies on dual-socket machines"; generalising to single-socket is listed under limitations. — [DDRop paper §7.2](https://ddropattack.eu/ddrop.pdf)
- **DDRop on software mitigations:** they "ultimately cannot address the underlying hardware vulnerability". Disabling unneeded APIs (SNP's PAGE_SWAP_DISABLE is the named example) raises the bar. — [DDRop paper](https://ddropattack.eu/ddrop.pdf)
- **TDX CI mode does not help:** per the DDRop paper it "would only prevent tampering across different key domains". — [DDRop paper](https://ddropattack.eu/ddrop.pdf)
- **Interposer attacks and the memory engine:** "impossible to issue a microcode to update the inner workings of the memory encryption engine". — [TEE.fail site](https://tee.fail/)
- **Intel on DDRop:** "fall outside Intel's standard threat model", pointing to Platform Ownership Endorsement. The notice's ID (2026-09-14-001) and URL date (2026-08-11) disagree. — [Intel notice](https://www.intel.com/content/www/us/en/security-center/announcement/intel-security-announcement-2026-08-11-001.html)
- **NVIDIA and interposers:** DDRop says GPU "HBM is integrated directly into the package... inaccessible to a physical adversary". That text concerns HBM. — [DDRop paper](https://ddropattack.eu/ddrop.pdf)
- **GPU-CC internals (H100, Jul 2025; MLSys 2026):**
  - "GPU-CC does not implement runtime memory encryption"; it relies on a PCIe firewall.
  - RPC metadata is plaintext, so element counts reveal RPC type.
  - Transfer timing is bimodal over 4,394 transfers.
  - SEC2 GPFIFO/GPPUT is neither signed nor encrypted, judged DoS only. Reported to PSIRT.
  - — [arXiv 2507.02770](https://arxiv.org/html/2507.02770v1), [MLSys PDF](https://gzs715.github.io/pubs/GPUCC_MLSYS26.pdf)
- **GPU Rowhammer (secondary):** GPUBreach, GDDRHammer and GeForge (Apr 2026) target GDDR6, with no CVE as of Apr 5 2026. GPUHammer's authors argue MIG and CC prevent co-location. GDDR7 has always-on on-die ECC, and RTX PRO 6000 and H100 were not tested. — [CSA note](https://labs.cloudsecurityalliance.org/research/csa-research-note-gpubreach-gpu-rowhammer-privilege-escalati/), [barrack.ai](https://blog.barrack.ai/gddrhammer-geforge-gpu-rowhammer-gddr6/), [BleepingComputer](https://bleepingcomputer.com/news/security/nvidia-shares-guidance-to-defend-gddr6-gpus-against-rowhammer-attacks)
- **Tinfoil's public position:** "Hardware-level attacks are the most credible threat to confidential computing today."; side channels and "I/O pattern leakage" are not protected; "Confidential computing reduces risk, but it does not eliminate all risk." — [Tinfoil FAQ](https://tinfoil.sh/security-and-privacy-faq)

### Inferences
- **NVIDIA's goal versus the record:** the whitepaper's goal on DDR interposers concerns the GPU's own scope, but the CPU side of the same deployment is DDR5. An interposer on CPU DIMMs reads or tampers with the CVM that holds prompts in plaintext, so GPU protection does not save the session.
- **TDX versus SNP is a real trade-off, not a clear win:**
  - SNP signs in the ASP. Interposer work has not extracted a VCEK on Zen 5, and DDRop's forgery case study is TDX-specific. But SNP has had four software or firmware-level breaks in 2025-26, of which Fabricked and BREAKFAST forged attestation before patches, and ACPI is unmeasured under pinned OVMF.
  - TDX measures ACPI (per the BadAML sources), but attestation keys live in DRAM-backed SGX enclaves, so one extracted PCK can forge universally unless the client allowlists PPIDs.
  - For an anonymous client, SNP's failures are mostly closed by a TCB floor and SMT check. TDX's physical failure has no patch.
- **Mandatory client policy on SNP:** enforce a TCB floor covering AMD-SB-3020, -3027, -3034, and BREAKFAST's fixes, cross-checked against VCEK extension fields, plus `smt_enabled` false (StackWarp), `alias_check_complete` (BadRAM), debug disallowed, and preferably `page_swap_disabled`.
- **The HBM argument does not cover RTX PRO:** B200 is HBM3e, on-package. RTX PRO 6000 uses GDDR7 off-package, so DDRop's stated reasoning does not extend to it. R580 says that traffic is encrypted, but I found no statement on freshness or integrity. No published attack was found in either direction.
- **Rowhammer is low relevance here:** whole-GPU passthrough to one CVM removes the co-tenant those attacks need.

### Gaps
- Xeon 6 / Granite Rapids against TEE.fail and DDRop. Neither paper's text mentions Granite Rapids; both tested Xeon Silver 4509Y. Verda's B200 CPU model is undocumented, although a separate study ran B200 on Xeon 6767P.
- Per-cell DDRop results for SNP with PAGE_SWAP_DISABLE and for NVIDIA CC. The table symbols were lost in my text extraction, so read the PDF directly.
- AMD-SB-3027, AMD-SB-3033 ("SEV Firmware Arbitrary Code Execution"), AMD-SB-3029 and AMD-SB-3048 were not retrieved.
- I found no public NVIDIA attestation-key extraction or CC-specific CVE. The [May 2026 bulletin](https://nvidia.custhelp.com/app/answers/detail/a_id/5821/~/security-bulletin:-nvidia-gpu-display-drivers---may-2026) was not read in detail, and a [CVE-2026-0427 feed entry](https://cvefeed.io/vuln/detail/CVE-2026-0427) was inconsistent and unverified. So this is an absence of findings, not evidence of soundness.

## What can an operator with physical access still do, and what is the answer to an extracted attestation key?

### Takeaway
An operator with brief physical access and host control can defeat confidentiality on TDX and SNP, and on TDX can also forge attestation. Proof of Cloud style binding stops relay to a rogue machine, but it does not protect you from the datacenter that holds the genuine machine.

### Cited Findings
- **Attacker model:** DDRop requires root on the host, BIOS access, and "brief, one-time physical access to the victim server such as during routine maintenance". — [DDRop paper §3.1](https://ddropattack.eu/ddrop.pdf)
- **Consequence on TDX:** "completely compromise a TD, capture victim ciphertext, turn any TD into debug mode, and forge arbitrary attestation reports". — [DDRop paper §6](https://ddropattack.eu/ddrop.pdf)
- **NVIDIA GPU scope:** "Invasive package probing and fault injection are outside this GPU threat model." — [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md)
- **VCEK lifecycle:** TOB-WAPI-10 (High, Partial): "VCEKs from compromised AMD SEV-SNP processors are freely available online and can be used to sign fraudulent attestation reports. AMD continues to issue VCEK certificates with seven-year validity". The recommendation is an hwID allowlist from VCEK extension OID 1.3.6.1.4.1.3704.1.4: "It is not sufficient to validate the CHIP_ID field of the attestation report". Meta's fix is TOFU enrolment with VCEK digests in a Cloudflare log. — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **VLEK caution:** Trail of Bits advises against VLEK because a single ASP leak exposes it. — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **DCEA (Proof of Cloud basis, arXiv v2, Mar 3 2026):** binds CVM attestation to platform TPM evidence with two roots of trust, blocks the "mix-and-match proxy attack", and was prototyped on GCP bare-metal TDX with Intel TXT. It needs provider cooperation. — [arXiv 2510.12469](https://arxiv.org/abs/2510.12469)
- **Residual supply-chain trust is recognised in the literature.** — [SysTEX 2026](https://www.net.in.tum.de/fileadmin/bibtex/publications/papers/Rezabek-SysTEX-2026.pdf)
- **Intel POE (draft-02, Sep 28 2026):** signed "by platform owners, CSPs, or fleet managers -- not by Intel". It binds PIID to owner, with no location or custody statement and no in-band revocation. — [IETF draft](https://www.ietf.org/ietf-ftp/internet-drafts/draft-bzb-rats-intel-poe-endorsements-02.txt)
- **Proof of Cloud Alliance:** a member-attested registry mapping hardware IDs to facilities. Only promotional or mirror sources were available. — [zkVerify](https://zkverify.io/blog/why-zkverify-joined-the-proof-of-cloud-alliance), [iExec](https://www.iex.ec/academy/proof-of-cloud), [GitHub](https://github.com/proofofcloud)
- **Cloud provenance precedent:** Flashbots checks Google Cloud provenance. — [Flashbots forum](https://collective.flashbots.net/t/flashbots-attestation-verification-now-supports-google-cloud-provenance-checks/5954)
- **Apple's alternative:** physical controls plus non-targetability via OHTTP relay and single-use blind-signature credentials. — [Apple PCC blog](https://security.apple.com/blog/private-cloud-compute/), [PCC research](https://security.apple.com/blog/pcc-security-research/)

### Inferences
- An interposer attack leaves nothing in the attestation report. I read no mechanism by which a client could detect one, and DDRop's timing-based detection idea is not a deployed product. So a passing attestation cannot show an absence of interposers.
- On a rented cloud, Verda or its datacenter staff, or anyone compelling them, is the party that holds physical access. So the physical-attacker case is the service's actual adversary, not an edge case.
- **Practical mitigation set for this PoC:**
  - Allowlist hardware IDs (VCEK hwID or PPID) enrolled once by TOFU and published in a transparency log, so forged-from-elsewhere evidence fails.
  - Short-lived sessions and ephemeral channel keys, which limit retrospective decryption.
  - Anonymity itself: with Tor and unlinkable credentials, a physically positioned attacker sees content but not identity, unless the content identifies the user. Attack economics then resemble Apple's non-targetability, but only partially, since a fleet of one or a few machines offers little target diffusion.
- That last point matters: the anonymity layer is what keeps a physical break from becoming a targeted identity breach. The TEE and the anonymity layer are independent defences, and neither should be described as sufficient alone.

### Gaps
- Verda documents no datacenter provenance, Proof of Cloud membership, or physical-security controls.
- Proof of Cloud governance could not be verified at a primary source.
- Whether Verda would provide TPM evidence for DCEA-style binding is unknown.

## What is the measured performance overhead of CC-mode LLM inference on recent GPUs?

### Takeaway
Overhead is highly configuration-dependent and hits small models the hardest. Headline numbers of a few percent come from large models, while a small model on default settings has measured 34-39% throughput loss, and much of that is recoverable by tuning.

### Cited Findings
- **8x B200, Xeon 6767P, TDX, driver 595.71.05 (arXiv Sep 2026; authors at Confidential.ai):**
  - MiniMax-M2.7 at 8 GPUs: about 2.8-3.6%.
  - Nemotron-120B single GPU: 1.7-2.8%; with full graph and patches under 1%.
  - **Qwen3-8B bf16 on shipped SGLang: 34.4-38.9%; at concurrency 64, TPOT 5.48 to 8.28 ms (+51%) and TTFT 259 to 409 ms (+58%).**
  - vLLM Qwen3.5-9B: 1.50x with cudagraph, 1.94x eager.
  - Output length sweep: 14.4% at 256 tokens down to 1.1% at 2048.
  - PCIe ceiling about 10 GB/s; model load H2D 3.5 vs 51.6 GB/s; NVLink -11 to -18%; P2P latency 14.5 vs 3.7 µs.
  - Cold start and attestation time not measured; ±2pp noise.
  - — [arXiv 2608.26575v2](https://arxiv.org/html/2608.26575v2)
- **RTX Pro 6000 and B300 on TDX (Phala, Jun 2026):** **RTX Pro 6000 about 10% on dense decode, KV restore TTFT +43%.** B300 dense -13/-14%, MoE -25/-28%, MLPerf-shaped -1.1%, KV restore TTFT +131%. H2D 55.48 to 11.26 GB/s, about 330 µs per crossing. `--no-async-scheduling` leaves about 1% residual (MoE about 5%). Pooled loader 287 s to 8.4 s. I could not find support in its tables for a worker-thread "92%" claim. — [arXiv 2606.23969v1](https://arxiv.org/html/2606.23969v1)
- **Verda vendor benchmark:** B300, SNP, Qwen3.5-397B, SGLang 0.5.12: throughput -1.1 to -5.5%, TPOT +0.5 to +4.4%. No RTX PRO or B200 figures. — [Verda blog](https://verda.com/blog/confidential-computing-b300-b200)
- **H100/H200:** under 7% typical; Llama-3.1-8B 6.85%, and 8.84% on H200 in an earlier version. — [arXiv 2409.03992v4](https://arxiv.org/html/2409.03992v4), [v3](https://arxiv.org/html/2409.03992v3)
- **IBM:** close to par for larger models with pipelined processing. — [IBM Research](https://research.ibm.com/publications/securing-ai-inference-in-the-cloud-is-cpu-gpu-confidential-computing-ready)

### Inferences
- The overhead looks like a fixed per-crossing toll, so fast small models and short outputs lose the most. That is the PoC's exact shape, so vendor "up to 99%" headlines do not apply.
- Token-level streaming over Tor will be dominated by Tor latency rather than CC overhead. Cross-track: this needs the Tor track's numbers.
- The Phala RTX Pro 6000 study ran TDX, while Verda offers RTX PRO on SNP only. No published RTX PRO on SNP number was found.

### Gaps
- No small-model RTX PRO 6000 on SEV-SNP measurement exists in what I found, and no independent B200-on-SNP figure.
- Attestation and cold-start cost, which matter if a client attests per request, was not measured in these studies.

## Which verification steps contact a third party, and can they be stapled so the verifier service cannot see client IP and timing?

### Takeaway
Yes, for the client: AMD and Intel evidence can be appraised offline given stapled collateral, and Tinfoil's v3 client is a working precedent for contacting only the enclave. NVIDIA's tooling defaults to online RIM and OCSP calls and has no documented fully offline mode, so GPU appraisal belongs inside the enclave and away from client timing.

### Cited Findings
- **AMD:** KDS (`kdsintf.amd.com`) serves ARK, ASK and VCEK, and is rate limited. Certificates can be pre-provisioned for offline use, with CRLs handled separately. — [Anjuna air-gapped docs](https://docs.anjuna.io/apm/v3.3/airgapped_attestation.html), [IETF RATS wiki](https://wiki.ietf.org/group/rats/referencevalues/amd-key-distribution-service), [LIT sev-snp-utils](https://github.com/LIT-Protocol/sev-snp-utils)
- **Intel:** collateral comes from PCS or PCCS. dcap-qvl verifies from quote plus collateral plus time, offline, and its default PCCS is Phala's. — [dcap-qvl](https://docs.rs/dcap-qvl/0.6.3), [Intel PCCS](https://cc-enabling.trustedservices.intel.com/intel-sgx-tdx-pccs/02/overview/)
- **Intel collateral lifetime (weak sources):** about 30 days per one source, while one implementation caches 8 hours. — [Automata](https://docs.ata.network/tee-overview/tee-verifiers/intel-sgx-tdx-dcap), [Atlas](https://www.mintlify.com/concrete-security/atlas/protocol/verification-steps)
- **NVIDIA endpoints:** `--verifier local` is the default, with `--rim-url https://rim.attestation.nvidia.com`, `--ocsp-url https://ocsp.ndis.nvidia.com` and `--nras-url https://nras.attestation.nvidia.com`. `verify-token` fetches `<nras-url>/.well-known/jwks.json`. — [nvattest command reference](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/latest/sdk-cli/command-reference.html)
- **NVIDIA redirection points:** `--gpu-evidence-source {nvml|file}`, for evidence "collected ahead of time for testing or as part of a custom attestation workflow"; `NVAT_RIM_STORE_BASE_URL`, `NVAT_OCSP_BASE_URL`, `NVAT_NRAS_BASE_URL`; `RimStore::create_filesystem("/var/lib/nvat/rims")`. That page names `ocsp.attestation.nvidia.com`, which conflicts with `ocsp.ndis`. — [Rust SDK config](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/latest/sdk-rust/configuration.html)
- **NVIDIA revocation and CoRIM:** the OCSP responder comes from AIA, NULL means no revocation check, and disabling signatures is "never appropriate in production". CLI docs call CoRIM experimental. — [CoRIM verifier 2.0.0](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/2.0.0/api/group__local__corim__verifier.html), [CLI intro](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/2.0.0/sdk-cli/introduction.html)
- **Tinfoil v3 precedent:** "The document carries CPU evidence, attested transport keys, and all required verification collateral."; "verify.Verifier appraises a document offline. It opens no connections, holds no cache and is immutable." It uses embedded trust roots and a single fetch of `https://<enclave>/.well-known/tinfoil-attestation?nonce=<nonce>`, with witnesses up to 7 days old. The README does not cover GPU evidence. — [tinfoil-go verify README](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md)
- **Tinfoil older flow:** `kds-proxy.tinfoil.sh` and `github-proxy.tinfoil.sh`, where a proxy "can withhold material but cannot forge it". — [Tinfoil verification docs](https://docs.tinfoil.sh/verification/verification-in-tinfoil), [verifier v0.11.2](https://pkg.go.dev/github.com/tinfoilsh/verifier@v0.11.2)
- **Meta and Trail of Bits on GPU collateral:** RIMs baked into the CVM build; hardcode RIM, root certificate and OCSP signing certificates; validate OCSP expiry with client-provided timestamps; long term, a transparency log for RIMs. — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- **Key partitioning can defeat anonymity:** TOB-WAPI-1/3 note key rotation could de-anonymize; NCC found stale attestation from missing log freshness (the latter from a search summary). — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf), [NCC report](https://www.fox-it.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf)
- **Verda's documented tools make third-party contacts:** GitHub, apt, pipx, the AMD chain fetch, Intel DCAP via `/etc/sgx_default_qcnl.conf`, optional ITA, and NVIDIA remote and OCSP. — [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)

### Inferences
- **Do not use Verda's documented verification commands on the client.** They contact AMD, Intel or NVIDIA from the client's vantage point.
- **Workable pattern:**
  - The enclave appraises the GPU at boot with RIMs in the image and an OCSP response fetched by the enclave on its own schedule, so NVIDIA sees the server and not client timing.
  - The enclave staples VCEK or PCK chain, CRLs and TCB info, plus signed release or transparency proofs.
  - The client ships with pinned roots and measurements, picks a fresh nonce, and verifies offline over the Tor circuit. No client traffic reaches AMD, Intel or NVIDIA.
- **A VCEK fetch would otherwise leak:** chip ID and TCB, plus client IP and timing, to AMD.
- **Residual after stapling:**
  - Revocation freshness is bounded by the stapled validity window, so an enclave or attacker can withhold newer bad news for that window.
  - The client's measurement list must itself arrive privately, ideally shipped with the client or fetched over Tor.
  - Per-client enclave keys or per-client bundles would partition the anonymity set, so keys and configs must be fleet-shared or logged. Cross-track.

### Gaps
- NVIDIA documents no complete offline flow. Whether local verification with file evidence and a filesystem RIM store runs with egress blocked, and how it fails when OCSP is unreachable, needs a run.
- Whether `nvattest` needs a service key in any mode.
- Actual Intel and NVIDIA collateral validity windows at a primary source.

## What must you still trust, and is "provider cannot read content" defensible?

### Takeaway
"Provider can't read content" is not defensible as written against a provider holding physical custody, as the published attacks show. A qualified claim is defensible: remote and software-only operators cannot read content without defeating patched, attested hardware, provided the client enforces its policy.

### Cited Findings
- **The proposal's wording:** the ChatGPT conversation uses "Provider can't read content" and "nobody, including the operator, can read it", later softened to "Designed to prevent operator access under the stated threat model". — [ChatGPT conversation](https://chatgpt.com/share/6aee17a4-d47c-83e9-bd51-066a72c670ea)
- **NVIDIA scopes its model narrowly:** "The host OS, hypervisor, virtual machine monitor, and other tenants are outside this runtime trust boundary", with guest admins inside the TCB. — [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md)
- **Intel treats interposers as out of scope.** — [Intel notice](https://www.intel.com/content/www/us/en/security-center/announcement/intel-security-announcement-2026-08-11-001.html)
- **Availability is not protected:** "our cloud providers control resource allocation and can throttle or terminate enclave access." — [Tinfoil FAQ](https://tinfoil.sh/security-and-privacy-faq)
- **Output size leak:** TOB-WAPI-16 (Low, Unresolved). — [Trail of Bits report](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)

### Inferences
- **Residual trust list:**
  - AMD or Intel silicon, firmware (ASP, or TDX module and quoting enclaves) and PKI.
  - NVIDIA GPU silicon, firmware, RIMs, PKI, and the guest driver, including GSP binaries you cannot audit.
  - The guest image and whoever controls releases (reproducible and audited; no admin path).
  - Verda's pinned OVMF on SNP (measured and rebuildable, but not replaceable).
  - Verda's physical custody, its datacenter and supply chain.
  - Legal compulsion against any of these.
  - Traffic-shape side channels (I/O pattern, timing, output size).
- **Suggested honest wording:** "A remote or software-only operator, including root on the host hypervisor, cannot read content without defeating the attested, patched hardware, if the client enforces measurement, TCB floor and SMT/policy checks. A party with physical access to the server and control of the host can break this without a detectable trace; we reduce the value of that by keeping requests anonymous and unlinkable."
- **Contradictions of the proposal:**
  - Enclave attestation is not an end-to-end cryptographic proof against the host operator.
  - GPU evidence alone proves nothing about the VM.
  - The Tinfoil image is unlikely to drop onto Verda SNP.
  - Blackwell multi-GPU is MPT CC, not PPCIe.
- **What holds up:** client-enforced attestation over Tor with no third-party contact looks feasible. The anonymity goal is less dependent on the TEE than the confidentiality goal, which is good design news.

### Gaps
- Legal and jurisdictional exposure of Verda was not researched, as it is outside this track.

## Claim status and proposed validation

### Takeaway
The support matrix and the attack literature are solid as documents. Verda-specific behaviour is mostly unknown until someone boots an instance. Nothing below was run here, and documentation or source-only claims are marked untested in practice.

### Cited Findings

| Claim | Status | Evidence URLs |
|---|---|---|
| RTX PRO 6000 is officially CC-capable, SPT only | solid; documentation, untested in practice | [R580](https://docs.nvidia.com/confidential-computing/release-notes/r580-trd1.md), [R590](https://docs.nvidia.com/confidential-computing/release-notes/r590-trd1.md), [R595](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md) |
| B200 supports SPT and MPT CC | solid; documentation, untested in practice | [R590](https://docs.nvidia.com/confidential-computing/release-notes/r590-trd1.md), [R595](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md) |
| B200 multi-GPU uses Protected PCIe | false | [R595](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md) |
| TEE-IO is available for Blackwell today | unknown; no NVIDIA note read says so, and all deployments read use bounce buffers | [whitepaper](https://docs.nvidia.com/nvidia-secure-ai-with-blackwell-and-hopper-gpus-whitepaper.pdf), [R595](https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md), [arXiv 2608.26575](https://arxiv.org/html/2608.26575v2) |
| On Verda, RTX PRO 6000 is SEV-SNP only | solid; documentation, untested in practice | [Verda docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| A third party can verify a tenant-chosen image on Verda | conditional; depends on `-append` control and measurement stability; untested in practice | [Verda docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Tenant can supply firmware on Verda SNP | false | [Verda docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| ACPI is unmeasured on SNP with stock OVMF, and is an exploitable path | conditional; secondary sources; untested on Verda | [Endor](https://www.endorlabs.com/vulnerability/ghsa-g9ww-x58f-9g6m), [SentinelOne](https://www.sentinelone.com/vulnerability-database/cve-2026-100839/), [Tinfoil blog](https://tinfoil.sh/blog/2026-10-25-less-is-more) |
| CPU and GPU evidence are cryptographically bound | false for bounce-buffer modes as documented; binding is transitive | [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md), [TEE.fail](https://tee.fail/files/paper.pdf) |
| Transitive binding through a measured enclave is sound | conditional; needs unforged CPU evidence; SPDM-to-evidence link unconfirmed; source reading only | [Tinfoil](https://docs.tinfoil.sh/verification/attestation-architecture.md), [spdm.c](https://raw.githubusercontent.com/NVIDIA/open-gpu-kernel-modules/main/src/nvidia/src/kernel/gpu/spdm/spdm.c) |
| TDX attestation can be forged by a physical attacker with host control | solid; published experiments, not reproduced here | [TEE.fail](https://tee.fail/files/paper.pdf), [DDRop](https://ddropattack.eu/ddrop.pdf) |
| These attacks apply to Granite Rapids Xeon 6 | unknown | [DDRop](https://ddropattack.eu/ddrop.pdf), [TEE.fail](https://tee.fail/files/paper.pdf) |
| SNP attestation has not been forged via interposer on Zen 5 | conditional; no such result published as of this date; firmware forgeries exist | [TEE.fail](https://tee.fail/files/paper.pdf), [Fabricked](https://xca-attacks.github.io/fabricked/), [BREAKFAST](https://www.shwetashinde.org/publications/breakfast_oakland26.pdf) |
| SNP firmware-level breaks are closed by a TCB floor | conditional; patch values partly secondary; untested on Verda | [AMD-SB-3034](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3034.html), [AMD-SB-3020](https://www.amd.com/en/resources/product-security/bulletin/AMD-SB-3020.html) |
| GPU memory is safe from interposers | conditional; HBM argument covers B200; RTX PRO GDDR7 unstated | [DDRop](https://ddropattack.eu/ddrop.pdf), [R580](https://docs.nvidia.com/confidential-computing/release-notes/r580-trd1.md) |
| NVIDIA GPU attestation keys have not been publicly extracted | weak; absence of findings only | [Tinfoil FAQ](https://tinfoil.sh/security-and-privacy-faq), [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md) |
| Proof of Cloud protects against the datacenter operator | false as a general claim; it addresses relay to other machines | [arXiv 2510.12469](https://arxiv.org/abs/2510.12469), [POE draft](https://www.ietf.org/ietf-ftp/internet-drafts/draft-bzb-rats-intel-poe-endorsements-02.txt) |
| CC overhead is negligible for this PoC's small model | weak; 34-39% measured on defaults, recoverable by tuning; no RTX PRO SNP data | [arXiv 2608.26575](https://arxiv.org/html/2608.26575v2), [arXiv 2606.23969](https://arxiv.org/html/2606.23969v1) |
| A client can verify CPU evidence with no third-party contact | solid for SNP in the Tinfoil precedent; source reading, untested in practice | [tinfoil-go README](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md), [dcap-qvl](https://docs.rs/dcap-qvl/0.6.3) |
| GPU verification can run fully offline with NVIDIA tooling | unknown; redirect options exist but no offline mode is documented | [nvattest reference](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/latest/sdk-cli/command-reference.html), [config](https://docs.nvidia.com/attestation/nv-attestation-sdk-cpp/latest/sdk-rust/configuration.html) |
| "Provider cannot read content" as written | false against a provider with physical custody | [DDRop](https://ddropattack.eu/ddrop.pdf), [TEE.fail](https://tee.fail/files/paper.pdf), [Tinfoil FAQ](https://tinfoil.sh/security-and-privacy-faq) |
| Qualified claim: remote or software-only operator cannot read content given client policy | conditional; requires patched TCB, enforced policy, no admin path; untested in practice | [NVIDIA threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md), [Trail of Bits](https://github.com/trailofbits/publications/blob/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf) |

### Inferences
- The largest open risks for the PoC are Verda-specific and cheap to resolve: a few hours on one instance answers most items below.

### Gaps
- See "Proposed validation". None of these could be settled without running something or asking a vendor.

### Proposed validation
None of these was run. Each lists what to do, what result confirms or refutes the claim, and rough time.

1. **SNP report dump on Verda.** Inspect `reported_tcb` against the AMD-SB-3034, -3027 and -3020 floors, plus `smt_enabled`, `ciphertext_hiding_enabled`, `alias_check_complete`, policy bits and `key_info`. Confirms if TCB is at or above the floor and SMT is off; refutes the "patched" assumption otherwise. About 30 minutes.
2. **Recompute the measurement off-box.** Use `sev-snp-measure` with the published OVMF, kernel, initrd, cmdline and vCPU values, across boots and instances, and establish who controls `-append`. Confirms third-party verifiability if the digest matches and is stable. 1-2 hours.
3. **ACPI check.** Dump `/sys/firmware/acpi/tables` across boots and instances. On SNP, confirm that changes do not alter the measurement. On TDX, replay RTMR0 from the CCEL event log and check stability. About 1 hour.
4. **Boot an SSH-less dm-verity image** on Verda through the custom flow without dropbear, on both SKUs. Confirms that a no-admin production image is possible; refuted if Verda's flow requires the SSH initramfs. About half a day.
5. **nvattest local verification with egress blocked or logged,** using file evidence and a filesystem RIM store. Enumerates the endpoints contacted, shows OCSP failure behaviour, and shows whether a service key is needed. 1-2 hours.
6. **Source trace of SPDM-to-evidence binding,** through `spdmGetCertificates_HAL` and nvattest evidence handling. Confirms if the session identity is tied to appraised evidence, and refutes if not. 2-4 hours (source reading only).
7. **Small-model CC versus non-CC benchmark** on RTX PRO 6000 and B200, with and without `--no-async-scheduling`. Confirms whether the 34-39% default penalty applies and whether tuning removes it. About half a day.
8. **Ask NVIDIA or read the compatibility matrix** for GDDR7 encryption semantics (integrity, freshness), RTX PRO key rotation in R595, TEE-IO availability, and Product-Specific Terms. Days, dependent on a vendor response.
9. **Packet capture of a stapled-verifier client,** confirming that the only peer is the enclave. About 1 hour.
10. **Identify Verda's B200 TDX CPU** (Emerald versus Granite Rapids) and whether CI mode is on, by asking Verda or reading CPUID in the guest. Also ask about `page_swap_disabled` support. About 15 minutes.
