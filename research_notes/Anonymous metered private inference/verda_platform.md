# Verda platform: can its CC instances host the anonymous metered inference PoC?

Read on 2026-11-06. This track used public sources only, with no account, credentials, or API key, and nothing was run on Verda hardware.

How to read the evidence:

- **Cited Findings** are verified from a primary source. That means the source says it, not that it works. Everything about Verda behaviour below is untested in practice.
- **Inferences** are my reasoning from those findings.
- **Gaps** are unknown. Absence from public docs is recorded as unknown, not as no.

Versions and dates relied on:

| Source | Version or date |
|---|---|
| [Verda CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) | "last updated 27.10.2026" |
| [Public instance-types API](https://api.verda.com/v1/instance-types) | read 2026-11-06 |
| [Verda terms](https://verda.com/terms-and-conditions) | "Last updated October 30, 2026" |
| [SNP firmware release](https://github.com/verda-cloud/edk2/releases/tag/verda-amdsev-largebars-stable202605) | published 2026-08-04 |
| [TDX firmware release](https://github.com/verda-cloud/edk2/releases/tag/verda-inteltdx-stable202605) | published 2026-10-26 |
| Tools shown in Verda's docs | snpguest v0.10.0, [tdx-attest](https://docs.verda.com/assets/confidential_compute/tdx-attest.py) v0.6.0 (sha256 `01188c67...283b`) |
| [NVIDIA CC terms](https://www.nvidia.com/en-us/agreements/enterprise-software/product-specific-terms-for-confidential-computing/) | last modified Sept 4 2026 |
| [Tinfoil cvmimage](https://github.com/tinfoilsh/cvmimage) | main, last pushed 2026-11-05 |
| [Cure53 ExpressAI report](https://cure53.de/pentest-report_expressvpn-ai_2026.pdf) | March 2026 |
| Logged-in console snapshot from the sibling browser track, `verda_browser_notes.md` in this folder | 2026-11-06; not independently checked by me |

## What does Verda sell that matters here, and what is the "AI platform"?

### Takeaway

The CC products are VM instances, not containers or managed inference. `1RTXPRO6000.30V.CC` is the practical PoC target: one GPU, SEV-SNP, $2.78/h, and it was the cheapest GPU CC SKU in stock at the console snapshot. B200 CC has no 1x size, and the only B200 CC size in stock at that snapshot was 8x at $74.57/h, so the proposal's "RTX PRO 6000 CC or B200 CC" is not a close choice on cost.

### Cited Findings

CC SKUs, from the [public API](https://api.verda.com/v1/instance-types). The monthly column is my own arithmetic at 730 hours. The stock column comes from the sibling track's [console snapshot](verda_browser_notes.md).

| SKU | GPUs | vCPU / RAM / VRAM | On-demand | Spot | Per 730 h | Stock at snapshot |
|---|---|---|---|---|---|---|
| `1RTXPRO6000.30V.CC` | 1 | 30 / 90 GB / 96 GB | $2.780/h | $1.390/h | $2,029 (spot $1,015) | FIN-03 only |
| `2B200.60V.CC.AMD`, `.INTEL` | 2 | 60 / 340 GB / 360 GB | $18.64/h | $9.321/h | $13,607 (spot $6,804) | Size shown, no available row |
| `4B200.120V.CC.AMD`, `.INTEL` | 4 | see API | $37.28/h | $18.64/h | $27,214 | No available location |
| `8B200.240V.CC.AMD` | 8 | 240 / 1360 GB / 1440 GB | $74.57/h | $37.28/h | $54,436 (spot $27,214) | FIN-03 only |
| `8B200.240V.CC.INTEL` | 8 | 240 / 1360 GB / 1440 GB | $74.57/h | $37.28/h | $54,436 | Location "None" |
| B300 CC 4x, 8x | 4, 8 | see API | $47.70/h, $95.39/h | see API | not computed | "Notify me" on-demand |
| `CPU.4V.16G.CC` and larger, up to `CPU.360V.1440G.CC` | 0 | 4 / 16 GB at the low end | $0.04896/h | $0.01958/h | $35.74 (spot $14.29) | Not captured |

- RTX PRO 6000 CC exists only as 1x, and the API describes it as "Dedicated Hardware Instance" ([API](https://api.verda.com/v1/instance-types)). Its images are `24.04` and `26.04` with `cuda13.2.cc` or `cuda13.4.cc`.
- **Marketing conflict on B200.** [Pricing](https://verda.com/pricing) shows "1x B200 CC SXM6 180GB" at $9.320/h, and the [CC page](https://verda.com/confidential-computing) lists B200 as "8x GPUs" in a "(1x GPU)" column. The [API](https://api.verda.com/v1/instance-types), [GPU instances page](https://verda.com/gpu-instances), and console show 2x, 4x, 8x only. The $9.32 figure is a per-GPU price and cannot be rented alone.
- **Timeline.** "[now available by request](https://docs.verda.com/welcome-to-verda/release-notes/)" in March 2026, then "generally available" in August 2026. Older cached text saying early preview is stale, and the old docs URL ending in `confidential-compute/` now returns 404.
- **Regions.** FIN-01, FIN-02, and FIN-03, all Helsinki ([locations](https://docs.verda.com/welcome-to-verda/locations-and-sustainability/)). The console snapshot showed CC capacity only in [FIN-03](verda_browser_notes.md).
- **Billing.** Prepaid, in 10-minute increments. At zero balance, instances are discontinued and volumes deleted, restorable for up to 96 hours ([billing](https://docs.verda.com/welcome-to-verda/pricing-and-billing/)). NVMe is $0.20/GiB/month ([pricing](https://verda.com/pricing)).
- **Spot** can be evicted without warning ([services overview](https://docs.verda.com/resources/services-overview/)).
- **Quotas** apply to GPU, Spot GPU, and vCPU. Increase requests ask what you are building and are reviewed in about one business day ([quotas](https://docs.verda.com/welcome-to-verda/quotas/)).
- **Platform.** It includes GPU and CPU instances, instant clusters, serverless containers including batch jobs, block, shared, and object storage, an OCI registry, Private Networks (beta), managed Kubernetes, and contract-only bare metal ([services overview](https://docs.verda.com/resources/services-overview/), [docs](https://docs.verda.com), [GPU instances](https://verda.com/gpu-instances)). Verda Inference is in preview, is OpenAI-compatible, pays per token, and starts with one model ([release notes](https://docs.verda.com/welcome-to-verda/release-notes/)). Nothing I read describes it as confidential.
- **CC is VM-only in the public API.** The public `/v1/container-types` and `/v1/cluster-types` listings contain no CC entries ([API docs](https://api.verda.com/v1/docs)). The [console's container form](verda_browser_notes.md) has no Confidential filter either.
- **Multi-node CC** is "on our roadmap" ([CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/)), though a customer story mentions "[early support for distributed secure GPU clusters](https://verda.com/blog/expressvpn-success-story)".

### Inferences

- "Deploy to Verda's AI platform" should be read as "rent a CC VM and run everything inside it". Serverless containers and Verda Inference cannot be the confidential backend on current public evidence.
- For a one-small-model PoC, RTX PRO 6000 CC is the right first target. 96 GB VRAM is ample, it avoids multi-GPU attestation questions, and on-demand costs about 1/27 of the only B200 CC size in stock.
- CC capacity at a single site is an availability risk, and spot eviction plus the no-in-guest-reboot rule (next section) means restart automation is needed from day one. A zero balance is also destructive, so the prepaid balance needs monitoring.
- Cross-track: CPU CC instances could host a GPU-less component cheaply if another track decides one should be in a TEE. That is an option, not a recommendation.

### Gaps

- Live stock over time. [`/v1/instance-availability` and `/v1/images`](https://api.verda.com/v1/openapi.json) returned 401 without a token, so per-region availability here rests on one console snapshot.
- Default GPU quota for a new account.
- Whether a "Rome/Milan" label on CC CPU SKUs is accurate. The [API](https://api.verda.com/v1/instance-types) uses it, and my background knowledge (not checked here) is that Rome lacks SEV-SNP.

## What exactly does Verda's confidential computing provide?

### Takeaway

RTX PRO 6000 CC is SEV-SNP only. B200 CC is documented with both SEV-SNP and TDX, but only the AMD variant had capacity at the snapshot. CPU evidence and GPU evidence both reach the tenant, separately. All documented attestation happens inside the guest, there is no attestation API, and combining the two pieces of evidence for a remote verifier is left to you.

### Cited Findings

All from the [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) unless linked otherwise.

| Hardware | Size | SEV-SNP | TDX |
|---|---|---|---|
| RTX PRO 6000 | Single GPU | Available | Not available |
| B200 | Multi GPU | Available | Available |
| B300 | Multi GPU | Available | Available |
| CPU-only | Up to 360x | Available | Not available |

- **SNP evidence.** The sample uses `snpguest` with the `turin` chain, `family=26 model=2 stepping=1`, and 30 vCPUs. The report is signed by a per-chip VCEK.
- **TDX evidence.** [`tdx-attest`](https://docs.verda.com/assets/confidential_compute/tdx-attest.py) has `self-check`, `quote`, `verify`, `verify-ita`, and `ita-nonce`, and it checks TD debug, nonce binding, TCB verdict, and reference registers. The docs state that self-check "convinces the guest, not a remote party", and they tell you to let the remote party choose the nonce. Off-box verification works through Intel Trust Authority.
- **GPU evidence.** `nvidia-smi conf-compute -q` shows CC State ON, Multi-GPU Mode None, and CPU CC Capabilities AMD SEV-SNP. `nvattest attest --device gpu --verifier remote` runs inside the CVM. The sample shows GB20X, VBIOS 98.02.8D.00.01, and driver 580.126.09.
- **Ready state is manual.** You run `nvidia-smi conf-compute -srs 1` yourself, because "Current nvattest versions only attest the GPU and no longer set the ready state". A [Verda blog post](https://verda.com/blog/confidential-computing-b300-b200) implies attestation sets it automatically. The docs are more specific, so I rely on them.
- **Reboot rule.** A CVM "cannot reboot from inside the guest". Restart is Shutdown then Start, and "Each boot is freshly measured". The [CLI](https://docs.verda.com/cli/instances/) has `start`, `shutdown`, `force_shutdown`, and `delete`.
- **Multi-GPU.** The [blog](https://verda.com/blog/confidential-computing-b300-b200) says GPU-to-GPU traffic is encrypted and reports throughput overhead of 1.1% to 5.5% on 8x B300, with driver 595.58.03. The docs' attestation examples are single-GPU.
- **Conflicts.**
  - The [marketing page](https://verda.com/confidential-computing) names only SEV-SNP and claims a "Signed virtual TPM, bootloader, and kernel". The docs describe direct kernel boot with no bootloader in the chain, and they do not document a vTPM.
  - A [third-party PR merged Oct 19 2026](https://github.com/Lore-Hex/token-exchange-neocloud-in-a-box/pull/23) says Verda "uses AMD SEV-SNP" and does not satisfy its TDX verifier. It offers no hardware evidence and predates the [TDX firmware release](https://github.com/verda-cloud/edk2/releases/tag/verda-inteltdx-stable202605). The docs, API `.INTEL` SKUs, and that release all show TDX exists, at least on paper.
  - The [console snapshot](verda_browser_notes.md) shows the 8x Intel SKU with no location, so TDX was not rentable that day.

### Inferences

- Treat the PoC's attestation path as SNP-first. TDX adds a second verifier and is not needed on RTX PRO 6000.
- Measured code should run GPU attestation, then set the ready state, then publish the evidence. Verda's docs do not show that joint flow, and the [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) say the two are attested independently.
- Cross-track: [TEE.fail](https://tee.fail) reports NVIDIA evidence is "not bound" to a CVM and can be borrowed, so binding CPU and GPU evidence belongs on the TEE track's list.

### Gaps

- Whether multi-GPU Protected PCIe mode and NVSwitch topology attestation work on 2x and 4x B200, and how they are verified.
- Ciphertext hiding, SMT state, and whether the VCEK chain comes in the extended report or must be fetched from AMD.
- Xeon generation and TDX module version.
- Whether the vTPM claim is true in any form.

## Can a tenant control the launch measurement, can a third party verify the workload, and could Tinfoil's approach run?

### Takeaway

**This is the most favourable finding and it still needs hardware confirmation.** Verda documents that the tenant's own kernel and initrd go into the launch measurement, and the firmware is a public, pinned build. On that evidence, a third party can in principle compute the expected measurement offline.

Three limits matter:

1. The stock `.cc` image proves almost nothing about the workload, because the root disk is not measured.
2. The kernel command line appears to be host-derived, so the dm-verity root hash must live in the initrd rather than on the cmdline.
3. Verda's sample SNP verifier is not good enough for third-party use.

Stock Tinfoil images will not match on Verda. The Tinfoil approach looks portable, but only as your own fork, verified with your own published measurements.

### Cited Findings

All from the [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) unless linked otherwise.

**Mechanism.** Verda "launches each CVM with QEMU direct kernel boot" with "SEV-SNP kernel hashes enabled", which folds firmware, kernel, initramfs, and kernel command line into the launch measurement.

| Boot input | In the launch measurement? | Who controls it | Evidence |
|---|---|---|---|
| OVMF firmware | Yes | Verda. "you can't bring your own", with one public build pinned | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Kernel, initrd | Yes | Tenant. "come from your own OS volume, not from Verda" | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Kernel cmdline | Yes | Appears host-derived. Tenant influence is unknown | [encrypted_env_sample](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt) |
| vCPU count and signature | Input to the expected value | Verda and the SKU | [encrypted_env_sample](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt) |
| SNP POLICY | No, a separate signed field per secondary sources | Set at launch, value undocumented | [AWS](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/snp-attestation.html), [Contrast](https://docs.edgeless.systems/contrast/architecture/snp) |
| Root disk, SSH keys, startup scripts | No | Tenant | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) (measured-input list excludes them) |

**Firmware.**

- SNP: [release](https://github.com/verda-cloud/edk2/releases/tag/verda-amdsev-largebars-stable202605), commit `0c0bd94a4699bde01f86c6b9c39f37545af68a4a`, SHA-256 `fea1855b6c983324246e6d3fb6642be200c0a38fd17b94884a45130ffb0b3a81`, labelled reproducible, with a copy at `/boot/OVMF.amdsev.fd` in the guest.
- TDX: [release](https://github.com/verda-cloud/edk2/releases/tag/verda-inteltdx-stable202605), commit `d669e68c69806d53fee34ae861ffda4dc61a4619`, MRTD `eff2f480...6145c`. It "changes only with a new firmware release". Kernel and initrd go into RTMR1 and RTMR2.

**Custom OS.** "Booting a Custom OS" boots a RAM-only dropbear initramfs once, streams an image to `/dev/vda`, then does Shutdown and Start. "Booting an Encrypted Custom OS" uses LUKS2 on Ubuntu 26.04 and unlocks after attestation on every boot. The docs do not mention dm-verity or UKI (0 matches in [llms-full.txt](https://docs.verda.com/llms-full.txt)).

**Evidence that the cmdline is host-derived.**

- [encrypted_env_sample](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt) shows `console=ttyS0 root=UUID=... ro net.ifnames=0 biosdevname=0 fsck.mode=auto fsck.repair=yes pci=realloc,nocrs`, and says "Verda re-syncs root=UUID" to the partition holding `sbin/init` or `lib/systemd`.
- [encrypted_00_build_image.sh](https://docs.verda.com/assets/confidential_compute/encrypted_00_build_image.sh) creates a marker directory with the comment `makes Verda pick /boot as "root"`, and notes "the extractor looks for /boot/vmlinuz".

**Evidence pointing the other way.** [00_build_image.sh](https://docs.verda.com/assets/confidential_compute/00_build_image.sh) writes `net.ifnames=0 biosdevname=0 fsck.mode=auto fsck.repair=yes` into `GRUB_CMDLINE_LINUX` and runs `update-grub`. Those same values appear in the booted cmdline, even though the docs describe no GRUB step. The [OpenAPI](https://api.verda.com/v1/openapi.json) has 0 matches for cmdline, kernel, attest, firmware, or ovmf. So no API sets it, and an on-disk source is possible but unproven.

**A decoy-root pattern already exists.** [encrypted_attested_initramfs.sh](https://docs.verda.com/assets/confidential_compute/encrypted_attested_initramfs.sh) generates an ephemeral host key in RAM, binds its SHA-512 into the SNP report, serves the report on port 9000, and in `local-bottom` swaps "the decoy /boot root" for the real one. So Verda's own sample has a tenant initramfs mounting something other than `root=`.

**Third-party binding.** Verda demonstrates a remote verifier with `REPORT_DATA` bound to a per-boot SSH key ([encrypted_03_attest_and_unlock.sh](https://docs.verda.com/assets/confidential_compute/encrypted_03_attest_and_unlock.sh)). The docs also say to "record the Expected measurement from a trusted build".

**Weaknesses in the sample SNP verifiers.**

- [encrypted_03](https://docs.verda.com/assets/confidential_compute/encrypted_03_attest_and_unlock.sh) and [measure_boot.sh](https://docs.verda.com/assets/confidential_compute/measure_boot.sh) have 0 matches for policy, debug, tcb, or migrate.
- encrypted_03 takes the cmdline and vCPU inputs from the guest over SSH, so a host-altered cmdline would also change the "expected" value it computes.
- measure_boot.sh runs inside the CVM and reads on-disk files and `/proc/cmdline`.
- By contrast, [Tinfoil's policy file](https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/platform/policies.json) requires debug false, migrate_ma false, and a minimum TCB. Verda's TDX helper does check debug.

**vCPU signature conflict.** The docs sample shows `family=26 model=2 stepping=1`, while [encrypted_env_sample](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt) says QEMU presents family 26, model 0, stepping 0, "NOT the host silicon's model 2 / stepping 1".

**Golden volumes.** `POST /v1/instances` accepts an OS image type or UUID, "Hidden images", or a customized OS volume ID ([OpenAPI](https://api.verda.com/v1/openapi.json)). Whether CC SKUs accept that is untested, since `supported_os` lists only `.cc` images ([API](https://api.verda.com/v1/instance-types)).

**Change risk.** [Terms 3.2](https://verda.com/terms-and-conditions) commits Verda only to "commercially reasonable endeavours to notify" of material service changes.

**Tinfoil.**

- [docs/build.md](https://github.com/tinfoilsh/cvmimage/blob/main/docs/build.md): the disk has only "a fixed 2 GiB ext4 root partition" and a dm-verity hash partition, and "QEMU supplies the kernel, initrd, and firmware directly".
- Its [initrd](https://github.com/tinfoilsh/cvmimage/blob/main/tinfoil/cmd/initrd/main.go) reads `roothash` from the cmdline and errors if it is missing. It also parses [`tinfoil-config-hash` and `tinfoil-debug`](https://github.com/tinfoilsh/cvmimage/blob/main/tinfoil/internal/kernelcmdline/cmdline.go).
- It pins a [different OVMF](https://github.com/tinfoilsh/cvmimage/blob/main/platform/toolchain.lock.json) (Ubuntu `ovmf_2025.02-3ubuntu2`) and defines its own [QEMU shapes](https://github.com/tinfoilsh/cvmimage/blob/main/platform/measure.py).
- The licence is AGPL-3.0.
- [Tinfoil Containers](https://tinfoil.sh/blog/2026-03-10-tinfoil-containers) deploy "on Tinfoil". I found no statement about customer clouds and no Tinfoil-Verda relationship.

**Precedent.**

- Verda's [ExpressAI story](https://verda.com/blog/expressvpn-success-story) says "Every logged-in user has access to their own Attestation Report" and describes co-engineering "down to network and firewall configuration".
- [Cure53](https://cure53.de/pentest-report_expressvpn-ai_2026.pdf) describes a "custom QEMU-based hypervisor embedded in a cloud compute tenant" and does not name Verda.
- Finding EXP-23-007 (High, CVSS 8.7) was a frontend that set `checks.measurementTrusted = true` without comparing anything and never validated the VCEK signature. It was fixed during testing.

### Inferences

- **The stock `.cc` image attests the boot chain, not the service.** A third party learns "this firmware, kernel, initrd, and cmdline on genuine CC hardware". The root filesystem, SSH keys, startup scripts, and inference server are unmeasured, and the tenant still has root. That does not meet the proposal's "client attests the enclave" claim.
- **A verifiable design appears feasible.**
  - Put the verity root hash in the measured initrd, or possibly in a kernel-builtin cmdline. The built-in cmdline idea is my own and unsourced.
  - Use the decoy `/boot` plus marker so Verda's extractor finds the kernel.
  - Fix the partition UUIDs so `root=UUID` stays deterministic.
  - Ship no SSH or cloud-init, and bake the tools in.
  - Bind a TLS or HPKE key into `REPORT_DATA`, following the SSH-key pattern.
- **The verifier must be your own.** It should use a pinned measurement computed offline, a POLICY check for debug off and migrate_ma off, a TCB floor, and a VCEK chain check. It must never take inputs from the guest. The Cure53 finding shows how easily client verifiers go wrong, so test with a deliberately wrong pin.
- **Measurements will rotate outside your control** if Verda changes firmware, the cmdline template, or the vCPU model. The client's allowed-measurement list therefore needs an update channel, which is a design input for the architecture track.
- **Stock Tinfoil cannot match.** Its firmware can't be installed on Verda, there is no documented tenant cmdline for `roothash`, its disk layout lacks the `/boot` Verda's extractor expects, and its shapes and chip mappings differ. So Tinfoil's published measurements and Sigstore identity do not carry over.
- **A fork looks plausible.** Move `roothash` and the config hash into the initrd, add a small ext4 `/boot` with symlinks and marker, rebuild reproducibly, and compute measurements against Verda's OVMF. AGPL obligations apply, and you would publish the identity yourself.
- **Tinfoil is an alternative backend, not a layer on Verda.** Tinfoil hosts on its own bare metal, so "Verda backend" and "Tinfoil backend" are likely two separate backends behind one interface.
- **ExpressAI is weak precedent.** The Cure53 language suggests a custom tenant hypervisor rather than today's self-service flow, though the report does not name the provider.

### Gaps

- The exact source of `-append`, and whether a tenant can add arguments.
- The extractor algorithm: filesystems, partition scan, marker rule, symlink handling, kernel format and size limits.
- The actual SNP POLICY value on each SKU.
- Stability of the measurement inputs across hosts, regions, and time, and any notice period for changes.
- Whether a CVM with no SSH or cloud-init stays healthy from Verda's side.
- Whether CC SKUs boot from a pre-flashed volume ID. The sibling track could not confirm what the [console's "Other" OS tab](verda_browser_notes.md) offers on CC.
- How SSH keys are handled on a custom image. They are a [required deploy field](verda_browser_notes.md), so a verifiable image must demonstrably ignore them after flashing.
- I did not read AMD's ABI spec 56860, so the claim that POLICY sits outside the digest needs confirming there.

## Do networking and logging allow an onion service?

### Takeaway

Nothing public blocks it. Public-IP instances have unfiltered inbound and working outbound access, and the custom OS flow does not appear to require a Verda guest agent. Tor reachability and egress filtering are unconfirmed, and Verda logs usage data for up to about 12 months.

### Cited Findings

- Public IPv4 is 1:1 NAT "with both inbound and outbound internet access", and "Verda does not filter inbound traffic, so run a guest firewall". A private-only instance "has no internet access in either direction" unless you route through a NAT instance ([internet access, beta](https://docs.verda.com/private-networks/internet-access/)).
- Private Networks are beta and FIN-03 only. Legacy instances get a public IP on their NIC ([private networks](https://docs.verda.com/private-networks/)). The [console](verda_browser_notes.md) offers Public + Private, Private only, and Public only.
- The customer manages OS, network, and firewall ([shared responsibility](https://docs.verda.com/resources/shared-responsibility-model/)), and Verda recommends ufw and fail2ban ([securing your instance](https://docs.verda.com/cpu-and-gpu-instances/securing-your-instance/)). The [OpenAPI](https://api.verda.com/v1/openapi.json) has no firewall matches.
- [00_build_image.sh](https://docs.verda.com/assets/confidential_compute/00_build_image.sh) starts from a stock Ubuntu cloud image, disables cloud-init, and uses DHCP on eth0. The boot cmdline includes `console=ttyS0`.
- [Privacy policy](https://verda.com/privacy-policy): "Usage Data (infrastructure logs, job activity)" kept "Typically up to 12 months", and systems are "monitored for suspicious activity and intrusions". [Terms 6.3](https://verda.com/terms-and-conditions) lists "automated security monitoring, threat and abuse detection".
- Scheduled maintenance for FIN03 was [shown in the console](verda_browser_notes.md) for 2026-11-08, marked "No service interruption expected".

### Inferences

- An outbound-only onion service should work on a public-IP instance with all inbound blocked by the guest firewall. Private-only plus a NAT instance could work too, but adds a component and depends on the beta network supporting CC.
- No mandatory agent appears necessary, based on the custom image flow. That is document-level only.
- Serial console output is host-visible, so the image must not log anything sensitive to `ttyS0`.
- Verda's logs and monitoring are outside the enclave, so they can see timing and traffic volume to Tor relays at most. Anonymity relies on Tor and the enclave rather than on Verda's retention policy. Traffic-volume correlation belongs on the Tor track.

### Gaps

- Egress filtering, port blocks, or Tor-specific flags. None documented and none ruled out.
- Scope of network logging (flow records or deeper) and its retention.
- Serial console capture and retention.
- Egress pricing.
- Whether Private Networks work with CC SKUs. The console form suggests yes, but deployment is untested.

## What do the terms allow?

### Takeaway

The terms do not mention Tor, onion services, or anonymizers. That is not permission. The real legal questions are the platform-use clause, the "competes with" language now that Verda sells inference, and suspension for liability risk. Get written confirmation before treating this as more than a private PoC. NVIDIA's terms add obligations of their own.

### Cited Findings

All from the [Verda terms](https://verda.com/terms-and-conditions) unless linked otherwise.

- **Not addressed.** A text search found no Tor, onion, VPN, anonymizer, exit node, or relay. "Proxy" appears only inside the definition of Control.
- **Resale.** Customers may not "resell, rent, license, or otherwise make the Service available to any third party", except as permitted.
- **Platform use (Schedule 1 §2).** You may offer your own services to third parties if the service "does not consist entirely or predominantly of the Service".
- **Competition.** A service that "replicates, competes with or is substantially similar to the Service" is restricted. Verda now sells [per-token inference](https://docs.verda.com/welcome-to-verda/release-notes/).
- **Content.** AI may not be used "for illegal purposes" or where it "may contribute to social harm", and material that "facilitates illegal activity" is prohibited. I found no explicit requirement to moderate.
- **Security testing** needs "prior written consent" for "security, vulnerability, penetration, or similar testing of the Service". This may touch attestation tamper tests.
- **KYC and access (6.3).** Verda may run "know-your-customer checks, screening requirements", and "shall not require access to Customer Data" beyond what is strictly necessary.
- **Suspension (14.4)** if use "poses a security risk" or "may subject the Supplier or any third party to any liability".
- **Disclosure (11.3)** "to any regulator, law enforcement agency".
- **Jurisdiction.** Verda Cloud Oy, Finnish law, arbitration in Helsinki under the Finland Chamber of Commerce.
- **AI terms.** "AI Services" includes "infrastructure for running AI Models ... in any deployment mode", and Schedule 5 carries its own no-resale clause (3.1.16). Schedule 6 §4.2 contains an unfilled "[URL]" placeholder.
- **CC schedule (Schedule 4).** Comply with NVIDIA terms, "maintain accurate and complete records related to usage", "refresh cached output of NVIDIA Confidential Computing at least once in any twenty-four (24) hour period", and do not prohibit users accessing NVIDIA CC directly. "NVIDIA will collect IP addresses and information about GPUs".
- **[NVIDIA terms](https://www.nvidia.com/en-us/agreements/enterprise-software/product-specific-terms-for-confidential-computing/).** Development use is free, but "A commercial Enterprise Product license must be obtained before offering or distributing a Customer Product" (1.2.3). Usage records are kept "at least three (3) years" with audit rights (16.7). NVIDIA "may also collect IP address" (11.1.2).
- **Compliance.** [Verda's homepage](https://verda.com) claims SOC 2 Type II, ISO 27001/27017/27018/27701, and GDPR compliance. [trust.verda.com](https://trust.verda.com) rendered no readable text for me, so the reports are unread and these remain claims.

### Inferences

- A private PoC for your own use is lower risk than a public service. The proposal's end state is an anonymous metered public service, where "predominantly", "competes with", and 14.4 all become live.
- An operator can be KYC'd and known to Verda while the end users are not. That matches the proposal's model, but it makes you the accountable party for abuse you are deliberately unable to see. This belongs on the legal track.
- Do not run a Tor exit or public relay. The onion service alone is enough, and it avoids an obvious source of abuse complaints.
- If clients call NVIDIA's attestation service directly, their IP is exposed to NVIDIA. So either the enclave fetches and serves the evidence, refreshed at least every 24 hours, or clients reach NVIDIA over Tor. This is a cross-track input for the client design.
- NVIDIA's 3-year usage records need checking against any no-log claim. They probably concern licence usage rather than prompts, but that needs legal reading.
- The no-moderation posture is not expressly banned, but "may contribute to social harm" is broad enough that Verda could rely on it.

### Gaps

- Verda's actual position on an onion-only anonymous service.
- Whether Schedule 5 applies to a CC VM you run yourself.
- What triggers 14.4 in practice.
- How Verda handles legal process and customer notice.
- What NVIDIA's "usage records" contain.
- Whether a commercial NVIDIA licence is needed for this product, and who would hold it.

## Can it be automated?

### Takeaway

Yes for lifecycle. There is a REST API, SDKs, a CLI, and Terraform. No API exposes attestation, so it must be served from inside your guest, which is what you want anyway.

### Cited Findings

- [REST API](https://api.verda.com/v1/docs), [Python SDK](https://github.com/verda-cloud/sdk-python), [Go SDK](https://github.com/verda-cloud/verdacloud-sdk-go), [CLI](https://docs.verda.com/cli/), [Terraform provider](https://docs.verda.com/infrastructure-as-code/terraform/compute-instances/) `verda-cloud/verda`.
- Startup scripts run on first deploy ([set up a GPU instance](https://docs.verda.com/cpu-and-gpu-instances/set-up-a-gpu-instance/)). They are not part of the documented measured inputs.
- Block volumes serve as OS drives and can be cloned, including across datacenters. Shared filesystem is NFS. Object storage is beta. There is an OCI registry and audit logs ([services overview](https://docs.verda.com/resources/services-overview/), [API docs](https://api.verda.com/v1/docs)).
- The API changelog added image category `confidentialComputing` on 2026-09-15 ([API docs](https://api.verda.com/v1/docs)).
- No public endpoint uploads a custom image. `custom_image` appears only in an audit-log enum ([OpenAPI](https://api.verda.com/v1/openapi.json)).

### Inferences

- A deployment pipeline can be: deploy a `.cc` image, flash through the documented initramfs flow, Shutdown and Start, then verify remotely. A golden OS volume would shorten that if CC SKUs accept it.
- Model weights on an unmeasured volume need their own integrity check, such as a hash in the measured image or dm-verity. NFS and object storage are outside the trust boundary. This is a design input.
- Terraform state and API tokens are operator secrets, but they do not change what clients verify, because clients trust the measurement rather than the operator.

### Gaps

- Whether the Terraform provider and SDKs expose every CC option. I did not exercise them.
- Whether the flash flow can be fully unattended.

## What should be asked of Verda support, in order?

### Takeaway

Questions 1 to 4 could change the plan. The rest refine it. Some can be answered faster by running the validations below, so run V1 and V2 before writing if an account is ready.

### Cited Findings

No external claims here. Each question comes from a gap in the sections above.

1. **Kernel cmdline.** What is the exact source of `-append`? Is it a fixed template or tenant-influenced (grub.cfg, or a `/boot` file)? Can arguments be added? Is it stable across hosts and over time? What is the extractor's exact algorithm: filesystems, partition scan, marker rule, symlinks, kernel format and size?
2. **Custom image support.** Are a custom kernel, a custom initramfs, a dm-verity root, and no SSH or cloud-init supported on CC SKUs? Does anything depend on guest liveness?
3. **Launch stability for `1RTXPRO6000.30V.CC`.** What is the SNP POLICY value (debug off, migrate_ma off), the vCPU count and signature, and the QEMU version, across hosts and regions? What notice is given for firmware or template changes, and how long is old OVMF retained?
4. **Terms, in writing.** Is an onion-service-only, anonymous, unmoderatable, metered third-party inference service permitted, considering "competes with", §2 platform use, and the "AI Services" definition? It would not run a Tor exit or relay. What triggers 14.4?
5. **Network.** Are there egress filters or port blocks? Is Tor flagged or blocked? What flow logging and DPI exist, and what is the retention? What is egress pricing? Is serial console output captured?
6. **Availability and quotas.** What is RTX PRO CC on-demand versus spot stock per location, the default GPU quota, and private-network support on CC?
7. **Golden volume.** Can a CC SKU boot from a pre-flashed or cloned OS volume ID? Are non-`.cc` images, raw upload, or hidden and custom images possible?
8. **Multi-GPU on 2x and 4x B200.** Protected PCIe mode, NVSwitch ownership and sharing, topology attestation, and driver, VBIOS, and RIM pins?
9. **Hardware detail.** VCEK and certs in the extended report, ciphertext hiding, SMT, TCB update cadence, chip IDs, CPU generations per SKU (the "Rome/Milan" label), and Xeon generation and TDX module?
10. **vTPM.** How does the marketing claim square with the docs?
11. **Physical security.** What is Verda's stance on TEE.fail, and is colocation involved?
12. **NVIDIA terms.** Licence pass-through, scope of usage records, meaning of the 24-hour refresh, and whether a commercial licence is needed for a product.
13. **Testing consent.** Is consent available for attestation tamper tests on your own instances?
14. **Operations.** How is legal process handled, what KYC applies, and what notice is given for maintenance or forced shutdown, given no in-guest reboot?

### Inferences

Support answers are weaker evidence than a test. Treat answers to 1 to 3 as hypotheses until V1 to V4 confirm them, and get answer 4 in writing.

### Gaps

Verda's support responsiveness and willingness to answer these are unknown. ExpressAI's [co-engineering](https://verda.com/blog/expressvpn-success-story) suggests depth is available to some customers.

## Does Verda fall short enough to need an alternative?

### Takeaway

No, not on current evidence, so I did not compare alternatives in depth. If V2 to V4 fail, the first fallback to ask about is Verda bare metal running your own QEMU and OVMF.

### Cited Findings

- Bare metal is contract-only or "available on request" ([services overview](https://docs.verda.com/resources/services-overview/), [GPU instances](https://verda.com/gpu-instances)). The [Cure53 report](https://cure53.de/pentest-report_expressvpn-ai_2026.pdf) shows a custom-hypervisor pattern exists somewhere, with the provider unnamed.
- An unevaluated lead: IONOS has a [hardened CVM kernel how-to](https://docs.ionos.com/cloud/compute-services/compute-engine/confidential-computing-vm/how-tos/hardened-cvm-kernel-howto). I have not checked its GPU support.
- [TEE.fail](https://tee.fail) reports a DDR5 interposer attack costing under $1,000, forged TDX quotes via Intel PCK extraction, and ECDSA key extraction from an SNP VM, with vendors treating physical attacks as out of scope. That contradicts the scope of [Verda's CC page FAQ](https://verda.com/confidential-computing), which says "Cold-boot attacks, RAM extraction, and bus probing recover ciphertext, not plaintext".

### Inferences

- Cross-track: physical access to the datacenter remains in the trust model at any provider, so this is not a reason to leave Verda specifically. On the reported results, forged-quote risk looks worse on TDX than SNP, which weakly favours the SNP path. The TEE track should confirm.
- Bare metal would restore full launch control, but you would lose Verda's pinned public firmware story and would need to publish your own.

### Gaps

- Whether Verda bare metal supports CC at all.
- Prices and terms for it.
- Any other provider's tenant-measurement support. Not researched.

## Proposed validation

None of this was run. It needs a Verda account, so the user would run it. The sibling track's notes show a project balance of [$100](verda_browser_notes.md), which covers V1, V2, V4, V5, V7, V8, and V10 on the RTX SKU at spot. Times are rough. Consider [the AUP's testing clause](https://verda.com/terms-and-conditions) before V5's tamper checks. Normal attestation use on your own instance is probably fine, but it is untested as a legal reading.

| ID | What to run | Confirms if | Refutes if | Rough time and cost |
|---|---|---|---|---|
| V1 | Deploy `1RTXPRO6000.30V.CC` at spot. Run `measure_boot.sh`. Record `/proc/cmdline`, POLICY from `snpguest display report`, and the measurement over two boots and, if possible, two hosts | Measurement matches and is stable. Debug off and migrate_ma off | Measurement varies without a known cause, or debug or migrate_ma is on | 1 h, about $5 |
| V2 | Change `/etc/default/grub` and `update-grub`, or a `/boot` file. Shutdown and Start. Diff `/proc/cmdline` | Cmdline changes, so the tenant has control | No change, so the cmdline is host-only and the root hash must go in the initrd | 30 min |
| V3 | Build a custom kernel and initrd with a verity root and decoy `/boot`. Compute the expected value offline with `sev-snp-measure` and Verda's OVMF | Offline value equals the reported measurement | Mismatch that can't be explained, or the extractor refuses the layout | 1 to 2 days |
| V4 | Boot that image with no SSH or cloud-init. Exercise API Shutdown and Start | Instance stays running and can be controlled | Instance is marked unhealthy or stopped | Part of V3 |
| V5 | Write an off-box verifier with pinned measurement, POLICY and TCB checks, and a TLS or HPKE key in `REPORT_DATA`. Try a wrong pin and a wrong nonce | Right inputs pass and wrong inputs fail | A wrong input passes | 0.5 day |
| V6 | Run `nvattest` and the built-in driver in the custom image, set the ready state, and serve a small model on vLLM | Inference works with no network installs after boot | Attestation or the driver needs unmeasured state | 1 day |
| V7 | Run a v3 onion service in the image. Connect from an outside client and stream a response | Stable reachability and acceptable streaming latency | Tor blocked, or an abuse notice arrives | 2 to 4 h |
| V8 | Deploy a CC SKU from a flashed or cloned OS volume ID | Boots with the same measurement | API rejects it or the measurement differs | 1 h |
| V9 | Only if TDX is needed: compare RTMR0 across shapes and hosts. It needs an Intel SKU with capacity, and none was available at the snapshot | RTMR0 is predictable | RTMR0 varies per host | 1 day, $100+ |
| V10 | Trigger a spot eviction or force shutdown and run restart automation | Service and attestation return with the same measurement | Measurement changes or manual steps are needed | 1 h |

## Claim table

"Untested" means supported only by documentation or source reading, not tested in practice.

| Claim | Status | Evidence |
|---|---|---|
| RTX PRO 6000 CC is rentable self-service at $2.78/h | Solid for listing and console; deploy untested | [API](https://api.verda.com/v1/instance-types), [console notes](verda_browser_notes.md) |
| B200 CC exists with minimum 2x, in AMD and Intel variants | Solid for listing; untested. Only 8x AMD had capacity at the snapshot | [API](https://api.verda.com/v1/instance-types), [GPU instances](https://verda.com/gpu-instances), [console notes](verda_browser_notes.md) |
| A 1x B200 CC can be rented | False | [API](https://api.verda.com/v1/instance-types), [pricing conflict](https://verda.com/pricing) |
| CC is generally available | Solid (since August 2026) | [Release notes](https://docs.verda.com/welcome-to-verda/release-notes/) |
| RTX PRO 6000 CC uses SEV-SNP only | Solid; untested | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| B200 CC offers both SNP and TDX | Conditional; documented, untested, third-party PR disagrees, Intel had no capacity | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/), [TDX firmware](https://github.com/verda-cloud/edk2/releases/tag/verda-inteltdx-stable202605), [PR](https://github.com/Lore-Hex/token-exchange-neocloud-in-a-box/pull/23) |
| Tenant can get both CPU and GPU evidence | Solid; untested | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Tenant kernel and initrd are in the launch measurement | Solid in docs; untested | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/), [helper scripts](https://docs.verda.com/assets/confidential_compute/measure_boot.sh) |
| Tenant can choose the firmware | False | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Firmware is public and reproducible | Conditional; claimed by Verda, rebuild untested | [SNP release](https://github.com/verda-cloud/edk2/releases/tag/verda-amdsev-largebars-stable202605) |
| Tenant can control the kernel cmdline | Unknown; evidence conflicts | [env sample](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt), [00_build_image.sh](https://docs.verda.com/assets/confidential_compute/00_build_image.sh) |
| Root disk is measured | False | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Stock `.cc` image proves the workload to a third party | False, by inference | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Third-party workload verification works with custom initrd and verity | Conditional; untested | [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/), [decoy-root script](https://docs.verda.com/assets/confidential_compute/encrypted_attested_initramfs.sh) |
| Verda's sample SNP verifier is adequate for third-party use | Weak; no POLICY or TCB check, guest-supplied inputs | [encrypted_03](https://docs.verda.com/assets/confidential_compute/encrypted_03_attest_and_unlock.sh), [measure_boot.sh](https://docs.verda.com/assets/confidential_compute/measure_boot.sh) |
| SNP measurement inputs are stable across hosts and time | Unknown | [vCPU conflict](https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt), [Terms 3.2](https://verda.com/terms-and-conditions) |
| Stock Tinfoil image and published measurements work on Verda | False, by inference; untested | [Tinfoil build doc](https://github.com/tinfoilsh/cvmimage/blob/main/docs/build.md), [toolchain lock](https://github.com/tinfoilsh/cvmimage/blob/main/platform/toolchain.lock.json), [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Tinfoil's approach can be ported as a fork | Conditional; untested | [cvmimage](https://github.com/tinfoilsh/cvmimage), [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| Tinfoil Containers can be deployed on Verda | Unknown; appears Tinfoil-hosted | [Tinfoil blog](https://tinfoil.sh/blog/2026-03-10-tinfoil-containers) |
| Verda serverless containers or managed clusters offer CC | False as of the public API and console; multi-node on roadmap | [API docs](https://api.verda.com/v1/docs), [console notes](verda_browser_notes.md), [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| An onion service can run on a CC instance | Conditional; untested | [Internet access](https://docs.verda.com/private-networks/internet-access/), [shared responsibility](https://docs.verda.com/resources/shared-responsibility-model/) |
| No mandatory guest agent | Conditional; document-level, untested | [00_build_image.sh](https://docs.verda.com/assets/confidential_compute/00_build_image.sh) |
| Terms permit Tor or an anonymous service | Unknown; not addressed | [Terms](https://verda.com/terms-and-conditions) |
| Platform-use clause permits the service | Conditional; needs written confirmation | [Terms](https://verda.com/terms-and-conditions) |
| Contract is under Finnish law | Solid | [Terms](https://verda.com/terms-and-conditions) |
| Verda holds SOC 2, ISO, and related certifications | Conditional; claimed, reports unread | [Homepage](https://verda.com), [trust portal](https://trust.verda.com) |
| API, SDKs, CLI, and Terraform exist | Solid; untested | [API docs](https://api.verda.com/v1/docs), [Terraform](https://docs.verda.com/infrastructure-as-code/terraform/compute-instances/) |
| An attestation API exists | False as of the OpenAPI | [OpenAPI](https://api.verda.com/v1/openapi.json) |
| Verda's physical-extraction FAQ covers bus interposers | False or overbroad, per TEE.fail; TEE track to confirm | [CC page](https://verda.com/confidential-computing), [TEE.fail](https://tee.fail) |
| Verda provides a signed vTPM | Unknown; marketing and docs conflict | [CC page](https://verda.com/confidential-computing), [CC docs](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/) |
| ExpressAI proves the self-service flow works for this use | Weak | [Story](https://verda.com/blog/expressvpn-success-story), [Cure53](https://cure53.de/pentest-report_expressvpn-ai_2026.pdf) |
| RTX PRO CC stock is dependable over time | Unknown; one snapshot, FIN-03 only | [Console notes](verda_browser_notes.md) |
