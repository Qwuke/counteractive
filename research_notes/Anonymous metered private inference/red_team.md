# Red team: anonymous metered private inference

Date: 2026-11-06. Read-only review of every note in this folder plus the source conversation (`/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/chatgpt/conversation.txt`, cited below as `conversation.txt`). No account, purchase, form, browser tool or third-party code was used. No existing note was edited.

`fact_check.md` and `prior_investigation_2026_10_25.md` reached this folder after my first draft, and this version accounts for both. §7 tests the prior investigation's ideas against this PoC.

**Tags used on every statement**

- **[T]** evidence-backed by a spike test, for what that spike actually ran.
- **[D]** evidence-backed by a document or source a note read. Not tested.
- **[P]** an idea or model figure from `prior_investigation_2026_10_25.md`, cited as `prior:line`. Its header calls it "a source of ideas, not evidence", so [P] never settles a fact.
- **[I]** inference: my reasoning or my arithmetic. Arithmetic script: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/red_team_arith.py`.

Citations are `note.md:line`, relative to this folder, checked against the notes as they stood on this date. A tag applies to the sentence it ends.

---

## Bottom line

**The claim as written is false, and it fails in two different ways.** It is false at every scale because purchase is not the only identified moment and because no scale helps against physical custody or a both-ends observer. At PoC scale it also fails statistically. A narrower claim survives (see 1.5). [I]

**Findings ranked by damage**

1. **One person holds every root.** The operator holds the buyer list, the Stripe account, the Verda account, the image release, the client release, the config and the price table. A malicious release defeats confidentiality and metering together, with no hardware attack. Transparency logs give evidence afterwards, and only if someone independent is watching. [I, from `tee_gpu_attestation.md:55,109`, `tor_traffic_analysis.md:172`]
2. **`skM` has no good home.** Persistent, upgradable and hidden from the operator: pick two. Verda's documented persistence path has the operator unlock the disk after every restart. Combined with a two-week Tor security cadence and "balances do not carry over to a new context", this is the hardest unsolved design point. [I, from `tee_gpu_attestation.md:92`, `credentials_crypto.md:181`, `tor_traffic_analysis.md:80,106`]
3. **The patch check behind the surviving claim has lost its source.** AMD removed the TCB and mitigation-bit values from AMD-SB-3034 and AMD-SB-3030 on 2026-09-24 [D: `fact_check.md:93-97`]. RTX PRO on Verda is SNP-only [D: `tee_gpu_attestation.md:8`], and Fabricked and BREAKFAST forge SNP attestation from software [D: `tee_gpu_attestation.md:135-136`]. So a client has no AMD-backed way to set the floor that stops a software attacker faking a valid report. [I]
4. **Nothing verifiable has run on Verda.** The Verda digest "has never been compared with a real report" [T: `spike_attestation.md:15`]. The stock image attests the boot chain and not the service [D: `verda_platform.md:187`]. Whether Verda's host meets the client policy (TCB level, SMT off) is unknown [D: `verda_platform.md:107`].
5. **The host sits underneath the privacy layers.** Three channels survive enclave shaping: packet delay injected after the shaper, bounce-buffer DMA timing, and block I/O from settlement writes. The prior investigation names the first two and calls the first unsolved [P: `prior:208,327`]. No current track joins them, and the proposed fix for GPU timing is an unbuilt engine. [I, §2 attacks 15-17]
6. **Whole-balance-at-risk turns retries into an active probe.** Anyone who can kill a stream forces a same-nullifier re-presentation on a fresh circuit. That gives an observer next to the client a confirmation signal that volume does not dilute. No note treats this as an attack. Deadline refunds ease the money pressure but not the reconnect signal. [I, §2 attack 3]
7. **"Per token" and "private" cannot both be exact.** `s` encodes prompt size, exact `t` encodes output length, and exact amounts give k=1 at U=100 and 13 at U=10,000. Settlement has to be coarse; only the local ledger can show exact tokens. [D: `credentials_crypto.md:365-371`, `tor_traffic_analysis.md:342`; I]
8. **Metering integrity is more exposed than confidentiality.** A 3.27x inflated count passes the hash chain [T: `spike_metering.md:363`]. 0 of 108 aborted streams received usage [T: `spike_metering.md:351`]. `NoUpdate` confiscates a hidden balance "with no cryptographic recourse" [D: `credentials_crypto.md:194`]. Disputes link. A usable aggregate check does exist (§3.4).
9. **The DoS number in circulation flatters the design.** The 650 junk/s headline is for the Ristretto crate [T: `spike_act.md:13`]. The primary candidate is interpolated at 8.61 ms, about 116 junk/s per core, or about 4.2 Mbit/s per core, roughly one onion circuit's median throughput. [D: `credentials_crypto.md:467`; I]
10. **The primary crypto has never run here.** `act-rs` build was denied [T: `spike_act.md:10`]. The EC-ACT reductions "remain to be written out" [D: `credentials_crypto.md:25`].
11. **Legal and platform gates may end a public service before any attack does.** They are Verda's "competes with" and suspension clauses, NVIDIA's commercial licence, Stripe's stored-value restriction, Tinfoil Terms §3 and §10, and an unresearched illegal-content duty. [D: `verda_platform.md:254-264`, `purchase_wallet_abuse.md:35-37`, `existing_systems.md:79-81`; `purchase_wallet_abuse.md:555` for the gap]

**Are the concepts on solid footing?**

| Layer | Footing | Why |
|---|---|---|
| Blind spend with change | Sound as a concept, with live partial precedents; weak as assurance | Routstr bills real E2EE requests with blind ecash and refunds [D: `existing_systems.md:245-247`]. Ristretto mechanics worked [T: `spike_act.md:11`]. EC-ACT is unproven and unaudited [D: `credentials_crypto.md:20,25`]. |
| Attested inference | Sound on Tinfoil's fleet; unproven on Verda | 4 of 4 real AMD-signed measurements reproduced, 44 of 44 negatives diverged [T: `spike_attestation.md:14`]. Not Verda, and the default verifier cannot accept Verda hardware [D: `fact_check.md:141-143`]. The SNP patch floor has lost its AMD source [D: `fact_check.md:97`]. |
| Tor as IP hiding | Solid | All 44,100 tokens delivered, 2 of 2 five-minute streams survived [T: `spike_tor.md:9`]. Same machine at both ends, no load test [T]. |
| Tor as traffic-shape hiding | Weak | 1 s rate correlation 0.97-0.99 survives to client reads [T: `spike_tor.md:144`]. |
| Token gate and hold | Feasible on llama.cpp; vLLM untested | "vLLM was not run" [T: `spike_metering.md:7`]. "No surveyed system implements a token hold" [D: `inference_metering.md:93`]. |
| The composition | Weakest | No note tests two layers together, and most findings below sit at those joins. [I] |

---

## 1. The central claim

### 1.1 Why three notes' scale numbers cannot be read side by side

The notes use different units. `purchase_wallet_abuse.md` Table A counts monthly buyers. The `credentials_crypto.md` simulation counts users all active in one hour, with about 3 requests each ("Everyone is assumed active", `credentials_crypto.md:363`). The Tor table counts requests per hour (`tor_traffic_analysis.md:348`). [D]

The conversion below assumes 10% of monthly buyers are active in the peak hour, at 3 requests each. These are my assumptions and are not measured. [I]

| Monthly buyers | Active in hour | Req/h | First-spend best guess, 24 h mean delay (Table A) | Amount k, exact / multiple of 256 | False candidates within 1 s of a start |
|---|---|---|---|---|---|
| 1 | 0.1 | 0.3 | 1.00 | 1 / 1 | 0.0002 |
| 100 | 10 | 30 | 0.23 | about 1 / under 28 | 0.017 |
| 10,000 | 1,000 | 3,000 | under 0.01 | 2 / 233 | 1.7 |
| 1,000,000 | 100,000 | 300,000 | under 0.01 | 122 / 24,723 | 167 |

Table A and the k columns are from the notes, mapped by my assumptions. The final column applies the Tor table's formula (req/h ÷ 3,600 × 2 s). [I]

**This shifts the 10,000 row a long way.** The simulation's U=10,000 gives m256 k=2,457, but 10,000 monthly buyers gives about 233 (10% active) or about 50 (2% active, extrapolated). Readers should not quote U as a customer count. [I]

The prior investigation adds a fourth unit: look-alike wallets active in the same hour after shaping. Its model gives 2.9 chat and 0.9 agent look-alikes at 300 wallets, 13 and 3.7 at 3,000, and 93 and 27 at 30,000 [P: `prior:242-250`], from surviving features worth 5-10 bits per wallet [P: `prior:230`]. These are model estimates. They run lower than a plain `k` because they count features the credentials simulation ignores. [I]

### 1.2 Observer matrix

| Observer | Can see | Can link | Scale assumption |
|---|---|---|---|
| **Card network, Stripe** | Name, card, email, billing address or tax postcode, device characteristics, mouse activity, IP, time, amount, merchant. Fraud data is kept "as long as it's useful". 3DS2 device data goes to the card issuer. [D: `purchase_wallet_abuse.md:43-53`] | Person to "customer of this merchant", at every purchase. Top-up cadence gives consumption over time at denomination resolution. [I] Not to spends, without the operator's logs. [I] | None. Identified at every scale. The merchant's nature is itself sensitive. [I] |
| **Issuer, operator** | Buyer list, kept at least 120 days for disputes and 10 years under EU OSS where it applies [D: `purchase_wallet_abuse.md:309,438`]. `k`, `s`, `a`, `t` per spend if `skM` is outside the enclave [D: `credentials_crypto.md:157-161`]. | Purchase to first spend by timing: best guess 1/(1+k) [D, own model: `purchase_wallet_abuse.md:150`]. Request to request by exact amount and rhythm. [I] | Depends on N, delay and denominations. See 1.3. |
| **Operator with a no-admin measured image, gate and store inside** | Buyer list, plus whatever the image emits. The tenant has no host-level packet view. [I] | Much less than the notes assume. `purchase_wallet_abuse.md:223` says the operator holds both logs, which is true of a gateway outside the enclave. Here the arrival log belongs to Verda. Linking then needs operator plus Verda, for instance one legal order to each. [I] | Skeptical of the notes' pessimism. Only holds for an image that exists and is verified, which is untested. [I] |
| **Verda, host** | Every CVM packet [D: `tor_traffic_analysis.md:320`]. Bounce-buffer activity [D: `verda_browser_notes.md:69`], with plaintext RPC metadata and bimodal transfer timing on H100 [D: `tee_gpu_attestation.md:147-152`]. Block I/O, serial console [D: `verda_platform.md:231`]. Physical custody. Logs "typically up to 12 months" [D: `verda_platform.md:224`]; scope unknown. | Request start, true end, token rate, each unmixed at low concurrency. It can train classifiers as a customer [D: `tor_traffic_analysis.md:320`]. No client IP. | Batching helps at volume (untested). [I] Physical attacks are scale-independent. [D: `tee_gpu_attestation.md:126-127`] |
| **Verda, network path** | Tor cells to relays, in 514-byte units with timing. [D: `tor_traffic_analysis.md:320`] | Same shape data, without host internals. Can delay or drop. [I] | As above. |
| **NVIDIA, AMD, Sigstore** | With a stapled document and the SDK: nothing from the client, tested on Tinfoil: 8 of 8 SDK runs with only loopback allowed, and the process contacted only the enclave in 6 of 6 [T: `spike_attestation.md:18,355`]. The CLI bypasses SOCKS through a direct `tls.Dial` [T: `spike_attestation.md:196`]. macOS `trustd` OCSP is unobservable [T: `spike_attestation.md:213`]. Verda's commands contact third parties [D: `tee_gpu_attestation.md:249`]. NVIDIA collects CVM IP and GPU data [D: `verda_platform.md:263-264`]. | A client-side fetch reveals "uses this service" and a chip ID to that vendor. [I] Server-side fetches expose only the operator. [I] | None. Implementation discipline. Real Tor was not tested for attestation. [T: `spike_attestation.md:359`] |
| **Tor guard, ISP** | Tor use, 514-byte multiples, timing. A steady stream with near-zero upload is distinctive [D: `tor_traffic_analysis.md:318`]. The sequence 2.3 KB up, 83-219 KB down, about 5 KB up, then a long stream is a service-class shape. [T for sizes: `spike_attestation.md:198`; I for shape] The ISP also sees Stripe and the issuer domain at purchase. [D: `purchase_wallet_abuse.md:277`] | One ISP holds "bought at T" and "uses this Tor shape", and can infer customer status with no operator help. Topic inference through Tor is extrapolated from non-Tor data; the time-only input should be about 46-85% per model, and no Tor test exists. [D: `tor_traffic_analysis.md:312`, corrected by `fact_check.md:228-229`] | Guard-side shape does not improve with service users. [I] |
| **Global passive adversary** | Both ends. Outside Tor's goals. [D: `tor_traffic_analysis.md:319`] | Start-time matching alone below about 1,800 req/h per visible shape class. [I, from `tor_traffic_analysis.md:346-353`] Rate correlation otherwise. [T: `spike_tor.md:144`, client side only] It also works after the fact wherever ISP and cloud flow records survive; the prior investigation rated that its one critical attack [P: `prior:321,409`]. Verda's flow logging is unknown [D: `verda_platform.md:237`]. | See 1.3. Realistic for a state or targeted investigation, not for a lone operator. [D: `tor_traffic_analysis.md:358`] |
| **Tinfoil as backend** | Under a pooled key: per-request timestamp, model, tokens, enclave and request ID, gateway IP [D: `existing_systems.md:49,53,93`]. | Usage to operator's key, not to a user. Anonymity from Tinfoil equals crowd size behind the key. [D: `existing_systems.md:93`] Tinfoil plus operator can join by time and count. [D] | At 1 user, Tinfoil has that user's complete timing and usage. [I] Terms read against it. [D: `existing_systems.md:96`] |
| **Other users** | Queue and TTFT interference on one GPU. [I] A shared cache if on: hit timing leaks between users under a shared API identity [D: `existing_systems.md:57`]. | Whether someone else is active, and roughly how long they generate. [I] Sybil traffic can shrink a batch to n-1. [D: `purchase_wallet_abuse.md:136`] | Worst at N=1 and low concurrency. Not measured by any note. [I] |
| **Not on the brief: device and distribution** | A seized wallet, journal and receipts. Whoever ships the client or update. | All spends, retroactively, when joined to the permanent nullifier store (§2 attack 18). A buyer-targeted build is possible where the operator controls the download. [I] | None. Scale does not help the individual. [I] |

### 1.3 What is honest at each scale

| Statement | 1 user | 100 | 10,000 | 1,000,000 |
|---|---|---|---|---|
| A spend has no cryptographic link to purchase | Vacuous | Design claim; proof unwritten | Same | Same [D: `credentials_crypto.md:25`] |
| Tor hides client IP from operator, Verda, backend | Yes | Yes | Yes | Yes [D: `tor_traffic_analysis.md:370`] |
| Remote or software-only parties cannot read prompts | Conditional | Conditional | Conditional | Conditional: patched TCB, enforced policy, no-admin image [D: `tee_gpu_attestation.md:289`]. The client cannot currently derive the patch floor from AMD [D: `fact_check.md:97`] |
| Physical custody cannot read prompts | False | False | False | False [D: `tee_gpu_attestation.md:126-127,144`] |
| Operator alone cannot tell which buyer made a request | False | False for buy-and-use (0.88 at 1 h). Weak with reserve (0.04 at 7 d) | True with reserve, a rounded or hidden amount, one config. False for 5-minute use (0.46) | True under the same conditions [D: `purchase_wallet_abuse.md:156-163`; I for conditions] |
| Operator alone cannot group one person's requests | False | False | Conditional across conversations; false within one | Same [I] |
| A both-ends observer cannot link | False | False | False across several turns if any shape survives | Single requests ambiguous (about 167 candidates); long linked sessions still at risk. Never claim it [I] |
| Host cannot infer length and timing | False | False | Partly, if batching mixes (untested) | Partly [I] |
| You cannot be targeted without attacking the whole system | False | False | False | Only with a large fleet, random selection and a log. No provider evidence [D: `existing_systems.md:151,372`; `verda_platform.md:55`] |
| Purchase is the only identified moment | False | False | False | False: top-ups, refunds, disputes, EU withdrawal, VAT records [D: `purchase_wallet_abuse.md:373-384,469-471`] |

Notes on the extremes:

- **1 user:** the only meaningful claims are IP hiding and qualified content confidentiality. The tor track says this plainly: "Requests from a lone active user are trivially 'the same client'". [D: `tor_traffic_analysis.md:370`]
- **100 users:** the skeptic's point runs both ways. A week-long reserve gives best guess 0.04 and effective set 63, better than the notes' tone suggests. [D: Table A] But that figure only covers purchase-to-first-spend linking, and a first-time buyer has no reserve. [I]
- **Heavy and agent users** do worse than these rows at every size. The prior investigation's model has a 20x-usage user attributable with probability around 0.8 at 300 wallets [P: `prior:252`], and the purchase server knows who buys the most. [I]
- **1,000,000 users:** this is a third of Tor's estimated 3.0-3.18M daily direct users [D: `tor_traffic_analysis.md:373`], which makes it a thought experiment. It requires multi-CVM, while multi-node CC is on Verda's roadmap and capacity is one site [D: `verda_platform.md:49,55`]. PoW "did not work with Onionbalance as of July 2025" [D: `purchase_wallet_abuse.md:520`]. Every verifier needs `skM` and one atomic store [D: `credentials_crypto.md:173,222`], about 1.7 GB/day at 240 B per spend. [I]

### 1.4 Prior art agrees

Trail of Bits reported issuance-timing correlation (TOB-WAPI-4, unresolved), and Meta's answer rests on "complete rollout and a steady-state QPS" [D: `purchase_wallet_abuse.md:109-111`]. LLM-Tor says it "does not provide absolute anonymity" [D: `existing_systems.md:204`]. OpenPCC requires "a well-behaved client wallet ... and sufficient OpenPCC network traffic" [D: `existing_systems.md:173`]. The source conversation itself said "I still wouldn't call it completely anonymous" [D: `conversation.txt:388`]. The prior investigation's Figure 1 repeats the claim ("the only identified step is the purchase") [P: `prior:82`], while its own Table 3 marks operator-plus-network linking as failing and operator session linking as partial [P: `prior:285,289`]. Nobody with a shipped system makes this claim. [I]

### 1.5 Strongest honest wording

> When you buy credits, Stripe, your card issuer and the operator know who you are, when you bought and how much, and they keep those records. Refunds and disputes identify you again.
>
> After purchase, the credit protocol is designed so that a spend has no cryptographic link to your purchase or your other spends. That design has no published proof or audit yet. Tor hides your IP address from the operator, the datacentre and the model provider. Your prompts are encrypted to published, attested code, and you can check what code ran. Nobody can read them remotely or through software unless they exploit a flaw in that code or in the processor's firmware, and today that check cannot confirm every known firmware flaw is patched.
>
> This does not protect you from anyone watching both your connection and the server, or from anyone with physical access to the server. It does not hide your usage pattern from the datacentre. It does not protect you from the operator releasing a bad version before someone notices. Until many people use the service at the same time, the operator may be able to guess which customer you are from timing and amounts. You pay in rounded amounts; your device shows the exact tokens.

[I, built on `tee_gpu_attestation.md:289` and `purchase_wallet_abuse.md:14-15`]

At fewer than a few hundred active users, add: "Today, treat this as content privacy and IP hiding. It is not anonymity." [I] The prior investigation's label, "confidential; weakly anonymous beta", says the same, and its rule is worth keeping: operator-published counts may lengthen a delay or add a warning but never remove one, because the operator can inflate any count [P: `prior:254`].

---

## 2. Composition attacks

Severity is for the person being linked: **PoC** means up to about 100 users, and **10k+** means 10,000 or more monthly buyers. "Moot" means an earlier, easier signal already links at that scale. All ratings are my judgement. [I]

| # | Attack | PoC | 10k+ | Known mitigation | Cost |
|---|---|---|---|---|---|
| 1 | Exact `s` and `t` reveal what shaping hides | High, moot | Critical if exact; Medium rounded | Deterministic multiple-of-256 grid, or amounts visible only in the enclave | 7.6% mean overcharge on synthetic data, saves 1,032 B of proof |
| 2 | Hold encodes prompt size | Medium | High | `s` only from a short published list, or fixed tiers with one spend per session [P: `prior:146,162`] | Locked balance, stranded dust; step changes mark long conversations unless spends are per session |
| 3 | Lost refund and retry, including forced retry | Low, moot | High for targeted observer | Delayed, jittered recovery; long retention; smaller chains; deadline refund fetched later [P: `prior:152`] | Balance "pending" for minutes or more; 208 B per spend during retention; refund inputs must persist [P: `prior:339`] |
| 4 | Wallet split and concurrency | Medium | Medium | Chains only from separate purchases; no split endpoint | Purchase must cover N × ceiling |
| 5 | History growth across turns | High | Medium to High | Upload buckets, plus conceding conversation-level linkage; or a transcript held in the enclave, sending only new text [P: `prior:188`] | 256 KiB is about 526 cells; billing grows quadratically; a held transcript takes GPU memory, with long contexts fitting "one to four per server" on its H100 sizing [P: `prior:367`] |
| 6 | Spend proof and first-request size | Medium | Medium | Minimum bucket 8 KiB [P: `prior:202`] or 16 KiB, or L up to 20 with rounding | About 17-33 cells per request |
| 7 | Per-key or per-context partition | Critical | Critical unmitigated; Medium mitigated | Pin `pkM`, `ctx`, `L`, ceilings, prices in client and measured config; log cosigned by independent witnesses [P: `prior:266`] | Rotation requires a release; no witness or monitor exists at PoC |
| 8 | Attestation fetch pattern | Medium, moot | Medium | Staple evidence; pad; same flow as the spend | 83-219 KB per verification |
| 8b | CLI verification bypassing SOCKS | Critical if used | Critical if used | SDK path only | None |
| 9 | Model choice | Low, one model | High beyond 2 or 3 models | Short list, flat unit price, router enclave | Prices stop tracking cost; another CVM at $2,029/month per GPU model |
| 10 | Change-credential timing | Medium, moot | Medium | Client delay; no pre-warm at stream end | Conversational latency |
| 11 | Client version differences | Medium | Medium | One profile, forced upgrades | Old wallets may strand balances |
| 12 | One machine, one onion address | High | High if still N=1 | Fleet with random selection and log | Multiples of $2,029/month; multi-node not available |
| 13 | Circuit reuse against 4 s | Low value | High if reuse is on | See below | 2.96 s median; 7-9% failures |
| 14 | DoS defences | High | High | Tor PoW, stream limits, cheap pre-admission token | Extra token layer and PoW-capable client |
| 15 | **Host delays packets after the shaper** | Low, moot | High for targeted observer | Only a mixnet resists it [P: `prior:327`] | About 10 s per turn and no streaming [P: `prior:327`] |
| 16 | **Bounce-buffer DMA timing** | High vs Verda | Medium (batching, untested) | Fixed-shape engine with dummy sequences [P: `prior:208`] | Estimated 20-35% of throughput [P: `prior:208`]; unbuilt and unmeasured |
| 17 | **Settlement write timing** | High vs Verda | Medium | Fixed-tick group commit, constant write size, dummy inserts [P: `prior:264`] | Up to one tick of latency |
| 18 | **Wallet ledger joined to nullifier store** | High | High | Erase `k` and receipts after commit; aggregate ledger; no times or amounts in store | Weaker reconciliation and dispute |
| 19 | **Clearnet issuance into the CVM** | High | Medium | Collect issuance over the onion; enclave polls Stripe outbound | Stripe key in TCB |
| 20 | **Top-up cadence and first purchase** | High | Medium | Reserve wallet, fixed denominations, calendar purchases; forced delay for a first purchase [P: `prior:218,224`] | Float at risk; credit usable 1 to 5.5 weeks after payment [P: `prior:228`] |
| 21 | **Pooled Tinfoil key** | High | High | Fresh cache secret per request; written permission | No cache discount |
| 22 | **Refund, withdrawal, dispute** | Medium | Medium | No refunds, or delayed burn voucher | Consumer-law exposure |
| 23 | **Model output rendered by client** | High | High | No remote fetches, previews or auto-links; treat output as hostile | Trivial; must be tested |
| 24 | **Log adoption delay against security patches** | Low (no log yet) | Medium | Emergency path with witness sign-off | The emergency path is also the fast route for a targeted image |

### Notes on individual attacks

**1. Exact amounts.** The conflict is known [D: `tor_traffic_analysis.md:342`]. k=1 at U=100 and 13 at U=10,000 for exact, against 28 and 2,457 at multiple-of-256 [D: `credentials_crypto.md:367,371`]. Moving `skM` into the enclave only helps if the store record is encrypted too, because the 240 B row contains the refund and `t` travels in it [T: `spike_act.md:113`; I]. An encrypted store has its own problem (attack 17, and rollback in §3.2).

**2. Hold encodes prompt.** The spike's rule is hold = prompt + clamped `max_tokens` [T: `spike_metering.md:239`], and `s` is public to the `skM` holder. Used naively, upload buckets at the Tor layer are undone by the credit layer. Publishing a ceiling list fixes the leak [D: `credentials_crypto.md:223`], but a long conversation still shows step-ups in `s` that only go one way. `inference_metering.md:204` notes the growth, and no current track sizes the fix. One spend per session at fixed tiers removes the leak by moving the hold into the enclave [P: `prior:138,146`]. That changes the spike's gate rules from per-request to per-session, so they need re-testing. [I]

**3. Retry, passive and forced.**

- Passive: a byte-identical retry links only to its own attempt and reveals a retry happened [T: `spike_act.md:119`]. A fresh-proof retry exposes a second `s` if different [T: `spike_act.md:120`]. The notes are right that this alone is minor.
- Forced, which no note treats as an attack: every spend puts the whole hidden balance at risk [D: `credentials_crypto.md:194-195`]. So the wallet must re-present. An adversary at the service, or Verda dropping packets, kills a chosen stream at time T. An observer next to the client then sees the circuit die and a new circuit carrying about 5 KB shortly after. That is a confirmation signal the attacker chooses, and it does not depend on request volume, which is what the base-rate defence relies on. [I]
- Natural failures make this hard to avoid: 18 stalls over 1 s in 1,322 stream-seconds and 3 of 42 fresh-circuit failures mean the client must regularly decide whether a stream is dead. [T: `spike_tor.md:94,228`] Aggressive recovery is easy to probe. Slow recovery strands the balance. [I]
- The prior investigation's deadline refunds, sealed and fetched later, remove the money pressure [P: `prior:152`]. Its resume step, where the wallet "reconnects on a fresh circuit" with a session token, recreates the same probe unless the wallet also waits. [I]
- A retention window shorter than a Tor outage means permanent loss [D: `credentials_crypto.md:215`]. RAM with a short TTL [D: `inference_metering.md:115`] loses whole chains on a reboot, not just the hold. [I]

**4. Split.** No split or merge primitive exists; a bespoke split shows `s` and issuances close in time [T: `spike_act.md:148,150`]. The prior investigation gets concurrency from four uniform chains per voucher, which avoids a visible split at the cost of a fixed $20 purchase [P: `prior:146,150`]. An `s=0` transfer "may itself be a visible, distinctive event" [D: `purchase_wallet_abuse.md:621`]. The serialized chain also forces non-overlap, which is a structural fact about each chain. [D: `credentials_crypto.md:189`; I]

**5. History growth.** Stateless multi-turn resends everything, so sizes rise within a conversation: 42,000 against 4,000 input tokens over 20 turns [D: `inference_metering.md:204`]. Inside the enclave the plaintext history links turns outright. I would stop treating turn-level unlinkability as a goal and promise conversation-level unlinkability only. Amount-based re-identification is weak (1.6% top-1 after 100 linked amounts) [D: `credentials_crypto.md:398`], so conceding this costs less than it sounds against the operator. [I] Text is different: one unchecked preprint re-identified at least one conversation for 50-75% of heavy users, given labelled history [P: `prior:260`]. That matters to anyone who breaks the enclave. [I]

**6. Proof size.** The Tor track's 4 KiB bucket does not fit. EC-ACT is 4,531 B at L=32 and the crate's wire form is 4,919 B [D: `credentials_crypto.md:459-463`; T: `spike_act.md:112`]. L must be at most 28 to fit at all, or 20 to leave 1 KiB for headers and prompt. [I] Rounding helps directly here: a grid of 256 removes 8 bits. [D: `credentials_crypto.md:405`] That also explains the prior investigation's "about 2.5 KB": the `-01` formula gives 2,496 B at L=16 [I; `prior:130`; formula at `purchase_wallet_abuse.md:572`].

**7. Partition.** The client accepted a per-user `ctx` and carried it in the clear across refunds [T: `spike_act.md:161,275`]. A per-user key is identifiable by trial verification [T: `spike_act.md:162`]. Issuance tagging is "the critical case" [D: `credentials_crypto.md:339`]. The mitigation is well known (TOB-WAPI-1, NCC; `purchase_wallet_abuse.md:118-120,267`). The weak part is detection: a log with no independent watcher catches nothing at PoC scale, and the operator also ships the client that checks it. [I] Overlapping keys let targeted clients be "identified by their use of older but still valid configurations" [D: `purchase_wallet_abuse.md:119`].

**8. Attestation.** Fresh liveness needs a nonce per session, because a captured document stays valid until its witness expires [T: `spike_attestation.md:182`]. So a nonce fetch next to a spend marks a session start. [I] The CLI failure is a direct IP leak [T: `spike_attestation.md:196`], and `existing_systems.md:319,325` currently suggests that CLI for its V1 and V2. See §5.

**9. Model choice.** Rare models and differing prices split the set [D: `inference_metering.md:231`]. RTX PRO's single-GPU mode means per-model CVMs, so the destination names the model [D: `inference_metering.md:230`]. The product requirement "more models later" directly shrinks the anonymity set. On Tinfoil the trailer shows `model=` in plaintext to the gateway [D: `existing_systems.md:40`].

**12. One machine.** "An attacker who compromises the node gets all traffic in that window" [D: `existing_systems.md:151`]. Non-targetability should be out of scope. Keeping a stable onion address needs sealed storage or attested key release [D: `tor_traffic_analysis.md:172`], which is the same unsolved problem as `skM` (§3.1). [I]

**13. Isolation: the notes are too absolute at small scale, and right at large scale.** `tor_traffic_analysis.md:176` calls per-request isolation "non-negotiable".

- Cost: 4.33 s against 1.37 s first byte; 3 of 42 failures plus one 120 s hang. [T: `spike_tor.md:11,109-116`] A 20-turn conversation has about a 77% chance of at least one failure. [I]
- Value below about 1,800 req/h per visible shape class: start-time matching already works and history growth links turns, so against a both-ends observer it buys almost nothing. It still denies a lone operator a session ID, which has some value. [I]
- Value above that: once false candidates per turn exceed 1, unlinked flows are a real barrier, because tuples multiply. A persistent circuit hands the adversary the link, and the false matches collapse. [I, from `tor_traffic_analysis.md:348-353`]
- Reading: default to per-conversation isolation at PoC, say so, and switch per request when volume makes it worth paying for. [I] The prior investigation reaches the same unit from the other side: one spend, one circuit, one session [P: `prior:136-138`].
- Caveat: the spike ran during a live Tor incident, so its costs may not be typical [D: `fact_check.md:212`].

**14. DoS.**

- Rejection costs: EC-ACT interpolated 8.61 ms [D: `credentials_crypto.md:467`], Ristretto 1.53 ms [T: `spike_act.md:195`], VOPRF 36 us, blind RSA 20 us [T: `spike_act.md:219,221`]. Generating junk costs about 0.4 us [T: `spike_act.md:13`].
- On 30 vCPUs that means 63-126 Mbit/s of junk saturates verification, about 12-42 onion circuits at OnionPerf's 3-5.3 Mbit/s medians. [I] BoringSSL doubles this. [D: `credentials_crypto.md:467`; I]
- Tor PoW protects introduction, not streams after connection; it is off by default [D: `purchase_wallet_abuse.md:520`]. Arti's client support conflicts across sources [D: `purchase_wallet_abuse.md:521`]. Per-request isolation multiplies a legitimate client's PoW cost under attack. [I]
- A batch of cheap admission tokens issued at purchase rejects junk without identity. It adds a second token type and its own partition. [I] LLM-Tor's abuse pseudonym re-links everything and must stay out [D: `existing_systems.md:202`].
- Per-user bans are not available in this design [D: `purchase_wallet_abuse.md:538-544`]. A flood affects everyone.

**15. Delay after the shaper.** The fixed tick runs inside the enclave [D: `tor_traffic_analysis.md:338`], and the host forwards its packets. The host can delay them in a pattern. Coarse rate survives to client reads at r=0.97-0.99 [T: `spike_tor.md:144`, for rate variation at the source]. So shaping defeats a passive observer, but not an active host paired with a client-side observer. Injected-watermark survival was not tested. The prior investigation names host watermarking as unsolved [P: `prior:327`], and no current track covers it. [I]

**16. DMA timing.** The tor track left this open: "Whether Verda's host can observe GPU activity timing is unresolved" [D: `tor_traffic_analysis.md:365`]. The TEE track holds the answer's direction without connecting it: bounce-buffer transfers are the only mode here [D: `tee_gpu_attestation.md:8`], RPC metadata is plaintext and transfer timing is bimodal on H100 [D: `tee_gpu_attestation.md:149-150`], and Tinfoil lists "I/O pattern leakage" as unprotected [D: `tee_gpu_attestation.md:154`]. So the host very likely sees batches of token activity beneath Tor and shaping. Not tested on Blackwell, and no paper measures it. [I] The prior investigation assumes the same and answers with a fixed-shape engine: admission on 250 ms ticks, fixed batches padded with dummy sequences, fixed transfers, at an estimated 20-35% of throughput, accepted only if a host-side classifier does no better than chance [P: `prior:208`]. That is the right test. It is also a custom engine on unmeasured SNP overhead, which cuts against "maintainable". [I]

**17. Settlement writes.** The durable record is committed at admission and at settlement [D: `credentials_crypto.md:205-207`]. If the store is on a Verda volume or remote, the host sees a write at true stream end, even when padded duration buckets hide it on the network. Padding is therefore undone by the crash-safety requirement unless commits happen on a fixed tick. No current track covers this, though the prior investigation's ledger pads inserts at a fixed rate [P: `prior:264`]. It is cheap to fix. [I]

**18. Ledger.** Reconciliation needs per-request history [D: `purchase_wallet_abuse.md:658-666`], and receipts are kept on the client [D: `inference_metering.md:160`]. Nullifiers must stay server-side for the life of the key [D: `credentials_crypto.md:173`]. A seized device plus that store rebuilds a user's request history with times and amounts, retroactively. Nothing in the protocol protects against it. "Show and reconcile budget" works against "unlinkable", and no current track joins them. Stripping timestamps from the store helps on the server side [P: `prior:321`], but the device ledger remains. [I]

**19. Issuance path.** Stripe webhooks need inbound HTTPS [D: `purchase_wallet_abuse.md:42`], while `verda_platform.md:229` wants all inbound blocked. If issuance reaches `skM` in the CVM over clearnet, Verda sees buyer IPs and timing next to Tor traffic. [I] If a front end tells the enclave "paid", then the front end can mint, so `skM` in an enclave does not stop operator or front-end minting. [I] The research brief says credentials are stored "over HTTP/DNS"; no note defines that phrase, and text search finds no treatment of it. Read it as a clearnet path, and treat it as exposed. [D for absence; I] The prior investigation buys over a separate Tor state [P: `prior:94,409`], but Stripe Radar exposes `:is_anonymous_ip:` to merchant rules, and purchase over Tor is untested [D: `tor_traffic_analysis.md:195`; `spike_tor.md:169`].

**20. First purchase and top-ups.** "Buy ahead" is the cheapest strong lever [D: `purchase_wallet_abuse.md:227`], but it cannot apply to a first purchase, which happens because the person wants to use the service now. [I] A forced delay can: the prior investigation activates week W purchases from week W+2, so credit is usable 1 to 5.5 weeks after payment [P: `prior:218,228`]. The delay does the work there, not the batch: batching "hides issuance time but not the Stripe payment time", and clients cannot verify batch size [D: `purchase_wallet_abuse.md:235`]. At 100 monthly buyers a weekly cohort is about 23 people. [I] Reserve also raises exposure: wallet loss is money loss, context retirement forfeits balance, and chargeback float grows [D: `credentials_crypto.md:219-220`; I]. A small ceiling near empty followed by a purchase links old and new credentials [D: `purchase_wallet_abuse.md:241`].

**21. Pooled Tinfoil key.** Without per-user secrets, "requests under the same API identity share one cache", and hit timing can leak between users. No off switch is described [D: `existing_systems.md:57`]. A fresh secret per request prevents hits [D], so the fix exists and costs the discount.

**23. Model output.** A compromised enclave can return anything. A client that fetches a remote image or previews a link defeats the IP hiding with one response. The prior investigation lists the agent form of this, where tools run model output from the user's own IP [P: `prior:335`]. No note lists plain rendering. It may be the cheapest deanonymization available after a TEE break. [I]

**24. Adoption delay against patches.** The prior investigation has wallets adopt new server software only after a week in the log, which blocks a targeted image [P: `prior:266`]. Tor inside the image now ships security releases about every two weeks, with an emergency release on 4 November [D: `tor_traffic_analysis.md:80,106`; `fact_check.md:198`]. So either the service runs known-vulnerable Tor for a week after each release, or wallets accept an emergency path that a targeted image could also use. No note raises this. [I]

---

## 3. Trust root

### 3.1 Where can `skM` live?

`skM` both verifies and mints: "A Moderator that shares `skM` with another party lets that party issue Credentials in its name" [D: `credentials_crypto.md:177`]. An enclave verifier "must hold `skM`" and needs a shared durable store outside the enclave [D: `credentials_crypto.md:222`]. The notes stop there.

| Placement | Survives reboot or eviction | Survives image update | Hidden from operator | Evidence |
|---|---|---|---|---|
| RAM in GPU CVM, new per boot | No. All balances void; spot unusable | No | Yes, barring a malicious release | [I]; `verda_platform.md:91` for no in-guest reboot [D] |
| LUKS disk, operator unlocks after attestation | Yes | Yes | **No**: the operator holds the key | The documented Verda path [D: `tee_gpu_attestation.md:92`]; "contradict the promise in production" [D: `tee_gpu_attestation.md:109`] |
| Chip-derived sealing bound to measurement | Same chip only | No, needs migration | Yes | [I, from AMD SNP background; ABI not read by any note] |
| Chip-derived sealing without measurement | Same chip only | Yes | **No**: any operator-built image on that chip can derive it | [I, same caveat] |
| Handoff from old enclave to next logged image | Needs a live peer | Yes | Until the operator releases a bad image; visible only to log watchers | [I] |
| Separate CPU CC instance ($36/month) | Same persistence problem | Same | Same | [D: `verda_platform.md:44`; I]. A gate compromise then cannot steal `skM` or mint without limit, and the issuer cannot see prompts. Rome/Milan label on those SKUs unresolved [D: `verda_platform.md:68`] |
| Operator-held | Yes | Yes | **No**; sees exact net charge | [D: `credentials_crypto.md:48`] |
| Publicly verifiable denominations (blind RSA), signer apart from verifier | Yes | Yes | Signer sees denominations only | Gate cannot mint. [I] Reject at 20 us [T: `spike_act`] |

**Reading.** No option gives all three properties on evidence in these notes. The two-week C tor security cadence [D: `tor_traffic_analysis.md:80`] means a new measurement every two weeks, so the "survives update" column is not a corner case. "Balances do not carry over to a new context" and "this document does not specify balance migration" [D: `credentials_crypto.md:181`], so rotating the key is not an escape. Spot is ruled out for any persistent key: it "may be evicted without warning" [D: `verda_browser_notes.md:26`]. [I] The prior investigation's separate ledger enclave with a synchronous replica at another provider gives reboot survival through handoff [P: `prior:94,264`]. It does not say how `skM` reaches a new ledger image, and other providers were not researched for this PoC [D: `verda_platform.md:364`]. Verda has one CC site [D: `verda_browser_notes.md:31`]. [I]

**The fallback deserves a second look.** `credentials_crypto.md` treats EC-ACT as primary and denominations as fallback. Rounding was needed anyway, so the fallback's main drawback costs less than presented. Denominations separate verifier from signer (RSA only; VOPRF does not), reject junk about 400x cheaper than interpolated EC-ACT, and put only the spent tokens at risk rather than the whole balance, which removes the forced-retry pressure in attack 3. It costs more client and server bandwidth and change handling, where OpenPCC needs exchange and delay [D: `existing_systems.md:171`]. A blind-RSA issuer also has to re-run signing per denomination, at 30 ms for 11 [D: `credentials_crypto.md:291`]. I would not assume EC-ACT wins before comparing these. [I]

### 3.2 If the enclave is compromised

| Attacker | Mint money? | Read prompts? | Deanonymize? |
|---|---|---|---|
| Remote code execution in gate or engine, no hardware break | Yes if in the same CVM as `skM`. [I] Silent: forged credentials have fresh nullifiers, which "the nullifier store cannot detect" [D: `credentials_crypto.md:123`, for the related `e` reuse case; I for key theft] | Yes, everything in plaintext from then on. Past traffic stays protected: Tor circuit keys are ephemeral in the CVM, and nothing is stored if no history is retained. [I] | No retroactive link. Blinding factors are client-only and the IP was never there. [D: `credentials_crypto.md:159`; I] Forward: see below |
| SNP firmware forge (Fabricked, BREAKFAST), before patch | Yes via forged attestation, a malicious image and debug [D: `tee_gpu_attestation.md:135-136`] | Yes | As above. The notes' fix was a TCB floor [D: `tee_gpu_attestation.md:161-162`], but AMD has withdrawn the values, and how a client confirms the fix from a report is open [D: `fact_check.md:93-97`]. Verda's host level is also unknown [D: `verda_platform.md:107`] |
| Physical interposer on DDR5, with root | Plausibly. TEE.fail recovered an OpenSSL ECDSA key from SNP with Ciphertext Hiding, without VCEK extraction [D: `tee_gpu_attestation.md:126`]. EC-ACT uses long-lived scalars on P-256, and "a repeated nonce ... reveals `skM`" [D: `credentials_crypto.md:123`]. Not demonstrated on EC-ACT [I] | Yes, on a real CVM with a valid report: an interposer attack "leaves nothing in the attestation report" [D: `tee_gpu_attestation.md:191`]. CPU DIMMs hold the plaintext whatever the GPU's protection [D: `tee_gpu_attestation.md:157`] | As above |
| Operator ships a bad image | Yes | Yes | Yes for future requests, by the forward paths below |

**Forward deanonymization after any of these** needs no cryptographic break. [I]

- Exact `s`, `t`, time per nullifier (the enclave's privacy benefit disappears from that moment).
- Tagging with `t` or per-client config.
- Content that self-identifies, or that matches labelled history [P: `prior:260`].
- Active watermarking with a client-side partner.
- A hostile response that makes the client fetch something (attack 23).

So `tee_gpu_attestation.md:295` ("less dependent on the TEE") is correct for IP and the cryptographic link, and overstated for amounts and timing. [I]

**Physical attack caveats.** On the SNP path that RTX PRO uses, the evidence supports secret extraction and does not support attestation forgery: DDRop shows forgery on TDX and only a relocation primitive on SNP, in a dual-socket evaluation whose limits "do not inherently preclude" single-socket machines [D: `tee_gpu_attestation.md:127,141`; `fact_check.md:81`]. AMD "does not plan to assign a CVE or release mitigations" [D: `fact_check.md:82`]. That still favours SNP on this point [D: `tee_gpu_attestation.md:158-161`], although `tee_gpu_attestation.md:161` leans on the floor AMD has since withdrawn. [I] RTX PRO's GDDR7 is off-package, and the HBM argument does not cover it [D: `tee_gpu_attestation.md:163`]. Verda's FAQ claim about bus probing is overbroad [D: `verda_platform.md:353`].

**A separate integrity hole: rollback.** The store sits outside the enclave, and TEEs here have no memory freshness [D: `tee_gpu_attestation.md:120`], with no documented monotonic counter or vTPM [D: `verda_platform.md:94`]. A host rewinding the volume re-admits spent credentials, and the enclave cannot tell. "A Moderator that loses it re-admits every" spent credential [D: `credentials_crypto.md:173`]. A zero prepaid balance deletes volumes after 96 hours [D: `verda_platform.md:50`], so a billing lapse becomes the same failure. No note makes this billing link. [I] The prior investigation's answer is a synchronous replica at a different provider plus state roots in the log [P: `prior:264`]. A replica on Verda alone shares the adversary. [I]

**Mint impact differs by backend.** On one self-hosted GPU, the cost is displaced capacity bounded by a fixed $2.78/hour. That is milder than the notes imply. On Tinfoil, each minted token is real upstream cash, and limits are soft and "not a reservation system" [D: `existing_systems.md:56`], so minting becomes a direct money drain. Detection is aggregate only: total sold against total spent [D: `purchase_wallet_abuse.md:679`], and float masks it. [I]

### 3.3 Is integrity or confidentiality more exposed?

**Metering integrity is more exposed. Confidentiality has the higher stakes.** [I]

| | Metering integrity | Confidentiality |
|---|---|---|
| Needs a TEE break or malicious image? | No, several paths work at the protocol or engine layer | Yes |
| Client can detect? | Partly, and only in aggregate | Not after a valid report |
| Paths | Biased or inflated counts, chosen `t`, `NoUpdate`, dropped refunds, aborted-stream tail, unsigned Tinfoil trailer, store rollback, minting, stochastic rounding bias | Malicious image, unpatched SNP, physical access |
| Reversible? | Money only, capped by purchase size | Irreversible |
| Who benefits? | The operator, directly | Few, and it is a targeted or legal event |

Evidence for the left column: 3.27x inflation with identical text passes the chain [T: `spike_metering.md:302,363`]; aborted runs get no usage [T: `spike_metering.md:219`]; a dead stream's gap "cannot be told apart from an over-count" [D: `inference_metering.md:162`]; Tinfoil's usage value is an unsigned string [D: `existing_systems.md:47`]. A library-level mismatched refund returned Ok and silently lost value [T: `spike_act.md:115,269`].

### 3.4 Is the operator's incentive to overcharge checked?

Only weakly, and the check has the same root as everything else.

- **Cryptography:** `t` is bounded above (`t <= s + a` in `act-rs` [D: `credentials_crypto.md:166`]) with no lower bound on what the operator returns. `t=0` is valid.
- **Attested code:** the only strong check, and a release can change it. Allowlist rotation is routine (Tor cadence, and Verda firmware outside your control [D: `verda_platform.md:195`]). [I]
- **Recourse:** anonymous users have almost none. Showing a receipt "would link that spend to the request" [D: `inference_metering.md:160`]. Goodwill only [D: `purchase_wallet_abuse.md:668`]. A chargeback reaches the purchase, not the spend. [I]
- **Price:** no signed price table exists in any surveyed system [D: `inference_metering.md:222`], though it is simple to add. [I]
- **Exposure cap:** smaller chains limit the loss per session to one chain, $5 in the prior investigation's design [P: `prior:146,337`]. That caps the harm without detecting it, and its claim that the wallet "recomputes every charge and detects" a short refund conflicts with the spike's per-request mismatches [T: `spike_metering.md:98,363`].

**Where the notes are too pessimistic: a real statistical audit exists.** `spike_metering.md` correctly refutes per-request recount (242 of 434 disagree) and concludes clients cannot reconcile. But the same data shows:

- `prompt_tokens` reproduces exactly, 421/421. [T: `spike_metering.md:5`]
- Aggregate completions differ by only 1.5% (16,596 reported, 16,347 recounted). [T: `spike_metering.md:98`]

So a client can check prompts exactly and check completions over many requests. That would catch 3.27x inflation or a sustained 5%+ skew, without any new protocol. [I] It needs a client-side tokenizer and does not cover aborted streams, thinking tokens or stochastic rounding. [I] It was measured on one small model on llama.cpp only, so re-test on vLLM. [T: `spike_metering.md:310`] A deterministic grid keeps the check per-request [D: `credentials_crypto.md:404`]. Stochastic rounding gives an operator room to bias, which argues against `credentials_crypto.md:404`'s suggestion to move to it later. [I]

---

## 4. Project-level gaps

### 4.1 The tenant-operator paradox

NVIDIA's own boundary includes "anyone with access to protected assets inside the guest" [D: `tee_gpu_attestation.md:55`], and Verda's flows give the tenant exactly that [D: `tee_gpu_attestation.md:109`]. Beyond the image, no current track examined these points. [I]

- **The operator knows buyers and controls updates.** A targeted release or client build is possible, and the same party holds the email address to send it to.
- **Compulsion.** One legal order can require logging or a backdoored image. No mechanism resists it, and detection needs an outside log watcher. Verda may disclose "to any regulator, law enforcement agency" [D: `verda_platform.md:260`]. Meta's and Apple's designs rely on non-colluding third parties, and NCC notes their "contractual obligations are toward Meta" [D: `existing_systems.md:121`]. A one-person PoC has no equivalent.
- **Required deploy fields.** SSH keys are required at deploy, so a verifiable image must demonstrably ignore them [D: `verda_browser_notes.md:83`]. Restart automation means an operator API loop stays in place [D: `verda_platform.md:61`].

A PoC should state this openly: "For now, you are trusting the operator's release process." [I] The prior investigation's answer, three-of-five witnesses plus a wallet rule needing at least three providers [P: `prior:262,266`], is a sound shape that a one-person PoC on one provider cannot have. [I]

### 4.2 Reproducible builds

- TOB-WAPI-18 is High and only partially resolved at Meta [D: `tee_gpu_attestation.md:98`].
- What's tested: reproducing `MEASUREMENT` from public artifacts [T: `spike_attestation.md:14`]. What's not: rebuilding those artifacts from source [T: same note's limits]. Tinfoil's "byte-for-byte" stays "conditional; not reproduced" [D: `existing_systems.md:411`]. Verda's OVMF rebuild is a claim [D: `verda_platform.md:398`].
- Portability is a fork: firmware differs, no documented `roothash` on cmdline, disk layout differs [D: `verda_platform.md:196-197`], and hosting uses a private `tinfoild` [D: `existing_systems.md:68`]. Licence AGPL on image, action and verifier [D: `existing_systems.md:76,455,457`]. The default verifier rejects chips outside Tinfoil's inventory, so a Verda backend needs a fork or a separate verifier [D: `fact_check.md:141-143`; `spike_attestation.md:17`].
- The rootfs is unmeasured by default [D: `verda_platform.md:138`]. Verda's extractor algorithm is undocumented [D: `verda_platform.md:204`]. V3 to V5 in `verda_platform.md:374-376` are the first real go/no-go.
- vLLM, driver blobs and Python wheels in a hermetic build are unexamined by any note. [I]

### 4.3 Legal exposure

Researched and open:

- **Verda:** platform use must not be "entirely or predominantly" the Service; "competes with" while Verda sells per-token inference; "may contribute to social harm"; suspension if use "may subject the Supplier ... to any liability". [D: `verda_platform.md:254-259`]
- **NVIDIA:** "A commercial Enterprise Product license must be obtained before offering or distributing a Customer Product"; 3-year records and audit rights. [D: `verda_platform.md:264`] `tee_gpu_attestation.md:31` says a production licence is required. Price unknown.
- **Stripe:** stored value sits in the Restricted section, which means added diligence and revocable approval. Seller-issued credits are "subject to limits" and not barred, so approval is plausible but discretionary, and nothing addresses bearer anonymous credentials [D: `fact_check.md:176-180`]. Visa excludes stored-value loads from liability shift. [D: `purchase_wallet_abuse.md:324`] $15 fee floor; 3 fraud disputes in 600 payments breach VAMP. [D and own arithmetic: `purchase_wallet_abuse.md:348,352`]
- **Chargebacks:** bearer credit can't be revoked; a window of 120+ days can't be held; Stripe's evidence asks for proof of use, which the design forbids. [D: `purchase_wallet_abuse.md:303,319,371`] A cohort hold of one or two weeks does not outlast this either [P: `prior:317`].
- **Tinfoil:** "You may not share accounts or API keys"; no resale without separate agreement. [D: `existing_systems.md:79-80`] The AUP also bars falsifying "billing, usage, verification, or attestation data". [D: `fact_check.md:133`]
- **Prepaid:** closed-loop treatment, Reg E five-year expiry against key rotation, EU withdrawal and VAT evidence retained for 10 years, all flagged for counsel. [D: `purchase_wallet_abuse.md:406-438,469-471`]

**Not researched by any track, and potentially the largest:**

- Illegal-content reporting duties for a service that cannot see content or users. [D as gap: `purchase_wallet_abuse.md:489,555`]
- Whether designing out moderation worsens the operator's position. [I]
- EU AI Act and DSA obligations, model-licence terms for a public service, and personal liability of a sole operator. These do not appear in any note. [I from text review]

"Cannot help investigate" is a design property here [D: `purchase_wallet_abuse.md:542`]. Counsel must see that sentence first. [I] The prior investigation's in-enclave policy model, which refuses in-stream, adds a control without linking [P: `prior:319`]. Whether that meets any duty is a question for counsel. [I]

**A USD Stripe sandbox plus private use avoids nearly all of this** and still proves mechanics [D: `purchase_wallet_abuse.md:477`]. That's the honest PoC boundary. [I]

### 4.4 Honest cost at PoC scale

| Item | Monthly | Evidence |
|---|---|---|
| `1RTXPRO6000.30V.CC`, on-demand | $2,029 | [D: `verda_platform.md:38`; console `verda_browser_notes.md:17`] |
| Storage, 100 GiB NVMe | About $20 | [D: $0.20/GiB, `verda_platform.md:50`; size is my assumption] |
| CPU CC issuer, optional | $36 | [D: `verda_platform.md:44`] |
| **Floor** | **About $2,085** | [I] |
| Spot at $1,015 | Not usable with a persistent `skM` or a promise of availability | [D: eviction `verda_platform.md:51`; I] |
| B200 alternative | $54,436 | [D: `verda_platform.md:41`] No smaller size in stock. |
| Second model | +$2,029 per GPU model | [I, from `inference_metering.md:230`] |
| NVIDIA licence, counsel, Stripe fees, disputes, reserve, audit | Unknown; not priced by any note | [I] |
| Maintainer time on a two-week security cadence | Unpriced, and the real cost | [I] |

The $100 project balance buys about 36 hours on-demand [I: `verda_browser_notes.md:17,31`].

**The break-even trap.** At an illustrative $10 per month per buyer, the floor needs about 209 buyers. Prices are my assumption. At that size the first-spend best guess is 0.78 at 1 hour's delay, 0.13 at 24 hours and 0.02 at a week [I, using `purchase_wallet_abuse.md:150`]. Request-grouping and exact-amount linkage stay strong. A service can therefore be viable and still small enough that the anonymity claim is weak, and revenue does not push it past that point. Token throughput isn't what binds: at an illustrative $1 per million tokens the floor means about 794 tokens/s continuous. [I]

**CC overhead on a small model at this SKU is unknown.** 34-39% was Qwen3-8B on one B200 with SGLang on TDX, and about 10% dense was RTX PRO on TDX. No measured figure covers RTX PRO on SNP [D: `fact_check.md:266-270`; `tee_gpu_attestation.md:219`]. The prior investigation's $20,000 a month for four H100s does not transfer [I; `prior:10,367`].

**Availability.** One site, one SKU, no in-guest reboot, maintenance on 2026-11-08 [D: `verda_browser_notes.md:31`, `verda_platform.md:91`]. One Verda decision stops every wallet's spend path. [I]

### 4.5 Things never tested end to end

No test joins any two layers. [I] Individually:

- `act-rs`, EC-ACT, draft test vectors. [T: `spike_act.md:10,236-237`] Its crates.io name also collides with `act_rs` 0.5.0, a supply-chain trap. [D: `fact_check.md:63`]
- vLLM, GPU CVM. [T: `spike_metering.md:7,310`]
- Anything on Verda hardware. [D: `verda_platform.md:3`; T: `spike_attestation.md:15`]
- Cell-level capture, any observer beside the service, padding, load. [T: `spike_tor.md:13,147`]
- Attestation over real Tor. [T: `spike_attestation.md:318,359`]
- Stripe, even in sandbox. [D: `purchase_wallet_abuse.md:10`]
- Crash and power loss for wallet or store. [T: `spike_act.md:238`]
- GPU evidence bound to this CVM: "nothing ... proves" it. [T: `spike_attestation.md:288`]
- RIM and OCSP in the client. [T: `spike_attestation.md:19,254`]

### 4.6 Maintainable and understandable?

The requirement conflicts with the honest component list: Stripe path, issuer, unaudited credential code, atomic store, gate, engine, Tor service, CPU and GPU verifiers, reproducible image, log and monitor, wallet with journal, Tor client, backend adapters. Each carries its own security cadence, and Verda's firmware can change with only "commercially reasonable endeavours to notify" [D: `verda_platform.md:169`]. Some of this can't be cut (journal, single config, verifier). A first version could defer the log's independence, multi-model, multi-backend and split wallets, provided the claims shrink to match. [I]

Other unexamined items, briefly: wallet at rest and key storage "not verified" [D: `purchase_wallet_abuse.md:627`]; AGPL on an embedded verifier [D: `spike_attestation.md:230`]; no GPU key rotation on Blackwell SPT in R580, which matters for an always-on boot, and R595 status is unknown [D: `tee_gpu_attestation.md:12,348`]; vLLM telemetry on by default [D: `inference_metering.md:198`].

---

## 5. Conflicts between notes

Rule applied: spike results outrank source-only reasoning for what they tested; `verda_browser_notes.md` outranks public pages for what can be rented.

| # | Conflict | Sources | Resolution | Status |
|---|---|---|---|---|
| 1 | Arti onion service readiness | "ready for use" and "suitable for testing and experimentation only" (`spike_tor.md:42-43`). "Some of the security features ... not yet implemented" (`tor_traffic_analysis.md:96`). Spike ran it without `experimental` | It works mechanically [T]. Privacy was not tested [T]. No current upstream document lists what is missing, so C tor is the safer default [D: `fact_check.md:189-191`]. The spike does not contradict that. "Only for testing" (`prior:196`) rests on the older document | Resolved |
| 2 | Arti version | 2.6.0 latest found (`purchase_wallet_abuse.md:585`) vs 2.8.0 built (`spike_tor.md`) | 2.8.0. `purchase_wallet_abuse.md` is stale. `arti-client 0.48.0` is a crate version, not a conflict | Resolved |
| 3 | Arti client PoW | "Not yet available" vs client from 1.3.0 (`purchase_wallet_abuse.md:521`); `hs-pow-full` experimental (`spike_tor.md`); `enable_pow = false` (`tor_traffic_analysis.md:96`) | Exists behind experimental flags. Whether it solves live puzzles only a test can settle | Open |
| 4 | Which draft is current | Purchase track uses `-01` throughout. Credentials says `draft-schlesinger-mole-ec-act-00` of 2 Nov 2026 is the current name (`credentials_crypto.md:20,259`). Existing systems says "ACT" generically | EC-ACT -00 with MOLE, active since 2 Nov 2026. `-01` expired. "Expired individual draft" (`purchase_wallet_abuse.md:16`) and "expired in August 2026" (`prior:108`) are true of the old draft only [D: `fact_check.md:53`] | Resolved |
| 5 | "ACT defines no retry" | `purchase_wallet_abuse.md:572,749` vs MOLE recovery by byte-identical resend or handle POST (`credentials_crypto.md:193`) | Superseded by MOLE. Still, the spike confirms a library alone gives none (`spike_act.md:12`), and the transport draft lists what recovery reveals as TODO (`credentials_crypto.md:214`) | Resolved |
| 6 | EC-ACT vs Ristretto numbers | L=32 spend: 4,531 B spec (`credentials_crypto.md`), 4,544 B by -01's formula (`purchase_wallet_abuse.md:572`), 4,919 B measured wire (`spike_act.md:112`). Refund 137 vs 176. Verify 8.61 ms vs 1.53 ms. Token 192 vs 211 | Spike numbers apply to `anonymous-credit-tokens` 0.4.2 only (`spike_act.md:236`). EC-ACT unmeasured. Do not quote 650 junk/s for EC-ACT | Resolved as to scope; EC-ACT timing open |
| 7 | Store size | 32 B per nullifier (`credentials_crypto.md:468`) vs 240 B (`spike_act.md:113`) | Both: 32 B for key life, plus 208 B result row during retention | Resolved |
| 8 | Retention | 300 s example (MOLE), "far longer" (`credentials_crypto.md:215`), RAM with short TTL (`inference_metering.md:115`), "user-funds parameter" (`purchase_wallet_abuse.md:613`) | RAM fails on reboot and strands whole balance [T for strand: `spike_act.md:12`]. Durable, at least the longest plausible outage. | Direction resolved; value open |
| 9 | Meter definition | "Tokens the gate sent" (`inference_metering.md:79`) vs charge generated (`spike_metering.md:242`) | Spike for llama.cpp: gate-sent loses up to 75% to a stalled reader. Re-test on vLLM. Charging generated means paying for unseen tokens (noted) | Resolved for tested engine |
| 10 | Hash chain adequacy | Described as sufficient with signature (`inference_metering.md:160`) vs refuted for honesty (`spike_metering.md:302,363`) | Consistency only. `inference_metering.md:162` concedes attestation must carry dead streams | Resolved |
| 11 | Exact ID checks | "Checked exactly if server exposes IDs" (`inference_metering.md:5`) vs streamed IDs lossy, exact only non-streamed (`spike_metering.md`) | Llama.cpp: non-streamed only. vLLM's `return_token_ids` untested (`spike_metering.md:304`) | Open for vLLM |
| 12 | `max_tokens` behaviour | README, inference notes vs spike (0 over in 504+2,459; `=0` gives 1 token, `spike_metering.md:255`) | Spike, for b12110 only | Resolved for tested engine |
| 13 | "Up to 92%" worker-thread drain | `inference_metering.md:27` cites `arXiv 2606.23969` vs "could not find support" in `v1` (`tee_gpu_attestation.md:219`) | `tee_gpu_attestation.md:219` is wrong: the paper states it in its abstract, contributions and recovery table, qualified at c=512. The real limit is that nobody reproduced it [D: `fact_check.md:268`] | Resolved |
| 14 | Tinfoil reachable over Tor | "Plausible blocker" Cloudflare-fronted proxies (`tor_traffic_analysis.md`), "unknown" (`existing_systems.md:361`) vs 14 of 15 reached incl. `.well-known/tinfoil-proxy` (`spike_tor.md:169,236`) and V3 client contacting only the enclave (`spike_attestation.md:355`) | Reachability is better than feared. kds-proxy, github-proxy and authenticated POST were not tried | Partly resolved |
| 15 | Verifying Tinfoil over Tor | `existing_systems.md:319,325` suggests the CLI, then `torsocks` vs CLI not SOCKS-safe (`spike_attestation.md:196`) | Use the SDK. CLI under `torsocks` untested. Do not follow V1 or V2 as written | Resolved |
| 16 | Who appraises the GPU | Enclave at boot (`tee_gpu_attestation.md:72,76`) vs client binds but does not appraise (`spike_attestation.md:12,234-236`) vs Verda's `--verifier remote` (`verda_browser_notes.md:66`) | Enclave at boot, with evidence bound to the report. Avoid NRAS, whose documented key is identity (`spike_attestation.md:270`). Neither binds GPU to CVM (`spike_attestation.md:288`) | Resolved; gap remains |
| 17 | Kernel cmdline control | "Unknown" (`verda_platform.md:399`), host-derived (`verda_platform.md:149`), "Verda appears to control" (`spike_attestation.md:130`) vs GRUB hint (`verda_platform.md:152`) | No Verda source says where the non-root part of `-append` comes from, so "host-derived" goes beyond the evidence. A verity hash in the initrd is safe either way [D: `fact_check.md:254-258`]. Run V2 | Open |
| 18 | Verda CPU signature | 26/2/1 vs 26/0/0 (`verda_platform.md:165`, `spike_attestation.md`) | Needs an instance; both diverge on real input | Open |
| 19 | B200 size and TEE | 1x at $9.32 (pricing) vs 2x, 4x, 8x (API), 8x AMD only in stock (`verda_browser_notes.md:18`). SPT on B200 and TDX documented (`tee_gpu_attestation.md:8`) | Console rules: only 8x AMD, $74.57/h. SPT B200 and TDX are not currently rentable | Resolved |
| 20 | Verda vTPM | "Signed virtual TPM" (marketing) vs docs silent (`verda_platform.md:94`); OpenPCC needs one (`existing_systems.md:176`) | Assume none until shown. OpenPCC path stays weak | Open |
| 21 | GPU ready state | Blog implies automatic vs docs manual (`verda_platform.md:90`) | Docs (`verda_browser_notes.md`) | Resolved |
| 22 | Driver and OS | 580.126.09, 98.02.8D (Verda sample) vs 595.58.03 (blog), R595 (NVIDIA) vs CUDA 13.2, 13.4 images; 24.04 vs 26.04 | Depends on image. Test on the instance | Open |
| 23 | Verda FAQ on bus probing | "Recover ciphertext, not plaintext" (`verda_platform.md:353`) vs TEE.fail | FAQ overbroad | Resolved |
| 24 | Physical extraction wording | Tinfoil FAQ "key extraction on SEV-SNP" (`existing_systems.md:54`) vs "no VCEK extraction claimed" (`tee_gpu_attestation.md:126`) | Both hold: an application key was extracted, not a VCEK. One Glitch VCEKs circulate for Zen 1-3 (`tee_gpu_attestation.md:137,181`) | Resolved |
| 25 | Anonymity's dependence on TEE | "Less dependent" (`tee_gpu_attestation.md:295`) vs `s` and `t` hidden "subject to TEE trust" (`credentials_crypto.md:406`) | IP and cryptographic link: independent. Amounts and timing: dependent unless rounded | Resolved |
| 26 | Timing through Tor | "Inter-token timing present" (`tor_traffic_analysis.md:307`) vs gaps under 50 ms do not survive (`spike_tor.md:12`) | Different observers: spike is at client reads. Service-side, cell level is untested (`spike_tor.md:13`). Neither supports "Tor hides shape" | Resolved as scope |
| 27 | Cold start | Estimated 2-6 s (`tor_traffic_analysis.md`) vs 7.74 s median, 5.32-15.09 (`spike_tor.md:74`) | Spike, n=5, same machine | Resolved |
| 28 | Per-request isolation | "Non-negotiable" (`tor_traffic_analysis.md:176`) vs "choice belongs to UX and crypto tracks" (`purchase_wallet_abuse.md:287`) vs measured cost | My reading in §2 attack 13: scale-dependent | Judgement |
| 29 | Upload buckets vs proof | 4 KiB smallest bucket (`tor_traffic_analysis.md:337`) vs 4,531-4,919 B spend | Bucket does not fit at L=32 | Resolved |
| 30 | Inbound on CVM | All inbound blocked (`verda_platform.md:229`) vs issuer needs Stripe webhooks (`purchase_wallet_abuse.md:42,71`) | Issuer cannot both hold `skM` in that CVM and take clearnet webhooks. Needs a decision (§2 attack 19) | Open |
| 31 | Key epochs | Short epochs limit fraud (`purchase_wallet_abuse.md:366,535`) vs forfeiture (`credentials_crypto.md:219`) vs Reg E five years (`purchase_wallet_abuse.md:412-413`) | Cannot all hold. Long context plus cross-context migration unspecified. Counsel and cryptographer | Open |
| 32 | Source of timing warning | NCC (conversation) vs Trail of Bits (`purchase_wallet_abuse.md:14,109`, `existing_systems.md:117`); `tor_traffic_analysis.md:376` still attributes via the conversation | Trail of Bits for Meta; NCC's warning there is relay and gateway collusion [D: `fact_check.md:158`]. `prior:214` cites a different NCC report, on Google's Private AI Compute, which no note checked | Resolved |
| 33 | Gateway read of tokens | "Cannot simply read" (conversation) vs documented trailer (`existing_systems.md:40,418`) | Trailer exists, unsigned | Resolved |
| 34 | Usage profiles and fingerprints | Weak re-ID at 1.6% (`credentials_crypto.md`) vs collapse in 2-3 purchases (`purchase_wallet_abuse.md:217`) | Not in conflict: usage statistics are weak, a stable fingerprint is strong | Resolved |
| 35 | Tinfoil client version | `tinfoil-go` around v0.16.0 (`existing_systems.md:13`) vs v0.17.0-rc.8 tested (`spike_attestation.md:34`); live ATC format `v2` (`existing_systems.md:29,321`) | Spike tested a pre-release on a beta path. Pin, and expect churn | Resolved as scope |
| 36 | CC overhead | 34-39% small model, about 10% RTX PRO dense (`tee_gpu_attestation.md:219`), 13-27% (`inference_metering.md:26`) | Different stacks: 13-27% is all B300, and 34-39% is one B200 on SGLang and TDX. No figure covers RTX PRO on SNP [D: `fact_check.md:266-270`] | Open |
| 37 | SNP TCB floor | Floor values and "mostly closed" (`tee_gpu_attestation.md:139,161,162`) vs AMD removing them on 2026-09-24 (`fact_check.md:93-97`) | The floor cannot be derived from AMD, and the values survive only on a secondary page. Client policy needs a new basis | Open, high impact |
| 38 | Tinfoil chip allowlist | "Not found" (`existing_systems.md:100`) vs fleet-bound (`spike_attestation.md:17,220`) | Fleet-bound in source; reject path not run (`fact_check.md:141-142`) | Resolved |
| 39 | Tinfoil's hosting | "Own bare metal" (`verda_platform.md:198`) vs FAQ's "cloud providers" (`fact_check.md:150`) | Unverified, so drop that reason. Two separate backends still holds | Resolved |
| 40 | Whisper Leak time-only range | 70-84% (`tor_traffic_analysis.md:312`) vs per-model 46-85% (`fact_check.md:221-229`) | Use about 46-85% per model. Still no Tor test | Resolved |
| 41 | Session opening | 1-2 s (`prior:188`) vs 4.33 s fresh, 7.74 s cold (`spike_tor.md:11,74`) | Spike outranks, with the incident caveat (`fact_check.md:212`) | Resolved |
| 42 | Spend proof size | About 2.5 KB (`prior:130,196`) vs 4,919 B at L=32 (`spike_act.md:112`) | Mostly bit length: 2,496 B at L=16 by the `-01` formula [I]. EC-ACT unmeasured | Resolved as scope |
| 43 | Unit of spending | Per session (`prior:136-138`) vs per request (`spike_metering.md:239`, `credentials_crypto.md:205`) | Design choice, and nothing has tested a session-level hold. I favour per session (§7) | Open |
| 44 | Strength of unlinkability | "Information-theoretic", including quantum (`prior:106`) vs "remain to be written out" and "also remain open" (`credentials_crypto.md:25,113`; `fact_check.md:45`) | Do not claim it for EC-ACT until the argument exists | Resolved |
| 45 | Batched issuance | Core defence (`prior:218`) vs "do not work well" (`purchase_wallet_abuse.md:233-235`) | The forced delay helps. The batch adds little, can be an n-1 set, and its size is unverifiable [I] | Resolved |
| 46 | Cover traffic | Paid cover sessions (`prior:226`) vs "add noise, not buyers, and cost compute" (`tor_traffic_analysis.md:388`) | Compatible. On one GPU the compute also crowds out real work [I] | Resolved |
| 47 | Client check of charges | "Recomputes every charge and detects" (`prior:337`) vs 242 of 434 mismatches and 3.27x passing (`spike_metering.md:98,363`) | Spike outranks. Aggregate check only (§3.4) | Resolved for tested engine |
| 48 | DDRop details | Dual-socket caveat and CI "does not help" (`tee_gpu_attestation.md:141,143`) vs the paper (`fact_check.md:80-81`) | Limits "do not inherently preclude" single socket. The CI line holds for forgery only, and matters only for TDX B200 | Resolved |
| 49 | B200 mode | "Protected PCIe" (`verda_platform.md:106,327`) vs Hopper-only (`tee_gpu_attestation.md:8`) | The relevant mode is MPT (`fact_check.md:124`) | Resolved |
| 50 | Refund bound | `t ≤ s` (`prior:106`) vs `t <= s + a` (`credentials_crypto.md:166`) | Different drafts: EC-ACT adds allowance `a` | Resolved |

**Evidence hygiene.** The Tor spike's service forwarded for up to 53 s to the attestation spike's server, which got 4 stray `GET /stream` requests [T: `spike_tor.md:14,171-181`]. The attestation note's results should be read knowing that; the effect looks negligible. [I] The fact check found one mismatch against raw spike output: ACT T2 has 1,330 decode errors and 4,118 `InvalidProof`, against 1,327 and 4,121 in the note, with matching totals [D: `fact_check.md:278`]. It read `purchase_wallet_abuse.md` and `spike_tor.md` only in part, and did not review this file [D: `fact_check.md:302-303`].

---

## 6. Where the notes lean too far

**Too optimistic**

- Treating `skM` in an enclave as hiding `s` and `t`, without addressing store encryption, key persistence, or a front end that can say "paid". (§2 attacks 1, 19; §3.1) [I]
- Treating enclave shaping as the fix for traffic shape. It covers passive network observers and leaves the host. (§2 attacks 15-17) [I]
- "A retry ... links to itself and to nothing else" (`credentials_crypto.md:214`). True of the cryptography, and false as an operational statement. (§2 attack 3) [I]
- Key transparency as a fix for tagging, with no independent watcher. (§2 attack 7) [I]
- "A verifiable design appears feasible" (`verda_platform.md:188`). Reasonable from documents, but host-side policy facts are unknown, and a strict client may refuse the real host. [I]
- Reading U in the simulation as customers. (§1.1) [I]
- "SNP's failures are mostly closed by a TCB floor" (`tee_gpu_attestation.md:161`), now that AMD has withdrawn the values. [D: `fact_check.md:97`]
- The prior investigation's first principle, that unlinkability never depends on the TEE (`prior:260`). True of the credential link. It concedes amount privacy rests on hardware (`prior:182`), and at small scale amounts plus timing are the link. [I]

**Too pessimistic**

- Per-request isolation as non-negotiable at PoC. (§2 attack 13) [I]
- "Cannot reconcile" completions: true per request, not in aggregate. (§3.4) [T for data; I for use]
- Rounding as loss of "per token". The ledger can still show exact tokens, with a mean 7.6% charge difference on synthetic data, and rounding saves proof size. [D: `credentials_crypto.md:371`; I]
- Tinfoil over Tor as a likely blocker. (§5 row 14) [T]
- Mint impact on a self-hosted fixed-cost GPU. (§3.2) [I]
- A TEE break as total: it does not unblind past spends or reveal IPs, and Tor-in-CVM limits retroactive reading. (§3.2) [I]
- Padding bandwidth: a shaped flow is 16.4 kbit/s [D: `tor_traffic_analysis.md:334`], so 10,000 req/h in 120 s slots is about 5.5 Mbit/s total. [I]
- Tenant arrival-time visibility, once a no-admin image exists. (§1.2) [I]

---

## 7. The prior investigation against this PoC

It is a fuller design than the current notes and worth mining. It carries three caveats: its numbers come from its own model, it assumes four H100s at two providers, and it assumes credit arrives weeks after payment [P: `prior:3,10,228`]. The verdicts below are mine. [I]

| Idea | What it helps here | Where it breaks on Verda with card purchase | Verdict |
|---|---|---|---|
| One spend per session, balance held in enclave, rolls inside the channel (`prior:138,148`) | Attacks 2, 5, 10, and makes 13 easier | The balance dies with RAM unless refund inputs persist (`prior:339`). The spike's per-request gate rules need re-testing. Held transcripts compete for one GPU's memory | Working assumption; test first |
| Uniform $5 chains, two spend tiers (`prior:146,162`) | Attacks 1, 2, 4; caps loss in attack 3 | A refund "rounded to the cent" keeps a `t` channel for whoever holds `skM`. The tail tier still flags a nearly empty chain (`purchase_wallet_abuse.md:241`) | Adopt |
| Weekly cohorts, activation at W+2 (`prior:170,218`) | Attack 20 | Credit is usable 1 to 5.5 weeks after payment, which conflicts with buy-then-use. Cohorts are about 23 people at 100 monthly buyers, and the operator can stall others (`purchase_wallet_abuse.md:235`) | Offer the delay as an option; make no cohort claim at PoC |
| Calendar purchases, six-week reserve (`prior:224`) | Top-ups in attack 20 | More float at risk, on a wallet with no recovery | Optional |
| Paid cover sessions (`prior:226`) | Presence patterns | Spends real money and GPU on a single box; adds noise, not buyers | Defer |
| Constant cadence, 10 frames/s (`prior:166`) | Wire shape | Same idea as `tor_traffic_analysis.md:334`. Host delays remain. 18.5 MB/h per open session (`prior:202`) | Adopt |
| Fixed-shape GPU engine (`prior:208`) | Attack 16 | Custom engine, unmeasured on RTX PRO SNP. At $2,029/month its estimated cost is $406-710/month of capacity | Measure the leak before building |
| Separate ledger enclave with replica (`prior:94,264`) | §3.1, rollback | No second CC provider has been researched (`verda_platform.md:364`), and the `skM` update path is undefined | Separate CPU CC instance: yes. Rollback claim: no |
| Witness-cosigned log, one-week adoption (`prior:266`) | Attack 7, targeted images | No witnesses at PoC; attack 24 | Log now; claim nothing from it yet |
| Three or more providers (`prior:262`) | Physical custodian | Verda is one, and Tinfoil is not interchangeable (`fact_check.md:143`) | Out of PoC reach; say so |
| Flat-link mode (`prior:321`) | Retrospective flow matching | 0.6 GB/day, needs a padding source, unproven | Research |
| "Weakly anonymous beta" label and one-way rule (`prior:254`) | The central claim | None | Adopt now |

None of these moves a §1.3 row from false to true at 100 users. The forced delay does extend the reserve case to first purchases. The prior investigation's own summary fits: "What it cannot remove is the statistics" [P: `prior:417`].

## 8. What would change these conclusions, fastest first

1. Boot a no-SSH custom image on `1RTXPRO6000.30V.CC`. Compare offline digest to the real report, and read POLICY, SMT and TCB. (`verda_platform.md:372-381`, V1-V5) If this fails, Verda is out as a verifiable self-hosted backend. [I]
2. Find a new basis for the SNP patch floor: ask AMD why the values were removed, and establish what report and VCEK fields can show. Without it, the client policy has no source. [I, from `fact_check.md:97`]
3. Write down the `skM` lifecycle end to end: boot, update, eviction, chip change, retirement. If it cannot be written, use capped per-boot credit or denominations. [I]
4. Compile and run `act-rs` on the same scenarios as `spike_act.md`, including junk rejection. Then compare honestly with blind RSA denominations. [I]
5. Capture cell-level traces beside the service: passive, then with injected delays, and read the client side. Settles attack 15 and §5 row 26. [I]
6. Observe host-visible GPU and disk activity from a host you control, with a CC GPU, if available. Settles attacks 16 and 17. [I]
7. Prototype a session-level hold, then re-run the metering spike on vLLM. Settles §5 row 43. [I]
8. Ask counsel the illegal-content question and get written answers from Verda and NVIDIA before any non-private use. [I]
