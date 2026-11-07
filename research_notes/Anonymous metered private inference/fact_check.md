# Fact check: anonymous metered private inference notes

Checked 2026-11-06 against primary sources I fetched myself. The notes were used only to find what was claimed. No accounts, purchases, forms or browser tools were used, and no third-party code was run.

## Result

Of 20 claims: **12 confirmed, 6 partly right, 1 wrong, 1 could not verify.** Spike numbers matched their raw output, with one small exception (the T2 row in the spike table).

The conclusions most affected:

- **SNP client policy (claim 5).** AMD removed its TCB and mitigation-bit values from both bulletins on 2026-09-24. The notes' mandatory "TCB floor" cannot currently be derived from AMD, and RTX PRO on Verda is SNP-only.
- **Tinfoil verifier (claim 9).** It is fleet-bound. `existing_systems.md` L100 says this was not found; the source shows it.
- **Verda cmdline (claim 19).** No Verda source says where the non-root part of `-append` comes from. Only `root=UUID` re-derivation is stated. "Host-derived" is stronger than the evidence.

| # | Claim | Verdict | Matters |
|---|---|---|---|
| 1 | EC-ACT draft: date, standing, "remain to be written out" | Confirmed | High |
| 2 | "ACT is an expired individual draft" | Partly right | Low to medium |
| 3 | `act-rs` facts and relationship to the authors | Confirmed | Medium |
| 4 | DDRop as described, including "TDX CI mode does not help" | Partly right | Medium |
| 5 | Fabricked as described, TCB floor values and policy | Partly right | High |
| 6 | BREAKFAST and TEE.fail as described | Confirmed | High |
| 7 | NVIDIA CC modes for RTX PRO 6000 and B200 | Confirmed | High |
| 8 | Tinfoil headers, proxy, terms, licence, `cvmimage` | Confirmed | High |
| 9 | Tinfoil chip allowlist "Not found" | Wrong | High |
| 10 | "Tinfoil hosts on its own bare metal" | Could not verify | Low to medium |
| 11 | Meta: multi-use ACS tokens, source of timing warning | Confirmed | High |
| 12 | Apple PCC token reuse | Confirmed | Medium |
| 13 | Stripe stored value wording and scope | Confirmed | High |
| 14 | Arti 2.8.0 conflicting statements on onion services | Confirmed | High |
| 15 | Live Tor incident and OnionPerf figures | Confirmed | Medium |
| 16 | Whisper Leak and Weiss et al. | Partly right | Low to medium |
| 17 | vLLM extensions and default telemetry | Confirmed | Medium |
| 18 | OpenPCC licence and in-memory banks | Confirmed | Medium |
| 19 | Verda kernel cmdline source | Partly right | High |
| 20 | Blackwell GPU-CC overhead numbers | Partly right | Medium |

## Claims

### 1. EC-ACT draft

- **Claim:** `draft-schlesinger-mole-ec-act-00`, 2 Nov 2026, expires 6 May 2027, individual submission, authors Schlesinger, Katz, Faz-Hernandez; reductions "remain to be written out" (`credentials_crypto.md` L20, L25, L62, L71).
- **Verdict:** Confirmed.
- **Source:** https://datatracker.ietf.org/doc/draft-schlesinger-mole-ec-act/ and the -00 text.
- **Difference:** None. The tracker shows an active individual draft, Informational, IESG state "I-D Exists". The draft also says bounds and quantum random-oracle unlinkability "also remain open", and concrete bounds "remain to be established". Both strengthen the note's caution.
- **Matters:** High. It supports the "capped PoC, not a production money system" conclusion.

### 2. "ACT is an expired individual draft"

- **Claim:** "ACT is an expired individual draft with no IETF standing" (`purchase_wallet_abuse.md` L16, also L125).
- **Verdict:** Partly right.
- **Source:** Same tracker page.
- **Difference:** Only the older `draft-schlesinger-cfrg-act-01` is expired. Its successor has been active since 2 Nov 2026, as `credentials_crypto.md` L62 says. "No IETF standing" is fair, since no working group has adopted it.
- **Matters:** Low to medium. The two notes conflict, and a reader of the purchase note alone would think the spec is dead.

### 3. `act-rs`

- **Claim:** 0.1.0, created 2026-09-07, pushed 2026-11-04, Apache-2.0, "not an officially supported Google product", vectors at L=1,2,4,8,16,64, CI with mutation testing and Miri, crates.io name collision (`credentials_crypto.md` L80, L83; `spike_act.md` L33).
- **Verdict:** Confirmed.
- **Source:** https://github.com/Moderation-of-unLinkable-Endorsements/act-rs , https://api.github.com/repos/Moderation-of-unLinkable-Endorsements/act-rs , https://crates.io/api/v1/crates/act-rs
- **Difference:** Two refinements only.
  - Relationship to the authors: the repo is in the MoLE org, but its contributors are SamuelSchlesinger (19 commits), thibmeu (4) and cjpatton (3). Only one of the three draft authors contributes.
  - The crates.io entry is registered as `act_rs` 0.5.0. crates.io treats the hyphen and underscore as the same name, so the collision is real.
- **Not checked:** The vector files and workflow mentions exist, but I did not run them. No `act-rs` code has run in this research (`spike_act.md` L10).
- **Matters:** Medium. It is a supply-chain trap, and the primary library remains unmeasured.

### 4. DDRop

- **Claim:** The table row, plus "TDX CI mode does not help" (`tee_gpu_attestation.md` L127, L141-146).
- **Verdict:** Partly right. The attack is confirmed; L143 is overstated and misattributed.
- **Source:** https://ddropattack.eu/ddrop.pdf , https://ddropattack.eu/ , https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3048.html , https://www.intel.com/content/www/us/en/security-center/announcement/intel-security-announcement-2026-08-11-001.html
- **Confirmed:**
  - Authors from KU Leuven, ETH, Durham and Google; CCS '26.
  - Root, BIOS and "brief, one-time physical access"; $159 BOM (quantity 10, no labour).
  - 2 x Xeon Silver 4509Y on TDX 1.5.16; 2 x EPYC 9555 on SNP 1.58.4.
  - Forgery shown on TDX only. On SNP, a relocation copy only.
  - Intel treats it as outside its threat model.
- **Difference:**
  - The quoted L143 phrase is on the DDRop website, not in the linked paper.
  - The paper says CI mode would prevent the victim-ciphertext and debug-toggle case studies. Own-TD quote forgery is expected to survive CI, but the authors' BIOS lacked CI, so this was "unable to verify" experimentally. "Does not help" is therefore right for forgery and wrong for the other two.
  - L141 omits that the paper says its limits "do not inherently preclude" single-socket machines.
- **Gap closed:** AMD-SB-3048 (L169 "not retrieved") says the attack is "outside the scope" of SNP's threat model, and AMD "does not plan to assign a CVE or release mitigations". It lists EPYC 4004, 4005, 8004, 9004, 9005 and MI300A.
- **Also:** The table's NVIDIA CC row is "inferred from documentation", not tested.
- **Matters:** Medium. The client still cannot trust a TDX quote against this attacker, so the policy conclusion stands. CI does change what a TDX B200 protects if attestation is otherwise sound.

### 5. Fabricked and the TCB floor

- **Claim:** In `tee_gpu_attestation.md`: TurinPI 1.0.0.8 with TCB[SNP] at least 0x5, GenoaPI 1.0.0.H at least 0x1C, MilanPI 1.0.0.J at least 0x1D plus mitigation bit 4 (L139). SNP's failures are "mostly closed by a TCB floor" (L161). Mandatory floor covering AMD-SB-3034 and BREAKFAST's fixes (L162).
- **Verdict:** Partly right. The attack and PI versions are confirmed; the floor values and the policy built on them are not supported by AMD as it stands.
- **Source:** https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3034.html , https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3030.html , https://xca-attacks.github.io/fabricked/ , https://www.shwetashinde.org/publications/fabricked_usenix26.pdf
- **Confirmed:** USENIX Security 2026; Schlüter, Wech, Shinde (ETH); UEFI plus hypervisor, no physical access; Zen 5 EPYC 9135 confirmed, Zen 3 and 4 assumed; not TDX or CCA; "also forges attestation reports"; CVE-2025-54510, CVSS 5.9.
- **Difference:**
  - AMD-SB-3034 now shows Microcode, SEV FW and TCB as N/A, with a revision dated 2026-09-24, "Removed TCB values".
  - AMD-SB-3030 has the same date, "Removed mitigation bits and TCB values".
  - The note's floor figures survive only on a secondary page (Thomas-Krenn), which also dates the Milan fix 2025-12-15 where AMD says 2025-12-02.
  - The note did flag these values as secondary. It could not know they had been withdrawn. I found no stated reason for the removal.
- **Matters:** High. RTX PRO on Verda is SNP-only, and an anonymous client's defence against these two forgery attacks rests on this floor. How a client confirms the PI fix from a report is now an open question.

### 6. BREAKFAST and TEE.fail

- **Claim:** The table rows (`tee_gpu_attestation.md` L126, L136).
- **Verdict:** Confirmed.
- **Source:** https://www.shwetashinde.org/publications/breakfast_oakland26.pdf , https://xca-attacks.github.io/breakfast , https://tee.fail/ , https://tee.fail/files/paper.pdf
- **BREAKFAST:**
  - IEEE S&P 2026; Giersfeld, Schlüter, Shinde (ETH); hypervisor only.
  - The genuine PSP signs a spliced measurement, so no VCEK is extracted.
  - CVE-2025-61971 and CVE-2025-61972.
  - The paper tests EPYC 9135 and EPYC 9124. The project site says EPYC 9334 for Zen 4, so the sources disagree. The note matches the paper.
  - The note's "fix versions unread" gap closes via AMD-SB-3030: TurinPI 1.0.0.8, Genoa++ 1.0.0.H, MilanPI 1.0.0.J.
- **TEE.fail:**
  - Chuang, Seto, Berrios, van Schaik, Garman, Genkin; IEEE S&P '26.
  - Interposer under $1000 plus root; Xeon Silver 4509Y; EPYC 9015 with Ciphertext Hiding.
  - PCK extraction and a forged quote at UpToDate. On SNP, an OpenSSL ECDSA key, with no VCEK claim.
  - Small addition: AMD also treats it as out of scope, while the note names only Intel.
- **Matters:** High. These set the threat model.

### 7. NVIDIA CC modes

- **Claim:** RTX PRO 6000 is single-GPU CC only; B200 has single and multi-GPU passthrough on SNP or TDX; Protected PCIe is Hopper-only; no release note says TEE-IO has shipped (`tee_gpu_attestation.md` L8, L11-22, L37).
- **Verdict:** Confirmed.
- **Source:** https://docs.nvidia.com/confidential-computing/release-notes/r580-trd1.md , https://docs.nvidia.com/confidential-computing/release-notes/r590-trd1.md , https://docs.nvidia.com/confidential-computing/release-notes/r595-trd1.md , https://docs.nvidia.com/confidential-computing/virtual-machines/v1.0.0/supported-platforms.md
- **Difference:** None on the claim. Two notes:
  - The whitepaper also has present-tense TDISP/IDE lines next to future-tense ones. No release note lists it, so the note's conclusion stands.
  - `verda_platform.md` L106 and L327 still ask about "Protected PCIe" on B200, contradicting L8. The relevant mode is MPT. NVIDIA says "up to eight" GPUs and does not state 2x or 4x.
- **Matters:** High. It fixes the CC mode and bounce-buffer path for both SKUs.

### 8. Tinfoil headers, proxy, terms, licence, `cvmimage`

- **Claim:** `existing_systems.md` L38-42, L65, L76, L79-81; `inference_metering.md` L98.
- **Verdict:** Confirmed.
- **Source:** https://docs.tinfoil.sh/guides/proxy-server.md , https://docs.tinfoil.sh/admin/admin-api.md , https://github.com/tinfoilsh/confidential-model-router/pull/518 , https://tinfoil.sh/terms , https://tinfoil.sh/terms/aup , https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/LICENSE , https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/AGENTS.md
- **Difference:** None. `X-Tinfoil-Usage-Metrics`, `X-Tinfoil-Max-Output-Tokens`, the pooled-key proxy pattern, "You may not share accounts or API keys", "non-transferable", `verify/` under AGPL v3 inside an Apache-2.0 repo, and "needs Tinfoil host access" all check out. Resale is not named in the terms, as the note says.
- **Addition:** The AUP also bars falsifying "billing, usage, verification, or attestation data", which the note does not list.
- **Matters:** High. The tension between a documented proxy and "may not share" is real, and anonymous resale still needs a written answer from Tinfoil.

### 9. Tinfoil chip allowlist

- **Claim:** "Whether any Tinfoil verifier binds to a chip allowlist ... Not found" (`existing_systems.md` L100), and the verifier is "provider-neutral in shape" (L89).
- **Verdict:** Wrong at L100. `spike_attestation.md` L17 and L220 are right.
- **Source:** https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/internal/policy/artifact.go
- **Difference:** `PolicyFor` looks up the authenticated identifier in a machines map, and an absent identifier returns the error "is not endorsed". The SEV code uses CHIP_ID as that key and rejects masked chip IDs. The TDX code uses PPID.
- **Limit:** No run has exercised the reject path, so the spike's "inconclusive" label is honest about testing. The code itself is unambiguous.
- **Matters:** High. The default verifier cannot accept Verda hardware, so a Verda backend needs a fork or separate verifier. L89's "safer assumption" reaches the same end, but for an incomplete reason.

### 10. "Tinfoil hosts on its own bare metal"

- **Claim:** `verda_platform.md` L198.
- **Verdict:** Could not verify.
- **Source:** https://tinfoil.sh/security-and-privacy-faq
- **Difference:** The FAQ speaks of "our infrastructure providers" including "GPU cloud providers", and "our cloud providers control resource allocation". I found no statement of own hardware or bare metal. `existing_systems.md` L53 reports the same providers. The 25-machine inventory shows machines, not who owns them.
- **Matters:** Low to medium. "Two separate backends" still holds, because of the fleet binding and `cvmimage`. The stated reason should be dropped or sourced.

### 11. Meta Private Processing

- **Claim:** ACS tokens are multi-use; the issuance-to-use timing warning comes from Trail of Bits; relay-gateway collusion comes from NCC (`existing_systems.md` L108, L115-118, L121).
- **Verdict:** Confirmed.
- **Source:** https://github.com/trailofbits/publications/raw/master/reviews/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf , https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf
- **Difference:** None. TOB-WAPI-4 says "up to a maximum number of times" and "concurrent requests bearing the same token". The prefetch timing warning is in Trail of Bits, and NCC has no hits for "prefetch". NCC's warning is the relay and gateway collusion. Minor: L116 links a third-party mirror, while Trail of Bits publishes the report itself.
- **Not checked:** Whitepaper p22 "finite number of times".
- **Matters:** High. These findings shape the unlinkability design.

### 12. Apple PCC token reuse

- **Claim:** A used OTT was reused "between 2 and 5 times", and a TGT and OTT swap from another Mac worked (`existing_systems.md` L146-147, L153, L371).
- **Verdict:** Confirmed.
- **Source:** https://arxiv.org/pdf/2605.24239
- **Difference:** None. Nuance: reuse then "stopped working", so it is bounded rather than unlimited, and the authors report responsible disclosure. Current status is unknown.
- **Matters:** Medium. It supports explicit double-spend tests.

### 13. Stripe

- **Claim:** "Nonfiat currency and stored value" is on the Restricted list; seller-maintained credits "may be subject to limits"; no anonymity entry; "unlikely" wording; billing-credits limits (`purchase_wallet_abuse.md` L35-38).
- **Verdict:** Confirmed.
- **Source:** https://stripe.com/legal/restricted-businesses , https://support.stripe.com/questions/restricted-business-list-and-considerations , https://docs.stripe.com/billing/subscriptions/usage-based/billing-credits.md
- **Difference:** None.
  - The heading sits in the Restricted section, which requires "additional due diligence", with approval revocable.
  - The prohibition-style wording targets credits issued by anyone other than the seller. Seller-issued credits are not barred, only "subject to limits".
  - "Unlikely" is on the support page, as the note cites, not on the list.
  - The list has zero hits for anonymity, VPN or proxy terms.
- **Matters:** High. Approval is plausible but discretionary, and nothing addresses bearer anonymous credentials.

### 14. Arti 2.8.0

- **Claim:** Upstream statements on onion-service readiness conflict (`tor_traffic_analysis.md` L95-101, L424; `spike_tor.md` L22, L41-46).
- **Verdict:** Confirmed.
- **Source:** https://blog.torproject.org/arti_2_8_0_released/ , https://arti.torproject.org/ , https://arti.torproject.org/FAQs , https://arti.torproject.org/guides/capability-limitations , https://docs.rs/crate/arti/2.8.0/source/src/arti-example-config.toml , https://crates.io/api/v1/crates/arti/2.8.0
- **Difference:** None.
  - The apparent date clash has an answer: the blog post is dated 3 Nov and the crate was published 4 Nov (14:29 UTC).
  - The homepage says "ready for use". The example config says some features needed for privacy "are not yet implemented" and links to the February 2024 document. `[vanguards]` is present, so that document is partly stale, as the notes say.
- **Not checked at source:** GitLab returned 403 to my fetches, so I compared `OnionService.md` against the spike's tagged copy only.
- **Matters:** High. No current upstream document lists what is actually missing, so hosting on Arti needs its own review. C tor is the safer default.

### 15. Live Tor incident and OnionPerf

- **Claim:** `tor_traffic_analysis.md` L18-36, L55-59, L423.
- **Verdict:** Confirmed, with one trivial difference.
- **Source:** https://forum.torproject.org/t/security-release-0-4-9-15/22332 , https://forum.torproject.org/t/security-release-0-4-9-16/22372 , https://forum.torproject.org/t/tor-0-4-9-15-begin-dir-hsdir-flood-oom-kill-despite-maxmeminqueues/22359 , https://forum.torproject.org/t/tor-relays-re-does-the-begin-dir-attack-affect-user-count-metrics/22366 , https://metrics.torproject.org/userstats-relay-country.csv , https://metrics.torproject.org/onionperf-latencies.csv , https://metrics.torproject.org/onionperf-throughput.csv , https://metrics.torproject.org/torperf-failures.csv
- **Incident:** "thunderstorm of directory requests" (0.4.9.15, 28 Oct) and "an emergency release" (0.4.9.16, 4 Nov) are both right. So is the attribution of the user spike, "very likely" to prohibiting pre-0.4.9.x clients.
- **Difference:** My copy of the users CSV gives 20.3M on 11-03, just under the note's 23M floor. The 10-28 to 11-02 range is 23.4M to 64.5M.
- **OnionPerf, recomputed by me, 2026-08-08 to 09-21, median of daily medians, DE/US/HK:**

| Metric | Server | Note | Recomputed |
|---|---|---|---|
| Latency (ms) | public | 110 / 270 / 520 | 110 / 270 / 520 |
| Latency (ms) | onion | 270 / 425 / 670 | 270 / 425 / 670 |
| Throughput (kbit/s) | public | 11,816 / 8,559 / 5,518 | 11,816.5 / 8,559 / 5,518 |
| Throughput (kbit/s) | onion | 5,309 / 4,279 / 3,017 | 5,309 / 4,279 / 3,017 |
| Timeouts, mean of daily | onion | 1.7-2.4% | 2.13 / 1.72 / 2.38% |
| Timeouts, mean of daily | public, US and HK | 0.55-0.68% | 0.55 / 0.68% |

- **Not checked:** Quartiles, whiskers, download times, build times and newer `op-*9a` sources.
- **Matters:** Medium. The baselines are sound. The spike ran during this incident, so its figures may not be typical.

### 16. Whisper Leak and Weiss et al.

- **Claim:** Time-only AUPRC "ranging 56.4-84.0% per model" (`tor_traffic_analysis.md` L272), and "70-84% time-only range" (L312), plus the attack and mitigation figures (L271-274, L361).
- **Verdict:** Partly right.
- **Source:** https://arxiv.org/abs/2511.03675 , https://arxiv.org/abs/2403.09751 , https://www.usenix.org/conference/usenixsecurity24/presentation/weiss
- **Difference:** The range matches no column in Table 1. It takes its low from LSTM and its high from BERT.

| Time-only AUPRC | Low | High |
|---|---|---|
| BERT | 46.3 | 84.0 |
| LSTM | 56.4 | 85.4 |
| LightGBM | 56.8 | 83.4 |
| Best per model | 57.6 | 85.4 |

- **Confirmed:** 28 models; 17 of 28 above 98%; 100% precision at 5-20% recall at 10,000:1; 11,716 negatives; time-only averages 69.7-71.8%; mitigations 97.5 to 92.9, 98.2 to 94.8, 98.1 to 93.3; Weiss 29%/55% on arXiv against 27%/53% at USENIX. Tor appears only in related work, and my search found no Tor test, which is an absence, not proof.
- **Matters:** Low to medium. The Tor reading remains an extrapolation, as the note says, but its input should be about 46-85%, not 70-84%.

### 17. vLLM

- **Claim:** `return_token_ids`, `continuous_usage_stats`, default usage telemetry (`inference_metering.md` L57, L64, L142, L161, L198, L289).
- **Verdict:** Confirmed.
- **Source:** https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/ , https://docs.vllm.ai/en/latest/usage/usage_stats/ , https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py
- **Difference:** Minor. `serving.py` uses the derived name `include_continuous_usage`. The field `stream_options.continuous_usage_stats` is in vLLM's API utilities (checked in the spike's v0.33.0 copy, L280-281), so L57's citation is slightly off.
- **Limit:** vLLM was not run in `spike_metering.md`, so nothing about it is tested.
- **Matters:** Medium. Telemetry must be disabled in the image, and the metering results need repeating on vLLM.

### 18. OpenPCC

- **Claim:** Apache-2.0, 957 stars, 45 commits, in-memory banks only so far as read (`existing_systems.md` L163, L166, L175, L188, L370, L462).
- **Verdict:** Confirmed, at listing level.
- **Source:** https://github.com/openpcc/openpcc , https://github.com/openpcc/openpcc/tree/main/anonpay/banking , https://github.com/openpcc/openpcc/tree/main/cmd
- **Difference:** None. `banking` has `httpapi`, `inmem` and `testcontract`; `cmd` has `mem-*` services; the README says "in-memory services". I found no durable store, but this is absence evidence from listings, and L188 correctly calls it unresolved.
- **Matters:** Medium. It is a good reference, and persistent double-spend state would have to be built.

### 19. Verda kernel cmdline

- **Claim:** "appears to be host-derived" (`verda_platform.md` L120, L135, L147-152); "Verda, via QEMU -append" (`spike_attestation.md` L86); "Unknown; evidence conflicts" (`verda_platform.md` L399).
- **Verdict:** Partly right. L399 and `spike_attestation.md` L348 are right. L120 and spike L86 go beyond the sources.
- **Source:** https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/ , https://docs.verda.com/assets/confidential_compute/encrypted_env_sample.txt , https://docs.verda.com/assets/confidential_compute/encrypted_00_build_image.sh , https://docs.verda.com/assets/confidential_compute/00_build_image.sh , https://docs.verda.com/assets/confidential_compute/measure_boot.sh
- **Difference:**
  - Verda documents QEMU direct kernel boot with `-append` and "no GRUB step". No document says where the `-append` text comes from.
  - Only `root=UUID` re-derivation is stated ("Verda re-syncs").
  - The note's evidence omits the sample's comment that CMDLINE is "baked into /boot", which points the other way.
  - It also omits that `encrypted_00_build_image.sh` has no GRUB edit and uses `CMDLINE` only to write `verify/cmdline.txt`, yet the sample shows the same `net.ifnames` and `fsck` values. So Verda's own comment is not backed by that script.
- **Matters:** High. It decides whether `roothash=` can sit on the cmdline and how portable Tinfoil's image is. Putting the hash in the initrd is safe either way, and the on-instance test should settle the rest.

### 20. Blackwell GPU-CC overhead

- **Claim:** "lost 13-27% throughput under TDX plus GPU-CC" (`inference_metering.md` L26); 34.4-38.9% for Qwen3-8B (`tee_gpu_attestation.md` L207, L213, L326); "could not find support in its tables for a worker-thread '92%' claim" (`tee_gpu_attestation.md` L219).
- **Verdict:** Partly right. 34-39% confirmed, 13-27% lacks its scope, and the 92% statement is wrong.
- **Source:** https://arxiv.org/abs/2606.23969 , https://arxiv.org/abs/2608.26575
- **Difference:**
  - **13-27%:** all endpoints are B300, neither target SKU: dense -13.0%, Gemma c=128 -14.2%, MoE -24.7% and -27.6%. L26 does not say so. RTX Pro 6000 is the separate "about 10%" dense figure. There is no B200 and no SNP.
  - **34-39%:** Qwen3-8B bf16, one B200, shipped SGLang 0.5.13, TDX on Xeon 6767P, concurrency 16/32/64 gives 34.4/34.6/38.9%. It is SGLang rather than vLLM, and TDX rather than SNP. L213 names SGLang; L326 names neither.
  - **92%:** the Phala paper states it in its abstract, its contributions and its recovery table ("up to 92% at c=512 (qualified)"). The real limit is that it was not reproduced. `inference_metering.md` L27 has this right, so the notes conflict.
  - **57% and about 1% are not in conflict:** the first is the gap to the CC-off async baseline, and the second is the residual against CC-off sync.
- **Matters:** Medium. No measured figure covers either target as Verda rents it: RTX PRO on SNP. Budget for measuring it.

## Spike numbers against raw output

Raw root: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes`

| Spike | Notes value | Raw file | Result |
|---|---|---|---|
| ACT | T2 flips: 1,327 decode errors, 4,121 `InvalidProof`, 5,448 total, 0 accepted | `act/raw/ristretto_t2.txt` | **Differs:** raw says `decode_err=1330 InvalidProof=4118`. Total and 0 accepted match. No raw file has the note's split |
| ACT | Checks 11, 27, 11 | `ristretto_t1.txt`, `t2`, `t3` | Match |
| ACT | L=32 sizes 4,919 / 176 / 141, in-flight 5,060, server 240 | `ristretto_t6.tsv`, `ristretto_t3.txt` | Match |
| ACT | Verify 1,534 us, reject 1,528 us, junk 0.38 us, 617 per second | `ristretto_t6.tsv` | Match |
| ACT | 42x VOPRF, 76x blind RSA | `circl_t6.tsv` | Match (36.33 and 19.96 us) |
| ACT | Races 200/200 and 200/200, split 50/50, 8,600 stranded | `ristretto_t3.txt`, `ristretto_t4.txt` | Match |
| Metering | 421/434 prompts, 434/434 hold | `metering/raw/t12_analysis.txt` | Match |
| Metering | 242/434, 88/434, 157/434, 92/145, E and F 0/434 | `t12_analysis.txt` | Match |
| Metering | No usage on aborted runs, 0 of 108 | `t3_*.jsonl` | Match |
| Metering | +2 tokens 54/54; stalled 38/39, exception 433 of 600 | `t3_*.jsonl` | Match (39 FIN, 15 RST) |
| Metering | Inflation 3.27x | `t5b_inflation.json` | Match (3.2686) |
| Metering | 54,079 vs 43,589 (19% unused), 241 at zero | `final_checks.txt` | Match |
| Attestation | 4 of 4 measurements, 44 of 44 negatives | `attestation/output/A3_real_report_repro.txt` | Match |
| Attestation | 10 runs one digest, 26 of 26 mutations | `A3_sensitivity_log.txt` | Match (27 cases include baseline) |
| Attestation | 36 cases, 0 unexpected | `B/B_offline_verify_sandboxed.txt` | Match (18 + 18) |
| Attestation | SDK 8 of 8; CLI fails | `B/S*.exit` | Match (S3 x5, S4, S5, S6 exit 0; S7 exit 1) |
| Attestation | 8 of 8 SPDM; root single | `B/C_gpu_evidence_check.txt` | Match |
| Attestation | Inventory 25: 12 Turin, 1 Genoa, 1 Genoa dev, 11 TDX | `B/B_fleet_inventory.txt` | Match |
| Tor | Fresh 13/15, median 4.333 s; reuse 15/15, median 1.368 s | `tor/raw/results/iso.json` | Match |
| Tor | Cold first byte 7.738 s; connect failures 3/42 | `tor/raw/results/cold.json` | Match |
| Tor | Pooled streams 30/60/100; sustained 9,000 and 18,000 tokens; total 44,100 | `tor/raw/results` | Match |

## Limits of this check

- **Partly read:** `purchase_wallet_abuse.md` lines 1-486 of 753, and `spike_tor.md` lines 1-135 of 238. Claims outside those ranges were not reviewed.
- **Not reviewed:** `red_team.md`, which appeared after this check started.
- **Local copies:** `OnionService.md` and the vLLM API utilities were read from spike downloads, because direct fetches failed or used a wrong path.
- **Selection:** Twenty claims were chosen by impact on conclusions. Claims not listed here are neither confirmed nor refuted.
