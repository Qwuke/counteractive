# Review of prior_investigation_2026_10_25.md

Reviewed 2026-11-06. **[E]** evidence-backed: a primary source I read, or a spike result within its stated limits. **[I]** my inference. `prior:N` is a line in the document under review.

## 1. How to use this file

The document's problem statements are mostly sound and several concepts survive, but its numbers are model output and its strongest security sentences outrun the sources. Cite the primary sources in Section 2, not the document. Rely on rows marked confirmed, and reword partly-right rows using the stated difference. Treat every concept in Section 4 as a hypothesis: none ran on Verda or a GPU, and the host-side GPU leak is plausible, not demonstrated. Do not repeat the statements listed in Section 5.

## 2. Claim checks

Tally: 24 rows; 11 confirmed, 11 partly right, 1 wrong, 1 could not verify. All differences are [E] unless tagged.

| # | Claim | Verdict | Primary source | Exact point of difference |
|---|---|---|---|---|
| 1 | zkAPI: one spend per session, capped key, vault, receipt | partly right | [EF](https://blog.ethereum.org/2026/10/01/introducing-zkapi), [OA](https://openanonymity.ai/blog/posts/oa-ethereum-foundation-collab.md), [thread](https://ethresear.ch/t/zk-api-usage-credits-llms-and-beyond/24104) | True of the Oct 2026 build, which OA led with the EF. A signed receipt charges the note; it is not called a refund. Provider sees prompts; zkAPI server learns dollars per session. The Feb 2026 thread (ref 17) is per-request, so "both use one spend per session" misdescribes it. |
| 2 | Open Anonymity: session-scoped capped keys | confirmed | [OA](https://openanonymity.ai/blog/posts/unlinkable-inference.md) | Fixed tickets per prompt or session, blind RSA. Publication date not checked. |
| 3a | ProxyGPT | confirmed | [2407.08792](https://arxiv.org/html/2407.08792) | Volunteer proxies, blind e-cash, Tor. Proxies see plaintext; no TEE. |
| 3b | funion is arXiv 2501.02933 | wrong | [2505.18398](https://arxiv.org/html/2505.18398v1), [2501.02933](https://arxiv.org/abs/2501.02933) | 2501.02933 is Echomix. funion is 2505.18398. The brief for this review repeats the wrong ID. |
| 3c | Mixnet about ten seconds per turn | partly right | same | Modelled, not built. 9.0 s is the mixnet share; totals are 13–48 s; no streaming. |
| 4 | NCC: issuance and inference timestamps correlate inside one operator | partly right | [NCC v1.4](https://www.fox-it.com/media/cbuhuhuu/ncc_group_google_e025353p_report_2025-11-08_v14.pdf) | Analytic, not demonstrated: "weak correlations", Medium risk, Low exploitability, centred on lone or distinctive users. Yet "most prominent risk". Trail of Bits on Meta (TOB-WAPI-4) is separate. [I] "No relay can remove it" is sound but the document's own. |
| 5 | Pham: at least one conversation for 50–75% of heavy users | partly right | [2609.33903](https://arxiv.org/abs/2609.33903) | Exists. 50–75% holds for in-set SWE-Chat users (n≈30), no time gap, on a metric the paper says inflates. WildChat reaches about 10% at 10% FAR. Needs ID-linked history. "Heavy" is the document's gloss. |
| 6 | Whisper Leak: timing-only about 70% AUPRC | confirmed | [2511.03675](https://arxiv.org/html/2511.03675) | Paper's time-only averages are 69.7–71.8%, range 46–85 by model. One topic against 11,716 negatives, chance about 50%. Not tested over Tor. |
| 7 | Prop 331 | confirmed | [Spec](https://spec.torproject.org/proposals/331-res-tokens-for-anti-dos.html) | Draft since 2021, "about 200 bytes", not implemented in Tor. |
| 8 | ARC dead over PQ unlinkability | confirmed | [Datatracker](https://datatracker.ietf.org/doc/draft-ietf-privacypass-arc-crypto/history/) | Dead WG Document, 2026-07-22. "Unable to develop an attractive version" is softer than "nobody could". |
| 9a | ACT information-theoretic, including against quantum | partly right | [EC-ACT -00](https://www.ietf.org/archive/id/draft-schlesinger-mole-ec-act-00.txt), [cfrg-act-01](https://www.ietf.org/archive/id/draft-schlesinger-cfrg-act-01.txt) | Older drafts say so. Current draft: "intended"; the idealized argument "remain[s] to be written out"; hash-derived randomness needs quantum-RO analysis that "remains open". |
| 9b | Two refunds of one spend share the next nullifier | confirmed | EC-ACT §4.6.5 | At most one can be spent. Spike T2: a re-request with larger `t` mints a sibling, so a result store is needed. |
| 9c | Spend can leave `t` to session close | partly right | EC-ACT; [MoLE](https://www.ietf.org/archive/id/draft-jms-mole-protocols-01.txt) §5.2.3.2 | The crypto allows it: IssueRefund is a separate step. The MoLE profile does not: result recorded first, then "MUST NOT attempt" otherwise. credentials_crypto.md is right. |
| 10a | Tinfoil usage fields, no pre-authorization | confirmed | [Admin API](https://docs.tinfoil.sh/admin/admin-api) | Exact phrase present; limits are "soft". |
| 10b | Tinfoil web-search timing remark | confirmed | [Blog](https://tinfoil.sh/blog/2026-01-22-private-ai-web-search) | Conditional on a small user base and both parties compelled. |
| 10c | Moderation violation tied to account | confirmed | [Blog](https://tinfoil.sh/blog/2026-09-14-safety-without-compromising-privacy) | Flags accumulate to suspension. "Bit" is the document's word. |
| 10d | Random padding in Tinfoil's proxy | partly right | [FAQ](https://tinfoil.sh/security-and-privacy-faq) | FAQ says it pads streaming responses. "Random" and "proxy" are not stated; I found no padding code in the router's Go sources. Timing and byte volume stay visible. |
| 11 | Apple cost tokens, re-linking | confirmed | [Attacks](https://security.apple.com/documentation/private-cloud-compute/attacks), [notes](https://security.apple.com/documentation/private-cloud-compute/releasenotes) | Apple names re-linking through "timing and amounts". Omitted: bucketed units, hour timestamps, aggregation, relays; signing keys not yet logged. [I] It is quota, not money, so the refund "lesson" (prior:317) is the document's. |
| 12a | Battering RAM, WireTap | partly right | [BR](https://batteringram.eu/), [WT](https://wiretap.fail/) | Both DDR4 only. WireTap studied SGX only, so "Intel and AMD" fits Battering RAM alone. |
| 12b | TEE.fail | partly right | [Site](https://tee.fail/) | TDX key extraction reads as automated from one signing operation; "in some cases" is not there. SNP: OpenSSL ECDSA keys, correct. Omitted: GPU attestation relay. |
| 13 | About 5% chargeback loss far above monitoring thresholds | partly right | [Stripe](https://docs.stripe.com/disputes/monitoring-programs) | Visa: non-compliant 5 and 0.5%; excessive 1,500 and 1.5% outside CEMEA. Mastercard ECM: 100 and 1.5%. Counts, regardless of outcome, not value. [I] 300 payments at 5% is 15 disputes: over Visa non-compliant, under excessive and ECM. |
| 14a | C tor single onion experimental, non-anonymous | confirmed | [Manual](https://manpages.debian.org/testing/tor/torrc.5.en.html) | Stated as such. |
| 14b | Arti removed single onion | could not verify | [2.8.0 config](https://docs.rs/crate/arti/2.8.0/source/src/arti-example-config.toml) | Config has no such section. I found no statement that it existed or was removed; GitLab returned 403. "Absent" is safe. |
| 14c | Arti onion hosting only for testing | partly right | [Arti](https://arti.torproject.org/) | Homepage: "ready for use". Config: privacy features "not yet implemented". The testing-only line is in a Feb 2024 document (spike_tor.md). Contested, not settled. |

## 3. Host-side GPU observation

**Demonstrated or documented [E]**
- All CPU-GPU traffic crosses bounce buffers in shared memory as ciphertext ([whitepaper](https://docs.nvidia.com/nvidia-secure-ai-with-blackwell-and-hopper-gpus-whitepaper.pdf); [CVM threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model.md)).
- Performance counters are disabled in CC-On and management paths give health-level data. So the direct reading of "GPU busy" is closed by those paths. The CVM threat model is silent on utilisation, interrupts, timing and traffic volume.
- NVIDIA's [bare-metal threat model](https://docs.nvidia.com/confidential-computing/bare-metal/threat-model) disclaims timing and power side channels, and adds that filtering telemetry does not remove every one. It does not cover CVMs. Tinfoil's FAQ lists I/O pattern leakage as unprotected.
- On H100, [arXiv 2507.02770](https://arxiv.org/abs/2507.02770) found a size-dependent RPC timing signal over 4,394 transfers, and metadata and semaphores exposed in shared memory. It ran no LLM workload.

**Plausible, not demonstrated [I]**
- At low load, one stream on one GPU, the host could estimate input size from upload volume and prefill duration, and output length from per-step transfers. That is the PoC case. Batching blurs this at scale.
- Interrupt and doorbell paths run through host-controlled virtualisation. No NVIDIA document I read addresses them.
- The host forwards guest packets, a more direct channel than PCIe. It can also delay them after any shaper (red_team.md attack 15).
- Server-level power may show busy and idle. Not studied under CC.

**Only asserted**
- "Reveals the same lengths" (prior:208). I found no paper recovering token counts or lengths from the host side on H100 or Blackwell CC. Nearby work is not CC: SparLeak, NVBleed. A negative search is not proof of absence.
- 20–35% of throughput. No source exists; the document calls it estimated, and prior:367 limits its 20 ms step to small models. Measured figures in that range are CC overhead, not padding: 13–27% on B300 under TDX ([2606.23969](https://arxiv.org/abs/2606.23969)); 30–40% on stock stacks, 1–3% tuned ([2608.26575](https://arxiv.org/abs/2608.26575)). I read abstracts only. RTX PRO 6000 on SNP is unmeasured (fact_check.md §20).
- That a fixed-shape engine closes the channel. Step cost depends on KV length, so constant shape needs fixed attention work. [I]
- [I] At saturation batches are already full, so dummies cost little; the real cost is worst-case step time. At PoC load rent is fixed, so latency and maximum context matter more than a percentage.

**Buildability.** New engineering. vLLM [pads CUDA graph sizes](https://docs.vllm.ai/en/latest/design/cuda_graphs/) as an optimisation, and its dummy run is warm-up; a search summary says unused padding does no work. [E] An always-on loop with dummy sequences, fixed prefill and fixed transfers does not exist there. [I] It would break the unpatched-engine gate (inference_metering.md:83) and cannot apply to hosted Tinfoil. The classifier test in prior:208 is the part to keep, but it needs host captures, and a Verda tenant is not the host. [I]

## 4. Concept assessments

**a. One spend per session vs per-request hold and change**
- *Problem:* exact amounts act as a join key, and resending history makes input cost quadratic (10.5x at 20 turns). Notes confirm: median set of 2 at 1,000 simulated users, invented population. [E]
- *Per-request evidence:* no surveyed token hold; abort delivers no usage; stopped readers ran to max_tokens 38 of 39 times; relay overrun 54–73 tokens; retries link through the nullifier; one spend in flight per chain; 5,060 B client state. No vLLM or GPU. [E]
- *Does it remove the leak?* Partly. [I] Uniform `s` ends per-request amounts. `t` at close still shows session cost to whoever holds `skM`: one cent is about 1,850 tokens at $5.40/M and 20,000 at $0.50/M. Single-turn sessions keep a rounded per-request leak. Hiding `t` from the operator rests on the TEE. Stream and host signals are untouched.
- *Bill checking:* easier display, weaker proof. [I] Per-token budget needs no ledger trip, but it is the enclave's report. The client can only compare its count with one rounded `t`, and commit-reveal rounding is only statistically checkable. Its recount is itself unreliable: text re-encode mismatched 55.8%, streamed IDs were lossy. [E] A dead-stream gap looks like overcharging (inference_metering.md:162). Detection has no recourse (prior:337). Hold-and-change shares the recount problem but gives one checkable charge per turn.
- *Cost and failure:* chain locked for the session; a lost refund strands up to $5; held KV competes for one GPU; RAM loss needs a crash policy; a long circuit rides through stalls. Turns are linked by design, at least to the enclave. [I]
- *Conformance:* both options need a custom flow, so MoLE does not separate them; chunked prepayment stays nearer the profile (credentials_crypto.md:203-210). [E]
- *PoC:* applies. Best candidate to test, not a settled answer.

**b. Uniform chains, two tiers**
- *Problem:* a spend lower-bounds the balance, and varied amounts tag purchases (credentials_crypto.md:223). [E]
- *Solves:* yes for `s`, apart from a one-bit tail signal. Not for `t`. Simulated rounding raises the set (exact 2, multiple-of-256 233), but it cannot exceed active users. [E, invented population]
- *Cost:* dust, concurrency capped by chain count, no split or merge. Tiers partition a small crowd. [I]
- *PoC:* applies, cheap. Keep.

**c. Weekly cohorts, activation two weeks later**
- *Math:* best guess is 1/(1+k); 10% needs N × mean delay of at least 270 for monthly buyers (purchase_wallet_abuse.md:150-152). A 21-day mean needs N≥13. Cohorts are about N/4.3: 2 at 10 buyers, 7 at 30. [E for the formula, I for the arithmetic]
- *Chargebacks:* no. Two weeks is 12% of 120 days; the document concedes this.
- *Anonymity:* the benefit comes from the delay. [I] Buy-ahead or a client-side random delay gets it without protocol change. Batching does not hide Stripe's clock, and clients cannot verify batch size (:235). [E]
- *Cost:* credit usable 1–5.5 weeks after payment. [E]
- *PoC:* neither justification holds. Optional delay at most, no cohort claim. [I]

**d. 100 ms constant cadence vs 250 ms batching**
- *Arithmetic:* right. 10×514×8 = 41.1 kbit/s, 18.5 MB/h; 4×514×8 = 16.4 kbit/s, 7.4 MB/h. Downstream only, no TLS. [I]
- *"A few tens of kilobits":* only with compact framing. Spike payload was 24–82 kbit/s at 30–100 tok/s. [E] One cell per flush would be 123–411 kbit/s; my arithmetic, no capture. [I] Capacity is about 830 or 332 tok/s at 6 B/token, so both suffice once SSE-per-token is gone.
- *"Constant":* at the sender only. Spike: 18 stalls over 1 s in 1,322 s, longest 9.9 s, bursty delivery. [E] That hurts experience more than hiding, since stalls follow the network, not content. [I]
- *Hiding from a guard:* plausible, untested. Constant-rate sending removes the content-dependent source rate; Carlini and Nasr support this on direct TLS (via tor_traffic_analysis.md:275). No spike tested padding or a cell-level observer. [E] Stream and circuit windows stop the sender when a reader stalls, so ticks must queue or drop, and catch-up bursts appear. [I] Start, stop and session length remain, and a constant stream may itself stand out.
- *Comparator:* unpadded 250 ms batching kept r of 0.73–0.85 at 1 s, so it is no defence. [E] The fair test is constant 100 ms vs 250 ms: +50 ms vs +125 ms, against first bytes of seconds. Nothing favours 100 ms. [I]
- *PoC:* applies; test both.

**e. Separate ledger enclave vs key with GPU service**
- *Problem:* whoever gets `skM` can mint undetectably, and the engine's TCB is large. [E]
- *Solves:* software blast radius, yes. Physical custody, no: one host operator sits under both, and TEE.fail took ECDSA keys on SNP (untested on EC-ACT). [I]
- *Cost:* Verda CPU CC from $0.049/h (notes). A second attestation flow. The `skM` lifecycle stays unsolved (red_team.md §3.1).
- *New leak:* GPU-to-ledger calls recreate NCC's timing pattern inside one operator. [I]
- *PoC:* optional.

**f. Three or more providers vs Verda alone**
- *Solves:* limits one compelled provider's long-run view, not a hostile host for a given session. [I]
- *Cost:* no second CC provider researched; Tinfoil is not interchangeable. Out of PoC reach. [E, notes]
- *Honest claim:* a remote or software-only operator cannot read content without defeating attested, patched hardware, if the client enforces measurement and policy; physical access plus host control can break this undetectably; and you trust the operator's releases (tee_gpu_attestation.md:289; red_team.md §1.5, §4.1). No custody separation can be claimed. [I]

**g. Onion key and inner key in report data**
- *Evidence:* Verda documents REPORT_DATA bound to a per-boot SSH key. On Tinfoil, forged channel keys were rejected through it. Not run on Verda. [E, notes]
- *Solves:* relay of evidence to another channel, if the measurement covers the service. Verda's stock image attests the boot chain only (verda_platform.md:187). [E]
- *Does not solve:* no CPU-GPU cryptographic binding on bounce-buffer GPUs; forged quotes. [E]
- *Cost:* low. A stable onion key inherits the `skM` persistence problem. The inner key is the backend-neutral part; hosted Tinfoil has no onion key to bind. [I]
- *PoC:* applies. Strong keep.

**h. Paid cover and look-alike table**
- *Reasoning:* active × P(match) is the right form, but the numbers strain their inputs. At 5 bits, 2.9 look-alikes needs 93 of 300 wallets active in one hour; at 10 bits it is impossible. Growth (2.9, 13, 93) is sublinear with no stated reason. [I]
- *Omitted:* intersection across time (purchase fingerprints collapse after 2–3 purchases), content, amounts and two-vantage correlation.
- *Newer notes:* same direction, different units (red_team.md §1.1). Below a few hundred users, no anonymity claim holds.
- *Cover:* costs bandwidth, credit and GPU; adds noise, not buyers. [I]
- *PoC:* defer. Keep the conclusion, drop the table.

**i. Witnessed transparency log**
- *Problem:* a per-user key, context or image tags a buyer; spike T5 showed the client accepts a per-user context. [E]
- *Already there:* Sigstore and Rekor cover measurements and identity, stapled and verified offline across 36 cases; Tinfoil's config pins weights. [E] Its freshness witness is its own.
- *Adds:* issuer keys, contexts and onion addresses, plus split-view resistance. Apple logs advertised keys and reads the log through proxies. [E]
- *Cost:* no witnesses for a PoC; one-week adoption conflicts with C tor's two-week security cadence (red_team.md attack 24).
- *PoC:* log keys and contexts with the existing pattern; claim nothing from witnesses. [I]

## 5. Out-of-date or misleading statements

**Header discrepancies all stand.** The header is too cautious on proof size: L explains the whole gap. [E] EC-ACT -00 gives SpendMessage = 403 + 129n bytes, with n normally L: 1,564 B at 9, 2,467 B at 16, 4,531 B at 32. The crate is about 532 + 137L: 1,628, 2,724 and 4,919 B at 8, 16 and 32 (spike_act.md). So 2.5 KB means L≈14–16. The document's own $5-in-cents chain needs only L=9, about 1.6–1.8 KB, while per-token units at $0.50/M need L≈24, 3.5–3.8 KB. [I]

**Also stale or misleading**
- "Fork ... with one small change" (prior:365): EC-ACT adds `ctx_spend`. [E]
- "The measurements say yes" (prior:188): omits the spike's stalls. A 100,000-token upload in 1.3–3.2 s is unmeasured, as prior:365 admits.
- "Recomputes every charge and detects" (prior:337): the spike's recount results undercut this.

**Do not repeat**
- "Unlinkability must never depend on the TEE" (prior:260). True at most of credential transcripts. Amount hiding, content and shaping all depend on it, and the document's own table shows partial rows. [I]
- "Even if every enclave is broken" (prior:417). Transcript unlinkability is intended, proof pending, and requires pinned keys and contexts. A broken enclave exposes content that can identify its author. [E, I]
- "Information-theoretic ... including a quantum computer". Say "designed to be; proof not yet written".
- "Invisible on the wire" (prior:202). Untested.
- "Reveals the same lengths" (prior:208). Say "plausibly".
- "About one to two seconds" (see header).
- 20–35%, $5.40/M, $20k, the look-alike table and 0.8 attribution, as evidence.
- "Arti can host only for testing", and "removed it".

**Disagreements with later notes** (read after my verdicts)
- fact_check.md: none on overlapping facts (Whisper Leak's range, TEE.fail, Arti).
- red_team.md §7 prices the engine at $406–710/month. Do not carry that: it multiplies an unsupported percentage by fixed rent. [I]
- red_team.md adopts 10 frames/s; I treat cadence as open. It says yes to a separate CPU instance; I say optional, given the new timing channel. [I]
- red_team.md attack 16 says "very likely"; I keep "plausible" until an LLM or Blackwell measurement exists. [I]

## 6. What this should change in the report

1. Rate footing by layer: credentials promising but unproved and unaudited; attestation real, with physical and firmware limits; shaping untested; anonymity absent at PoC scale.
2. Present session-level spending as the leading hypothesis, with its `t` leak and bill-checking trade-off, against hold-and-change. Both need custom settlement.
3. List host-side GPU leakage as an open risk needing a host you control. Give no throughput figure and do not assume a fixed-shape engine.
4. Treat constant-rate shaping as a test: compare cadences, include stalls, and require compact framing.
5. Keep as low-cost concepts: key binding in report data, logged and pinned keys and contexts, uniform chains.
6. Leave weekly cohorts, paid cover, three-provider custody and witness quorums out of PoC claims. Mention them as later-scale ideas.
7. Use the single-provider wording in 4f, the "weakly anonymous" label, and the one-way rule on published counts (prior:254).
8. Fix citations per Section 2.
