# Verda: findings read directly from public pages (2026-11-06)

Read in the browser connected to the Claude in Chrome extension, which is Chrome rather than the user's Firefox. The user then signed in to Verda in Chrome, and the logged-in console's deploy forms were inspected read-only. Nothing was deployed or changed, no credentials were entered by Claude, and non-essential cookies were declined on the public sites.

Sources:
- https://verda.com/pricing (read 2026-11-06)
- https://verda.com/confidential-computing (read 2026-11-06)
- https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/ (page says last updated 27.10.2026)
- console.verda.com deploy-instance and containers/new forms, logged in (read 2026-11-06)

## Console findings (logged in, 2026-11-06): authoritative for what can actually be rented

The console has a "Confidential" filter on the instance deploy form. With it on:

| Console SKU ID | GPUs | vCPU | RAM | VRAM | CPU | Locations | On-demand | Status at time of reading |
|---|---|---|---|---|---|---|---|---|
| 1RTXPRO6000.30V.CC | 1x RTX PRO 6000 (only size offered) | 30 | 90 GB | 96 GB | not shown (docs: SEV-SNP) | FIN-03 available; FIN-01, FIN-02 listed unavailable | $2.780/h | Available in FIN-03 |
| 8B200.240V.CC.AMD | 8x B200 | 240 | 1360 GB | 1440 GB | AMD EPYC | FIN-03 available; FIN-01, FIN-02 listed unavailable | $74.566/h | Available in FIN-03 |
| 8B200.240V.CC.INTEL | 8x B200 | 240 | 1360 GB | 1440 GB | Intel Xeon | "None" | n/a | Unavailable, no location |
| B200 CC 2x, 4x | 2x, 4x | not shown | not shown | not shown | not shown | none | n/a | Offered in the size selector but no available instance row appeared for 2x |
| B300 CC | | | | | | | | "Notify me" on-demand; 4x and 8x sizes shown under spot |

Resolved conflict: **B200 CC has no 1x option** (sizes are 2x, 4x, 8x), and the only size actually available was 8x. The pricing page's "1x B200 CC" row is a per-GPU price, not a rentable size. Real entry cost for B200 CC today is $74.57/h on-demand or $37.28/h spot, about $54.4k per 730-hour month on-demand. RTX PRO 6000 CC is about $2.03k per month on-demand.

Other console facts:
- Spot view: B200 CC 8x total shown as $37.283/h. RTX PRO 6000 CC 1x also appeared selectable under spot. The form describes spot as "Low-cost and may be evicted without warning". On-demand is "billed per 10 min".
- With a CC SKU selected, a dedicated "Confidential compute" OS tab appears as the default, with images: Ubuntu 26.04 or 24.04 + CUDA 13.4 or 13.2 Open + CC. Storage shows $0.014/h for the default 50 GiB volume. Kubernetes, Docker, Ubuntu, JupyterLab and Other tabs remain listed. My clicks on "Other" did not visibly change the selection, so its contents with CC are UNKNOWN.
- SSH keys are a required field on the deploy form. Startup script is optional. Network choices: Public + Private IP, Private IP only (SSH through a jump host or own route), or Public IP only (legacy). Inference: an instance can be provisioned with no public IP, which is a plausible fit for an outbound-only onion service, depending on the egress route; see https://docs.verda.com/private-networks/internet-access/.
- Serverless containers form (containers/new): **no Confidential filter and no CC SKUs**, only plain B300, B200, H200, H100, RTX PRO 6000, L40S. This confirms containers are not a CC option today. Container endpoints also get a public URL at containers.datacrunch.io/{deployment-name}.
- Verda Inference (preview) lists one model, Kimi K3.1 Pro, and nothing about confidentiality in the console listing.
- Scheduled maintenance notice for FIN03 on 2026-11-08 19:00-23:00 UTC ("No service interruption expected"). FIN-03 is the only location with CC capacity, so capacity is concentrated in one site. The project shows a $100.00 balance and no instances.

## Verified from Verda's own pages

### SKUs and price
| Listing on pricing page | CPUs | RAM | VRAM | On-demand | Spot |
|---|---|---|---|---|---|
| 1x RTX PRO 6000 CC 96GB | 30 | 90 GB | 96 GB | $2.780/h | $1.390/h |
| 1x B200 CC SXM6 180GB | 30 | 170 GB | 180 GB | $9.320/h | $4.661/h |
| 1x B300 CC SXM6 268GB | 30 | 255 GB | 268 GB | $11.93/h | $5.963/h |

- CPU-only CC instances exist from CPU.4V.16G.CC at $0.04896/h, on AMD EPYC. Possibly relevant for a credential issuer or wallet-facing service, which does not need a GPU.
- The serverless containers price table lists no CC variants. Verda Inference (preview, per-token API) is a separate product and is not described as confidential. So CC appears to be available on VM instances only, not on the "AI platform" container or inference layers. The docs and CC page do not state this outright, so it is an inference from the price tables.
- Multi-node CC is described as "on the roadmap".

### Hardware / TEE matrix (docs)
| Hardware | Size | AMD SEV-SNP | Intel TDX |
|---|---|---|---|
| RTX PRO 6000 | Single GPU | Available | not available |
| B200 | Multi GPU | Available | Available |
| B300 | Multi GPU | Available | Available |
| CPU-only | up to 360x | Available | not available |

The public pages disagree on B200 CC sizing: the pricing page lists "1x B200 CC", the CC page lists "8x GPUs" under a price column headed "(1x GPU)", and the docs say "Multi GPU". The console section above resolves this: no 1x exists. RTX PRO 6000 CC at spot would be about $1.01k per 730-hour month, if eviction is tolerable.

### Launch measurement: tenant control appears to be supported
- QEMU direct kernel boot (-kernel / -initrd / -append) with SEV-SNP kernel hashes. The docs say a SHA-384 digest over OVMF, kernel, initramfs and kernel command line is folded into the launch measurement.
- "Your kernel and initial ramdisk come from your own OS volume, not from Verda." At every boot Verda reads whatever the distro's vmlinuz and initrd.img symlinks point to. A kernel update changes the measurement at the next Shutdown and Start.
- OVMF is host-loaded and customers cannot bring their own. Verda pins one public build: verda-cloud/edk2, branch amdsev-large-bars, release verda-amdsev-largebars-stable202605, commit 0c0bd94a..., with a published SHA-256. It is rebuildable from source according to the docs.
- TDX: TDVF is similarly pinned (MRTD published; "it changes only with a new firmware release"). Kernel and initrd go into RTMR1 and RTMR2. The docs show a `tdx-attest quote <nonce>` flow with REPORTDATA nonce binding, intended for "a remote party", and a verifier that runs off-instance.
- Custom OS: documented flow streams an image onto /dev/vda from a RAM-only dropbear initramfs. Encrypted custom OS: documented flow in which LUKS2 root unlocks only after a remote script verifies an SEV-SNP report bound to a per-boot SSH key and recomputes the measurement. The encrypted flow is Ubuntu 26.04 and Debian-family only.
- In-guest reboot does not bring a CVM back. Restart is Shutdown then Start through console or API, and each boot is freshly measured.

### GPU attestation (docs)
- CPU and GPU are "attested independently".
- `nvattest attest --device gpu --verifier remote` runs inside the CVM, with OCSP status checks on the certificate chains. Sample output: GB20X, VBIOS 98.02.8D.00.01, driver 580.126.09.
- Sample `nvidia-smi conf-compute -q` shows "Multi-GPU Mode: None" and "CPU CC Capabilities: AMD SEV-SNP".
- The GPU does not accept workloads until `nvidia-smi conf-compute -srs 1` is run inside the CVM. Current nvattest does not set the ready state itself.
- PCIe transfers go through bounce buffers with AES-GCM. The docs do not mention TEE-IO or TDX Connect.
- SEV-SNP sample shows family 26 (AMD Turin), 30 vCPUs, and report signed by per-chip VCEK. Standard flow fetches VCEK from AMD (`snpguest fetch vcek`).

## My inferences (not stated by Verda)
1. The critical question looks favourable: a tenant can pick kernel, initrd and possibly cmdline, and a third party could in principle compute the expected measurement from public OVMF plus tenant artifacts. To be verified by the attestation spike.
2. The measurement does NOT cover the root filesystem. Verda's default Ubuntu image with a mutable rootfs and SSH access would attest the boot chain but not the running workload. A verifiable service would need the initrd or cmdline to pin the rootfs (dm-verity or similar) and would need to remove SSH and cloud-init, so the tenant cannot get in either.
3. Binding CPU and GPU evidence is left to the tenant. The obvious approach is for measured code to verify the GPU before setting the ready state, plus exporting evidence for the client. Verda's docs do not show a joint flow.
4. Where the cmdline comes from is not stated. The sample shows `root=UUID=...`. If it is host-supplied, the rootfs hash must go in the initrd.
5. Verda's FAQ says that if a server is physically removed, "cold-boot attacks, RAM extraction, and bus probing" recover only ciphertext (paraphrase of the page). That is narrower than it reads: TEE.fail-class DDR5 interposer attacks reportedly use deterministic memory ciphertext to extract attestation keys. To be checked against the TEE track; do not treat the FAQ as covering this.
6. The RTX PRO 6000 CC SKU looks like the natural PoC target: single GPU, lowest cost, and 96 GB is far more than a small model needs. Its limitation is SEV-SNP only, so there is no TDX option on that SKU.

## Unknown after the public pages and console
- What the "Other" OS tab offers on a CC SKU, and whether a custom image can be supplied at deploy time rather than flashed afterwards using the documented rescue flow.
- Whether spot CC is usable for a service (interruption behaviour).
- Any logging or mandatory agent inside CVMs, and SSH-key injection behaviour with a custom OS. SSH keys are required at deploy, so a verifiable image must demonstrably ignore them after flashing.
- Terms on Tor and anonymizing services (covered by the public-docs Verda track).
