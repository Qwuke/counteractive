# Independent review of the anonymous inference report

Reviewed 6 November 2026. Scope: the [report](Anonymous%20metered%20private%20inference.md), its relevant supporting notes, surviving experiment output, and primary sources for the architectural claims. Recommendations are in [ARCHITECTURE.md](../ARCHITECTURE.md).

The report is useful research, especially its distinction between laptop experiments and untested deployment work. Its main weakness is turning conditional observations into universal conclusions. Several of those conclusions would unnecessarily constrain the architecture. The recorded experiments largely support the reported numbers; they do not establish all the security properties inferred from them.

The findings below distinguish **corrections**, **unsupported conclusions**, and **design choices**. Priority 1 affects a trust boundary or major architecture decision. Priority 2 affects a validation criterion, cost estimate, or important qualification. Locations use the original report's line numbers, before the review notice was inserted; quoted phrases also identify the passages.

## Feedback to address before adopting the report as an architecture

### R1 — Priority 1: hold-and-refund has relevant prior art

**Correction.** The concept table, section heading and metering discussion say that no surveyed system implements a hold with returned change (original lines 14, 36 and 48).

OpenPCC's `Processor.BeginTransaction` verifies a credit and locks its nonce; `Transaction.CommitWithUnspend` consumes it and issues an unused-value coupon. Its whitepaper §2.4.2 describes a maximum spend, an inference cap and returned unused credit. These were already described in this repository's [existing-systems notes](../research_notes/Anonymous%20metered%20private%20inference/existing_systems.md). I independently read the [processor implementation at commit 911390a](https://github.com/openpcc/openpcc/blob/911390a80afcbe6dfb48987a641ab345494b1eca/anonpay/processor.go).

[Routstr's payment documentation](https://docs.routstr.com/client/payments/) also describes reserving an authorization ceiling before forwarding, settling at measured cost, and withdrawing remaining funds. Its persistent session/API-key balance is a different privacy design from unlinkable per-request credits. Its presence still refutes the broad absence claim.

**Suggested replacement:** “Prepaid ceilings and returned change have prior art in OpenPCC and Routstr. We have not validated a durable implementation combining our chosen credit protocol, attested metering, cancellation and recovery.”

**Architecture implication:** study and adapt their state transitions and failure cases before designing a new payment protocol. Existing code is evidence of precedent, not proof of production correctness.

### R2 — Priority 1: exact accounting is compatible with some privacy boundaries

**Unsupported universal conclusion.** The statement that per-token billing and privacy cannot both be exact, followed by mandatory coarse settlement, is too broad (original line 88).

EC-ACT exposes its spend and return amounts to the verifier; that observation is correct. But the report then conflates the verifier, the enclave and the human operator. If exact counts and charges stay inside approved confidential code, the operator need not receive them. The repository's [credential notes](../research_notes/Anonymous%20metered%20private%20inference/credentials_crypto.md), under amount leakage, explicitly make this distinction. The [EC-ACT draft §6](https://datatracker.ietf.org/doc/html/draft-schlesinger-mole-ec-act-00#section-6) describes leakage from public amounts, not an impossibility theorem about private computation.

**Suggested replacement:** “Exact charges revealed outside the confidential boundary can become correlation signals. Keep exact accounting inside that boundary; choose deliberately between exact external settlement, coarse settlement, and a protocol that hides settlement amounts.”

**Architecture implication:** distinguish token accounting, balance updates, externally visible payment amounts and bill verification. Session settlement reduces the number of observations but still reveals an aggregate amount to an external refund issuer. It does not automatically hide that amount. Choosing rounding is reasonable; claiming it is mathematically unavoidable is not.

### R3 — Priority 1: cache isolation is an alternative to disabling every cache

**Correction and qualification.** The report says prefix caching must be off and implies that privacy therefore forces a 42,000-versus-4,000 input-token bill (original lines 48 and 88). Shared caching across unrelated users does introduce a timing channel. It does not follow that reuse within an intentionally linked conversation must also be prohibited.

[vLLM documents cache salting](https://docs.vllm.ai/en/latest/design/prefix_caching/#automatic-prefix-caching): requests with different salts do not share cached KV blocks. A fresh unpredictable salt per session can preserve conversation-local reuse. The gate must assign it, keep it confidential and prevent clients from choosing another session's namespace. This addresses that cache-hit channel; it is not a defense against host observation or general resource contention.

[Tinfoil explicitly documents stable per-user or per-session cache secrets](https://docs.tinfoil.sh/sdk/prompt-caching). A fresh secret on every request is one conservative option, not the only isolation strategy.

The 42,000 arithmetic is valid for twenty prompts growing by 200 tokens each. The 4,000 number counts only new tokens in that simplified example. Neither establishes what an engine bills: cached tokens can remain included in prompt usage, and cached-token pricing is a separate policy. The [metering spike](../research_notes/Anonymous%20metered%20private%20inference/spike_metering.md) itself records 191 prompt tokens even when 190 are cached.

**Suggested replacement:** “Disable cross-session prefix sharing. Start with caching disabled; evaluate session-scoped caching with isolation tests and explicit cached-token prices. Distinguish full context size, prefill computation and billed input.”

### R4 — Priority 1: Tor's daily population is not a measured anonymity set

**Correction.** “The IP anonymity set is all Tor users” is not established by the delivery test or the 3.18-million figure (original line 11).

[Tor Metrics](https://metrics.torproject.org/userstats-relay-country.html) estimates directly connecting clients from directory requests and excludes bridge users in that series. It does not estimate the candidates for a particular inference session. Relevant candidates depend on what an observer knows: timing, geography, access to network paths, service membership and other metadata. Successful streaming tests demonstrate transport usability, not anonymity against these observers. The [Tor design paper](https://svn-archive.torproject.org/svn/projects/design-paper/tor-design.html) explicitly limits its traffic-analysis threat model.

**Suggested replacement:** “Tor normally prevents the destination from directly observing the client's IP address. Its overall population is context, not a quantified anonymity guarantee for this service.”

**Architecture implication:** describe protection per observer. Neither an arbitrary minimum user count nor a daily Tor population should unlock an ‘anonymous’ label.

### R5 — Priority 1: the attestation evidence supports a narrower result

**Unsupported evidence rating.** Calling “attested inference on Tinfoil's fleet” solid, with only the AMD patch caveat, overstates what the cited tests established (original lines 3 and 12).

The surviving output does contain four matching launch measurements and 44 divergent negative controls. The [spike](../research_notes/Anonymous%20metered%20private%20inference/spike_attestation.md) also clearly says that artifacts were not rebuilt from source, GPU reference-value and revocation checks were not tested, several verifier policy reject paths were only read, and real Tor was not used for attestation. Recomputing a launch measurement from downloaded binaries is distinct from independently rebuilding those binaries or validating a complete inference session.

[NVIDIA's threat model](https://docs.nvidia.com/confidential-computing/virtual-machines/threat-model) requires both CPU and GPU appraisal, approved workload loading and protected secret delivery. A valid GPU signature alone does not establish that complete chain.

**Suggested replacement:** “Offline CPU measurement reconstruction and selected verifier checks are supported by the recorded tests. Full confidential inference remains conditional on workload identity, channel binding, CPU/GPU policy, freshness, revocation and release trust.”

**Architecture implication:** maintain separate checks for source-to-artifact provenance, artifact-to-measurement binding, and measurement-to-live-channel binding. Signed provenance is not a substitute for source review or a reproducible build.

### R6 — Priority 2: 1.5% aggregate recount error does not establish 5% fraud detection

**Unsupported conclusion.** The assertion that the client can catch sustained skew above about 5% is not demonstrated by the experiment (original line 94).

The original records sum to 16,596 output tokens; the original analyzer recounts 16,347, a 1.5004% aggregate shortfall. But its forced partial-UTF-8 category already undercounts by 22/309, or 7.12%, without fraud. That artificial category is not an estimate of normal traffic; it is a counterexample to treating the aggregate as a universal error bound. No false-positive rate, detection power, workload-shift analysis or adversarial calibration was established.

The neighboring claim about exact client prompt counts also needs a scope limit. The independent tokenizer/template path matched 421 of 434 accepted requests; assistant prefill, typed content and `developer` role handling caused the other 13 differences. The 434-of-434 result used the server's own template/tokenization endpoints. Exact client reconciliation is feasible for a pinned, matching request format, but it was not demonstrated for every accepted format in this experiment.

**Suggested replacement:** “Recounting is a useful diagnostic. This corpus showed a 1.5% aggregate discrepancy, but no general fraud-detection threshold was validated.”

**Architecture implication:** enforce budget caps inside approved code and return authenticated usage receipts. Client recounting supplements that trust; it does not replace it. See the [preserved evidence and arithmetic](../research_notes/Anonymous%20metered%20private%20inference/review_2026_11_06/README.md).

### R7 — Priority 2: GPU rental price is not a bound on loss

**Correction.** The issuer-key discussion bounds minting damage by $2.78/hour of displaced self-hosted capacity (original line 86).

A fixed GPU constrains throughput and, under fixed provisioning, its rental expense. It does not bound foregone sales, service disruption, refunds, fees, or response costs. A forgery bug can also leave credits usable after the initial compromise until the relevant configuration is retired. These are distinct quantities; this is an accounting inference, not a new vendor-price claim.

**Suggested replacement:** “Fixed capacity limits immediate compute consumption. Total financial damage depends on capacity, pricing, outstanding forged credits, response time and contractual obligations; it is not bounded by the hourly rental price.”

**Architecture implication:** publish explicit exposure limits and avoid automatic scaling driven solely by apparently valid bearer credit.

### R8 — Priority 2: restore the missing VAMP counting assumption

**Missing qualification, not incorrect source arithmetic.** Three fraud-disputed payments in 600 exceed the cited non-compliant thresholds only under the counting assumption supplied in the purchase note but omitted from the report (original line 119).

[Stripe's documentation](https://docs.stripe.com/disputes/monitoring-programs) lists a count threshold of five and ratio of 0.5%, and explains that a transaction appearing in both TC15 and TC40 is counted twice. Three payments appearing in both produce six reports and 1.0%. Three TC15-only entries produce three and 0.5%, below the count threshold. The original [purchase note](../research_notes/Anonymous%20metered%20private%20inference/purchase_wallet_abuse.md) has the double-count assumption correctly. The denominator must also be the applicable captured-payment population.

**Suggested replacement:** “Three payments each reported in both TC15 and TC40 yield a VAMP count of six; against 600 applicable captured payments that is 1.0%, exceeding the cited non-compliant thresholds.”

This corrects the example, not the report's broader warning about chargeback exposure.

### R9 — Priority 2: key custody is an unresolved design, not a general impossibility

**Design choice presented too categorically.** The issuer-key discussion says the options provide only two of persistence, upgradeability and operator secrecy (original lines 20 and 86). That accurately describes gaps in the explored EC-ACT deployment; it does not establish a general three-way impossibility.

Changing an attested image does not inherently require changing the currency key or context. Keeping a key across releases requires a reviewed key-release or migration policy. Encrypted storage alone does not supply freshness: [Tinfoil's storage description](https://docs.tinfoil.sh/verification/attestation-architecture) explicitly excludes rollback protection for its persistent volumes.

A more substantial alternative is to change the requirement: use publicly verifiable blind tokens, keep the mint separate from inference, and explicitly trust the mint for financial custody. [RFC 9578](https://www.rfc-editor.org/rfc/rfc9578.html) distinguishes public-key verification from private-key verification. Possession of a correctly used blind-signing key does not by itself undo blindness, although configuration tagging and metadata correlation remain concerns.

**Suggested replacement:** “We have not validated persistent, upgradeable, operator-inaccessible EC-ACT key custody. A separate public-verification token design avoids placing the minting key in inference, at the cost of a different settlement protocol and explicit financial trust.”

**Architecture implication:** choose the financial trust model before buying infrastructure to solve a key-custody requirement that may be unnecessary. RSA verification separation does not make refund signing or state durability automatic.

### R10 — Priority 2: a failed measurement experiment needs diagnosis

**Overly strong decision criterion.** A custom-image digest mismatch is stated to remove Verda as a backend (original lines 110 and 129).

A mismatch must stop secret release. It can also result from incorrect prediction inputs: CPU model, vCPU count, firmware, command line or image extraction. The report's own spike lists these unknowns. [Verda documents direct kernel boot](https://docs.verda.com/cpu-and-gpu-instances/confidential-computing/), while [dm-verity requires an authenticated root digest](https://docs.kernel.org/admin-guide/device-mapper/verity.html); those facts define the real acceptance criteria.

**Suggested replacement:** “Do not accept the backend until the expected measurement can be independently derived and the measured chain enforces the approved workload. Reject the deployment route if the required inputs cannot be controlled or authenticated after diagnosis.”

**Architecture implication:** retain Verda as a candidate, not an established platform or a platform disproved by one failed run. Likewise, Tinfoil Containers are a candidate fallback: their [GPU access requires enablement](https://docs.tinfoil.sh/containers/overview), and durability, policy and cost still need validation.

## Other wording to tighten

- **Physical and global observers:** replace “no design helps” with “the proposed single-provider TEE/Tor design does not guarantee protection.” [TEE.fail](https://tee.fail/) demonstrates attacks under specified physical and software access, not an impossibility result for all designs. Restricting the threat model is justified; declaring all architectures helpless is not.
- **Compromise and the past:** blindness does not by itself establish retroactive anonymity or content secrecy. Retained transcripts, live conversation history, client journals, identifying prompt content and retained keys matter. Say that a later compromise does not *by itself cryptographically unblind old issuance transcripts*, under the chosen scheme's assumptions. Do not promise that it cannot identify a user.
- **AMD mitigation values:** the source check confirms removal of TCB values on 24 September 2026. The [3034 bulletin](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3034.html) still publishes BIOS/PI remediation versions, and [3030](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3030.html) still lists remediated versions. The unresolved issue is a remotely verifiable mapping to those fixes, not the absence of published fixes altogether. Keep that distinction in the opening summary.
- **Absence claims and estimates:** “no signed price table was found in the surveyed sources” is supportable; “none anywhere” is not. Provider prices, capacity and engineering-day estimates should remain dated observations or estimates. They were not reconfirmed through a logged-in console in this review.
- **Refunds and records:** identified payment administration is real. Retaining tax records is not itself a newly identified inference event, and a refund protocol may or may not join a particular spend to a purchase. Describe the actual join instead of treating every payment event as proof of inference deanonymization.

## Findings that remain supported

The review is not a rejection of the report's core cautions:

- The [current EC-ACT draft](https://datatracker.ietf.org/doc/html/draft-schlesinger-mole-ec-act-00) is preliminary and states that its intended security arguments are unfinished. Old Ristretto-crate timings are not measurements of the P-256 implementation.
- The [MOLE protocol profile](https://www.ietf.org/archive/id/draft-jms-mole-protocols-01.txt) records the refund decision before executing the protected operation. Deferred inference settlement needs separately specified semantics.
- Inference counts, token conservation, double-spend prevention, recoverability and anonymity are different properties. Passing a test for one does not prove the others.
- Atomic spent-token state and recoverable results matter. Ordinary encrypted disks and database transactions do not establish protection against a malicious infrastructure operator restoring old state.
- A CPU/GPU confidential-computing label does not make a stock tenant-administered image private from its tenant. Workload identity, administrative access and channel binding must be addressed.
- Batching is not constant-rate padding. Padding is not a defense against every timing, host or two-vantage observer.
- The architecture has not been tested end to end, and the notes contain no completed Verda GPU validation.

## What was independently checked

The workspace initially contained research documents, not an application implementation or a Git checkout. The original scratch files were located and selected numeric evidence was retained under [review_2026_11_06](../research_notes/Anonymous%20metered%20private%20inference/review_2026_11_06/README.md).

The new standard-library audit recomputed 434 accepted requests, 434 matching server preflight prompt counts, 54,079 held versus 43,589 used tokens, and 16,596 reported completion tokens from projected original records. It also confirmed that the saved Tor analysis records 44,100 emitted and received tokens over 11 completed runs, and that the saved attestation output records four matching measurements and 44 divergent controls. These are checks of recorded evidence, not new inference runs, signature verification or a new Tor experiment.

The older spike's 5,448 rejected bit flips are supported. Its detailed split of decode errors and invalid proofs differs slightly from the saved run; the previous fact-check already noted this and the final report does not repeat that split. No additional security conclusion follows from it.

This review did not re-audit all cited papers, run the proposed crypto implementation, prove unlinkability, validate cloud availability, or determine legal applicability. Unverified claims were not silently promoted to confirmed findings.
