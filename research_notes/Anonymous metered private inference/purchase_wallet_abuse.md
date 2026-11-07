# Buying credits by card, the client wallet, and the abuse and compliance implications of unlinkable prepaid credit

Survey date: 2026-11-06. This is an engineering survey that flags issues for counsel. It is not legal advice.

How to read these notes:

- **Cited Findings** were read from the named source. Sources are primary unless marked (secondary) or (search summary).
- **Inferences** are my reasoning or my own arithmetic.
- **Gaps** are unknown.
- Nothing here was tested in practice. I ran no third-party code, used no Stripe account and ran no Tor client. The only code run was my own standard-library simulation and a PDF text extractor, so spacing in PDF quotes may differ from the original.

**Premise corrections at a glance**

1. **The issuance-timing warning came from Trail of Bits, not NCC.** The substance is right, and it is stronger than the proposal suggests. Meta's response relies on reaching scale, which a PoC does not have.
2. **"The only non-private moment is buying tokens" holds cryptographically, not statistically.** At small user counts, timing and amount can rebuild the link. Refunds, EU withdrawal and VAT records also touch identity.
3. **ACT is an expired individual draft with no IETF standing.** It states that spent amounts, network metadata and timing are not protected.
4. **The PSD2 "premises of the issuer" exclusion does not cover online purchases**, per EBA Guideline 3.1.
5. **Small denominations do not limit chargeback loss.** The US dispute fee ($15, plus $15 if countered) exceeds a small purchase.
6. **LLM-Tor enforces bans with a stable pseudonym that links all of a user's requests.** Dropping that pseudonym, as the proposal does, means no per-user bans.

---

## 1. Card rail: how is a card payment bound to blind issuance, and what does Stripe learn?

### Takeaway

Stripe Checkout can mechanically bind one paid session to one blind-issuance collection without any blinded value reaching Stripe, and a sandbox needs no live approval. Live use is a separate, open question. Stripe's restricted list covers stored value and credits, and Stripe learns the buyer's full purchase identity regardless.

### Cited Findings

Versions relied on: Stripe restricted list last updated 2026-09-22; Stripe API version 2026-10-28.endive.

**What Stripe permits**

- "Nonfiat currency and stored value" is on the Restricted list. It includes "Sale of stored value or credits maintained, accepted, and issued by anyone other than the seller", and says "Seller-maintained stored value or credits may be subject to limits." It also covers "Preloaded payment cards, gift cards, virtual credits, or other products and services" "in which a monetary value is stored within the item (digital or physical)" — [Stripe restricted businesses](https://stripe.com/legal/restricted-businesses)
- Restricted means additional due diligence, case by case and revocable. Peer-to-peer money transmission is prohibited and money services businesses are restricted. No entry mentions anonymity, untraceability, VPNs or proxies — [Stripe restricted businesses](https://stripe.com/legal/restricted-businesses)
- Absent explicit approval, it is "unlikely we will be able to accept payments for you" — [Stripe support](https://support.stripe.com/questions/restricted-business-list-and-considerations)
- Stripe's Billing credits product allows prepaid credit for your own usage-based products. It prohibits gift cards and third-party payments, and says you "can't offer billing credits as stored value". The product is bound to a Stripe Customer, so it does not fit anonymous credentials, but it shows where Stripe draws the line — [Stripe billing credits](https://docs.stripe.com/billing/subscriptions/usage-based/billing-credits)

**Fulfillment mechanics**

- Webhooks are required for fulfillment (`checkout.session.completed`, `checkout.session.async_payment_succeeded`). The fulfillment function must be idempotent and concurrency-safe per Session ID, and must check `payment_status`. Payment Links use Checkout. Checkout waits up to 10 seconds for the webhook response before redirecting — [Stripe fulfillment](https://docs.stripe.com/checkout/fulfillment.md?ui=full-page)
- Session parameters include `client_reference_id` (up to 200 characters), `metadata`, `billing_address_collection`, `customer_creation`, `customer_email` and `expires_at` (30 minutes to 24 hours). Without a Customer object, email and address are still kept in `customer_details`, and guest customers are grouped in the Dashboard. If no `customer_email` is passed, customers are asked to enter one — [Stripe Checkout API](https://docs.stripe.com/api/checkout/sessions/create)

**What Stripe learns**

- Stripe collects device characteristics and activity indicators (mouse movement, time on page), uses `m.stripe.com`, and may use hCaptcha. "Stripe will continue to collect fraud detection risk factors on Stripe domains, like on Stripe Checkout payment pages." Data is retained "as long as it's useful" for fraud detection. 3DS2 device information goes to the card issuer — [Stripe advanced fraud detection](https://docs.stripe.com/disputes/prevention/advanced-fraud-detection)
- Stripe's guidance for fraud handling uses customer purchase IP, billing address, name and email — [Stripe dispute best practices](https://docs.stripe.com/disputes/best-practices)
- Mullvad's precedent: Stripe does not receive the account number, and a token is used instead. Mullvad says card and PayPal payments "are not anonymous" and "These kinds of companies log everything" — [Mullvad no-logging policy](https://mullvad.net/en/help/no-logging-data-policy)

**Tax location**

- "Stripe Tax requires your customer's location ... even if you don't have an active registration". Checkout and Payment Links use the address collected in the session. The US needs at least a postal code (country plus state is not supported), and a full address is recommended in many states. The EU works with country only, and IP is described as more reliable there — [Stripe Tax customer locations](https://docs.stripe.com/tax/customer-locations)

**Merchant of record**

- Stripe Managed Payments exists and is described as public preview on one page. Dates conflict across Stripe sources. Whether credits are eligible was not stated — [Managed Payments](https://docs.stripe.com/payments/managed-payments)

**Test mode and sandboxes**

- Sandbox payments "aren't processed by card networks or payment providers". Stripe recommends general sandboxes for new integrations. The page says a CLI command can provision an "anonymous Stripe sandbox" with "No account registration required". I report that as a documented option and did not use it — [Stripe sandboxes](https://docs.stripe.com/sandboxes)
- Test dispute cards: 4000000000000259 (synchronous fraud), 4061219946393810 (asynchronous fraud), 4000000000002685 (not received), 4000000000001976 (inquiry), 4000000000005423 (early fraud warning). 4100000000000019 is always blocked by Radar. 4000000000003220 requires 3DS. Evidence tokens `winning_evidence` and `losing_evidence` simulate outcomes. "The Stripe Services Agreement prohibits testing in live mode using real payment method details." — [Stripe testing](https://docs.stripe.com/testing)

### Inferences

**Classification risk.** Credits for your own service are mainstream. A bearer credential that literally stores value may still fall into the "virtual credits ... stored within the item" bucket, so get a written Stripe review before live mode. Nothing on the list targets anonymity as such, but the Tor-only use path may affect a human reviewer's judgment.

**Binding procedure (design inference, untested):**

1. The wallet draws a random `purchase_secret` and journals it. Over the clearnet path, it asks the issuer for a Session.
2. The issuer creates a Session with `client_reference_id = SHA-256(purchase_secret)` and returns `session.url`. The wallet opens it in the system browser, so card data goes only to Stripe.
3. The webhook marks the session payable, idempotently, per Stripe's rules.
4. The wallet collects over the same clearnet path, presenting `purchase_secret` and the blinded request. The secret never appears in browser history or at Stripe, unlike the Session ID in the redirect URL.
5. The issuer checks `payment_status` and single-collect state, then signs a fixed amount under the shared `ctx`. It stores the signed response for a stated retry window so a lost response can be re-fetched, then deletes it.
6. The wallet unblinds and verifies the issuer's proof. Trail of Bits' TOB-WAPI-2 (a server serving invalid signed tokens) is the analogue to guard against.

**What must not reach Stripe:** blinded values, wallet IDs and credential data, including in `metadata`. Stripe's knowledge of a random hash does not help it link later spends, because the hash and the blinded request are independent.

**Retained after issuance (operator):** Session ID, amount, coarse time, card fingerprint, collected flag, refund state.

**Payment Links or API-created Sessions.** API-created Sessions fit better because the issuer controls price, reference and 3DS options. Payment Links suit a demo with a very small server.

**A USD sandbox PoC avoids live approval, money movement and real disputes.** It does not show that Stripe would approve the product.

**What deletion cannot do.** Operator-side deletion does not remove Stripe's records. The purchase is identified by design, and privacy depends entirely on breaking the link afterward.

**Merchant of record.** This might shift VAT and consumer-law handling, but it adds a third party and has its own acceptance rules (not researched).

### Gaps

- Paddle, Lemon Squeezy and other merchant-of-record acceptable-use terms were not retrieved.
- Whether Managed Payments accepts prepaid credits is unknown.
- No published Stripe ruling on bearer credit exists. Only a written review can settle it.
- Whether a Payment Link accepts a preset `client_reference_id` was not checked.
- What activation, if any, a dashboard-created sandbox needs was not checked beyond the sandbox page.

---

## 2. Linkage at the purchase and spend boundary

### Takeaway

Blind signatures hide the cryptographic link, but timing and amount can rebuild it. In my model, the operator's best guess is right with probability 1/(1+k), where k is the number of other purchases during the mean first-spend delay. A PoC with tens of users therefore has little or no unlinkability from the operator, whatever the cryptography.

### Cited Findings

**Source of the timing warning (premise correction)**

- Trail of Bits' report (August 26, 2025, with fix review): "It is possible that identifying information about the client could be leaked by correlating the time between anonymous credential issuance and back-end TEE requests; the WhatsApp client implements prefetching of anonymous credentials to mitigate this risk." — [Trail of Bits report PDF copy](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf); [Trail of Bits library entry, not opened](https://trailofbits.com/library/meta-whatsapp-private-processing/)
- Prefetching still leaves a signal. TOB-WAPI-4 (Informational, Unresolved) says "there is a reasonably consistent delta between an ACS token prefetch and the newly signed token's first use", which an insider with edge logs could match — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- Meta's response: "we intend to update how ACS tokens are fetched and used to reduce correlation between first fetch and first use" and "We assess timing attacks of this sort to be difficult to achieve as our product reaches complete rollout and a steady-state QPS." — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- TOB-WAPI-6 says metrics such as device type, country calling code and latency could be combined with "ACS token redemption timing and gateway location" by an insider — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- NCC's report (27 August 2025) does not contain this finding in my text search. It does warn that "a colluding or compromised Fastly-operated Relay and Meta-operated Gateway could analyze timing, volume, and frequency patterns of requests to link them to individual users' IP addresses" and suggests "additional padding, batching, or carefully randomized request timing" — [NCC report](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf)
- A secondary article reports the Trail of Bits timing concern, but says both that Meta resolved "most" issues and that "All identified issues were addressed". The report itself says 16 resolved, 4 partially resolved, 8 unresolved — [CyberInsider (secondary)](https://cyberinsider.com/whatsapp-private-inference-pre-launch-audit-uncovered-critical-flaws/); [Trail of Bits blog](https://blog.trailofbits.com/2026/04/07/what-we-learned-about-tee-security-from-auditing-whatsapps-private-inference/)

**Key and configuration partitioning**

- TOB-WAPI-1 (Medium, Resolved): the server could serve different public keys per client, making redemptions linkable. Meta's fix fetches keys through Fastly, signed by a key hard-coded in the client — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- The same finding warns that overlapping key validity "could introduce de-anonymization vectors, as targeted clients may be denied access to new configurations and identified by their use of older but still valid configurations". It adds that attributes "should be chosen by the client", with expiry aligned to on-the-hour or at-midnight timestamps — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- "The number of active Issuer configurations also contributes to anonymity set partitioning." and "key rotations represent a trade-off between Client privacy and Issuer security." — [RFC 9576 §6.2](https://www.rfc-editor.org/rfc/rfc9576.html)

**Protocol statements**

- "Side-channel attacks, such as those based on timing correlation, could be used to reduce anonymity set size." (§6.3) "Cached tokens help improve privacy by separating the time of token issuance from the time of token redemption" (§7.1) "Collusion can enable linking of attestation, issuance, and redemption contexts." (§4) — [RFC 9576](https://www.rfc-editor.org/rfc/rfc9576.html)
- ACT draft -01 (revision dated 2026-02-13, page updated 2026-08-17, expired individual draft) lists as not provided: "The spent amount s is revealed to the issuer."; "IP addresses and network metadata can still link transactions."; "Transaction timing patterns could potentially be used for correlation." It says issuers "MAY add random delays and amount obfuscation where possible", and that deployments needing full unlinkability "MUST use a shared ctx across all clients within the same context" — [ACT draft](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01); [status page](https://datatracker.ietf.org/doc/draft-schlesinger-cfrg-act/)

**Precedent**

- Kagi: "The more users do so, the lower the probability that the server can guess it was you specifically". A per-request settings configuration "would automatically lose redemption-redemption unlinkability guarantees if approximately 1000 Kagi users used Privacy Pass". Kagi "highly recommend[s]" separating generation and redemption in time or in space, with no window stated — [Kagi blog](https://blog.kagi.com/kagi-privacy-pass)
- Amount plus timing has broken strong cryptography before. In Zcash, a unique value deposited then withdrawn soon after linked 12,841 unique values, 28.5% of coins ever deposited. A 25-minute window captured 70% of that value and one hour captured 83%. Most linked coins (87%) came from founders or miners, so this is an unusual population — [Kappos et al., USENIX Security 2018](https://www.usenix.org/system/files/conference/usenixsecurity18/sec18-kappos.pdf)
- Cashu has one mint key per amount, and an amount is sent with each blinded secret, so the mint sees amounts — [Cashu NUT-00](https://raw.githubusercontent.com/cashubtc/nuts/main/00.md)

**Literature**

- New users make existing users more secure, even if they never interact, and usability therefore affects anonymity — [Dingledine and Mathewson, Anonymity Loves Company](https://www.freehaven.net/anonbib/cache/oreilly-usability.pdf) (search summary)
- An active attacker can hold back a target and flood a batch with its own messages (the n-1 attack), so batch anonymity can be destroyed by whoever controls the traffic — [Serjantov, Dingledine and Syverson, From a Trickle to a Flood](https://apps.dtic.mil/sti/pdfs/ADA465475.pdf) (search summary)

### Inferences

#### Simulation (own calculation, not empirical)

Script: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/purchase_anonymity_set.py`, run with `python3 -I`.

Assumptions:

- N users each buy once per 30 days, with Poisson arrivals.
- First spend follows purchase after a random delay with a known distribution.
- The adversary uses only timing and denomination, and sees no network or content signal. Any real fingerprint makes results worse.

**Rule of thumb.** With exponential delay, the best guess is the most recent purchase, and it is correct with probability 1/(1+k), where k = other purchases/day × mean delay. The effective set size is about e·k for large k. The simulation matched this closed form (for example 0.107 predicted and 0.103 simulated at N=1,000, 6 hours).

For a 10% or lower best-guess rate you need k ≥ 9. For monthly buyers, that means N × mean delay in days ≥ 270.

**Table A. Worst case: the adversary can recognise a first spend.** Cells are best-guess accuracy / effective set size.

| N users | Purchases/day | 5 min | 1 h | 6 h | 24 h | 72 h | 7 d |
|---|---|---|---|---|---|---|---|
| 1 | 0.03 | 1.00 / 1.0 | 1.00 / 1.0 | 1.00 / 1.0 | 1.00 / 1.0 | 1.00 / 1.0 | 1.00 / 1.0 |
| 10 | 0.33 | 1.00 / 1.0 | 0.99 / 1.0 | 0.93 / 1.2 | 0.78 / 1.7 | 0.53 / 3.2 | 0.32 / 6.4 |
| 100 | 3.33 | 0.99 / 1.0 | 0.88 / 1.3 | 0.54 / 3.0 | 0.23 / 9.7 | 0.09 / 27.5 | 0.04 / 63.1 |
| 1,000 | 33.33 | 0.89 / 1.3 | 0.43 / 4.5 | 0.12 / 23.3 | 0.03 / 91.2 | 0.01 / 271.4 | <0.01 / 631.4 |
| 10,000 | 333.33 | 0.46 / 3.9 | 0.07 / 38.3 | 0.01 / 226.9 | <0.01 / 903.3 | <0.01 / 2,708.6 | <0.01 / 6,322.4 |
| 100,000 | 3,333.33 | 0.08 / 32.1 | <0.01 / 376.1 | <0.01 / 2,261.7 | <0.01 / 9,019.1 | <0.01 / 27,104.6 | <0.01 / 63,222.2 |

**Table B. Visible denomination classes, 24-hour mean delay, equal popularity.** Cells are best-guess accuracy / effective set size.

| N users | 1 class | 3 classes | 10 classes | 100 classes |
|---|---|---|---|---|
| 10 | 0.77 / 1.7 | 0.91 / 1.2 | 0.98 / 1.1 | 1.00 / 1.0 |
| 100 | 0.24 / 9.7 | 0.47 / 3.8 | 0.76 / 1.8 | 0.97 / 1.1 |
| 1,000 | 0.03 / 90.9 | 0.09 / 30.7 | 0.23 / 9.8 | 0.75 / 1.8 |
| 10,000 | <0.01 / 902.5 | 0.01 / 302.2 | 0.03 / 91.2 | 0.23 / 9.7 |
| 100,000 | <0.01 / 9,033.5 | <0.01 / 3,010.9 | <0.01 / 903.8 | 0.03 / 91.0 |

Free-form amounts behave like the 100-class column or worse, because a rare amount is its own class.

**Table C. First spends cannot be recognised.** Other users' sessions arrive at 2 per user per day and look the same. Cells are mean posterior on the true buyer / share of sessions with posterior ≥ 0.5.

| N users | 5 min | 1 h | 24 h |
|---|---|---|---|
| 10 | 0.81 / 0.93 | 0.34 / 0.16 | 0.02 / 0.00 |
| 100 | 0.38 / 0.30 | 0.05 / 0.00 | <0.01 / 0.00 |
| 1,000 | 0.06 / 0.00 | 0.01 / 0.00 | <0.01 / 0.00 |
| 10,000 | 0.01 / 0.00 | <0.01 / 0.00 | <0.01 / 0.00 |

Hiding the first-spend marker is worth roughly an order of magnitude in users, but this depends heavily on the assumed session rate. The model is a single-event approximation.

**Table D. Client-side uniform delay on [0, w], first spend recognisable.** Cells are best-guess accuracy / effective set size.

| N users | w=1 h | w=6 h | w=24 h | w=72 h |
|---|---|---|---|---|
| 10 | 0.99 / 1.0 | 0.96 / 1.1 | 0.86 / 1.2 | 0.66 / 1.7 |
| 100 | 0.93 / 1.1 | 0.69 / 1.6 | 0.29 / 3.9 | 0.10 / 10.4 |
| 1,000 | 0.55 / 2.1 | 0.12 / 8.9 | 0.03 / 33.9 | 0.01 / 100.4 |
| 10,000 | 0.07 / 14.4 | 0.01 / 83.8 | <0.01 / 333.5 | <0.01 / 1,000.5 |

**Table E. Window needed for k other purchases (monthly buyers), w = 30k/N days.**

| N users | k=10 | k=100 |
|---|---|---|
| 10 | 30 d | 300 d |
| 100 | 3 d | 30 d |
| 1,000 | 7.2 h | 3 d |
| 10,000 | 43 min | 7.2 h |
| 100,000 | 4 min | 43 min |

**Table F. Intersection with any stable fingerprint across m purchases.** Expected innocent survivors are N(k/N)^m.

| N | k per window | m=1 | m=2 | m=3 |
|---|---|---|---|---|
| 100 | 10 | 10 | 1 | 0.1 |
| 1,000 | 10 | 10 | 0.1 | <0.01 |
| 1,000 | 100 | 100 | 10 | 1 |
| 10,000 | 100 | 100 | 1 | 0.01 |
| 100,000 | 1,000 | 1,000 | 10 | 0.1 |

A fingerprint that survives across purchases collapses the set after two or three purchases, which is why no wallet ID, stable `ctx` or configuration quirk may cross.

#### Reading the results

- **A one-user or tens-of-users PoC gives no meaningful unlinkability from the operator.** Content privacy from the enclave is a separate property and remains. PoC claims should say this explicitly.
- **Meta's defence rests on scale.** Its "steady-state QPS" argument assumes a volume the PoC will not have.
- **The operator holds both logs.** With issuer and gateway under one operator, this is the joint deployment RFC 9576 warns about. Moving spend verification inside an attested enclave would reduce what the operator logs, but arrival times at the gateway remain (cross-track).

#### Mitigations, in rough order of value for cost

1. **Buy ahead of need.** A new credential sits in reserve, and the wallet keeps spending the old one until it runs out. The delay becomes days or weeks at no UX cost, and a low-balance top-up can be randomised. This is the cheapest strong lever.
2. **Client-side random delay** when no reserve exists, with a clear UX choice. A user who buys and uses immediately is much more exposed than the table average suggests.
3. **Two or three fixed denominations**, with no custom amounts. Use one issuer key and one `ctx` across them, with the amount hidden inside the credential after issuance.
4. **No first-spend marker.** No welcome flow, distinct endpoint or per-purchase `ctx` should distinguish a first spend.
5. **One key configuration for everyone**, hard-coded in the client release as Trail of Bits recommends. Prefer per-release rotation to overlapping keys.

#### Mitigations that do not work well

- **Issuer-side batching.** It hides issuance time but not the Stripe payment time. The issuer can also delay other buyers, so a batch can be an n-1 set, and clients cannot verify batch size.
- **Operator self-seeding** with dummy purchases. It may confuse a third party but not the operator, which knows which purchases are its own.

#### Other leak paths

- **A spend amount reveals a lower bound on balance.** A large spend excludes buyers of smaller denominations.
- **A smaller-than-usual ceiling signals a nearly empty credential.** If a purchase follows soon after, that signal can link old and new credentials. A wallet that tops up before the balance falls below the ceiling avoids it (cross-track with the crypto track's pre-authorisation design).
- **Exact per-request `s` and `t` values can themselves fingerprint a conversation** if the operator sees them (cross-track).

### Gaps

- No public data exists on real purchase-to-first-use delay distributions for prepaid AI credit.
- Whether Meta has since implemented its stated change is unknown. TOB-WAPI-4 was still Unresolved in the fix review.
- The two mix papers were read only through search excerpts.
- ACT does not quantify an anonymity set.
- Whether first spends are recognisable in practice (for example through session gaps) can only be measured.

---

## 3. Client path separation: what must not cross from purchase to spend

### Takeaway

Separation is a client engineering property that can be tested with a packet capture. It needs two network stacks with no shared state, plus Tor isolation for each spend context, and any stable wallet identifier or telemetry on the Tor path undoes it.

### Cited Findings

- Arti: streams with different `IsolationToken`s never share a circuit. `StreamPrefs` supports isolation groups, and `TorClient::isolated_client` is available — [IsolationToken](https://docs.rs/arti-client/latest/arti_client/isolation/struct.IsolationToken.html); [StreamPrefs](https://docs.rs/arti-client/latest/arti_client/struct.StreamPrefs.html); [arti-client 0.48.0 docs](https://docs.rs/arti-client/latest/arti_client/)
- "Clients SHOULD NOT reuse a ticket for multiple connections." "Reuse of a ticket allows passive observers to correlate different connections." — [RFC 8446 Appendix C.4](https://www.rfc-editor.org/rfc/rfc8446.html)
- Kagi uses a separate domain, kagisuggest.com, because it "prevents browsers from automatically sending your Kagi login cookies", and turns off personalization because it would "increase the likelihood of deanonymization" — [Kagi help](https://help.kagi.com/kagi/privacy/privacy-pass.html)
- LLM-Tor: "Redemption occurs exclusively over Tor"; the desktop client uses exit nodes, with onion service planned — [LLM-Tor whitepaper](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- TOB-WAPI-5 is "Sensitive inference-related data can be logged client-side" (Low, Unresolved). TOB-WAPI-6 advises minimizing "user-identified timing, location, and latency data" — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- NCC warned that serving different HPKE key configurations could differentiate users, and said key configurations must not be "unique or infrequently shared" — [NCC report](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf)
- As an analogue for clock leaks: clock skew can link a service reached by direct and anonymous paths, because remote timestamps expose a device-specific skew — [Murdoch, Hot or Not](https://murdoch.is/papers/ccs06hotornot.pdf) (search summary)

### Inferences

| Item | Rule | Basis |
|---|---|---|
| IP address | Spend path only through Tor. Purchase path never through the same circuit or client. | Design, RFC 9576 §4 |
| Cookies and web storage | None on the Tor path. No embedded webview shares a browser profile with Checkout. Use the system browser for Stripe. | Kagi precedent |
| TLS session tickets, connection pools | Separate HTTP client instances and no resumption across paths or spend contexts. | RFC 8446 C.4 |
| DNS | Purchase path may use the normal resolver, which sees the purchase domain. Spend path resolves only through Tor or uses an onion address. | Inference; no capture done |
| Wallet or device ID, install ID | Must not exist on the Tor path. Avoid generating one at all. | Table F |
| Telemetry, crash reports, update checks | None on the Tor path. Update checks must not correlate with spend timing. | TOB-WAPI-5, TOB-WAPI-6 analogues |
| Timestamps | No client timestamps in requests, or round them coarsely. Expiry aligned to whole hours or days. | TOB-WAPI-1 recommendation; Murdoch as analogue |
| User-agent, SDK version, TLS fingerprint | One fixed client profile. Every visible version is a partition. | RFC 9576 §6.2 reasoning |
| Key configuration | Same issuer and gateway key configuration for all. Pinned in the release. | TOB-WAPI-1, TOB-WAPI-3, NCC |
| `ctx` | Shared, never per purchase. | ACT §6 |
| Model selection, personalization | Treat rare settings as partitions. Prefer a small model list. | Kagi |
| Purchase-derived data | `purchase_secret`, Session ID and receipts stay in the purchase store and are never sent on the spend path. | Design |

**Isolation granularity.** Per-request isolation is the strongest but costs circuit build time. Isolating per conversation leaves requests in that conversation linkable only to each other. The choice belongs to the UX and crypto tracks.

**Same device, same time.** Local malware or a shared OS resolver is out of scope for this kind of separation.

### Gaps

- The TLS fingerprint of the planned Rust HTTP stack over Arti was not examined.
- Whether the system browser's Stripe session can be correlated with the wallet beyond timing has not been tested.
- No packet capture was run.

---

## 4. Chargebacks and fraud

### Takeaway

The operator cannot revoke what a stolen card bought, and disputes run for 120 days or more, so controls must act at purchase and limit exposure. A hold long enough to outlast disputes is not viable, and the dispute fee means small denominations reduce the loss only down to a floor.

### Cited Findings

**Timing and cost**

- Disputes are typically allowed "within 120 days of the original payment", with more time in some situations. "when a customer pays for a future event or service ... the dispute window starts on the event date, not the payment date". Response is usually due in 7-21 days, and the lifecycle can take 2-3 months. In rare cases one payment can receive more than one dispute — [Stripe how disputes work](https://docs.stripe.com/disputes/how-disputes-work)
- Dispute fees: US $15 received and $15 countered; euro-zone €20 and €20. The received fee is not returned; the countered fee is returned on a win — [Stripe dispute pricing](https://support.stripe.com/questions/june-2025-pricing-updates-for-disputes); [Stripe how disputes work](https://docs.stripe.com/disputes/how-disputes-work)

**Early fraud warnings (EFW)**

- "about 40% of Visa and Mastercard EFWs become fraud disputes" if not refunded. Refunding is optimal on charges "roughly less than or equal to your dispute fee", and "likely not worthwhile" above 35% more than the fee. A refund prevents a fraud report only as a reversal, "which usually happens within 2 hours of the payment capture" — [Stripe how disputes work](https://docs.stripe.com/disputes/how-disputes-work)
- "Issuers are required to report possible fraud for a captured payment, even if it gets refunded, but aren't required to report it for a payment authorization." A reversed authorization is not reported — [Stripe monitoring programs](https://docs.stripe.com/disputes/monitoring-programs)

**Evidence**

- "If your business provides digital goods, include evidence such as an IP address or system log proving the customer downloaded the content or used your software or service." Fraudulent disputes are over half of all disputes — [Stripe dispute best practices](https://docs.stripe.com/disputes/best-practices)
- Visa Compelling Evidence 3.0 applies to reason code 10.4. It needs two prior transactions 120-365 days earlier with the same payment credential and no fraud reported, with at least one match across email, delivery address, device ID or fingerprint, or IP — [Stripe CE 3.0](https://support.stripe.com/questions/how-does-stripe-support-visa-compelling-evidence-3-0)

**3D Secure**

- `payment_method_options[card][request_three_d_secure]` can be `any` or `challenge` on Checkout Sessions. Liability shift is expected for eligible fraud disputes but is not guaranteed and does not cover product-not-received. "Visa doesn't support liability shift for businesses engaging in ... stored-value card purchase or load" — [Stripe 3DS](https://docs.stripe.com/payments/3d-secure/authentication-flow)

**Monitoring programs**

- Visa VAMP non-compliant thresholds: count 5 and ratio 0.5%. Excessive: 1,500 and 1.5% in the EU and US. TC15 disputes and TC40 fraud reports are both counted, and a transaction in both counts twice. TC40s qualifying for CE 3.0 and disputes resolved by pre-dispute products are excluded. Refunds and dispute outcomes do not reduce the count. Enumeration excessive: 300,000 and 20% — [Stripe monitoring programs](https://docs.stripe.com/disputes/monitoring-programs)
- Mastercard ECM: 100-299 chargebacks and 1.5-2.99%. HECM: 300+ and 3%. EFM needs at least 1,000 e-commerce payments, more than $50,000 net fraud volume, more than 0.50% fraud rate, and low 3DS use — [Stripe monitoring programs](https://docs.stripe.com/disputes/monitoring-programs)
- From October 2026, Visa MERP in Asia Pacific targets recently launched accounts, with possible backdated fraud disputes and unpublished thresholds. Noted in case of APAC sales — [Stripe monitoring programs](https://docs.stripe.com/disputes/monitoring-programs)

**Card testing**

- Checkout has rate limiters, AI and CAPTCHA protections. Low-value payments and guest checkout are targets. Velocity rules need a Radar plan — [Stripe card testing](https://docs.stripe.com/disputes/prevention/card-testing)

**Precedents**

- Mullvad deletes the Stripe charge ID and the time portion after 20 days, and payment tokens expire after 20 days (cash 120). Accounting retention applies, for example "seven years from the end of the fiscal year". Chargebacks are not addressed — [Mullvad no-logging policy](https://mullvad.net/en/help/no-logging-data-policy). Its terms give a 14-day money-back guarantee, no refunds for cash and crypto, and say the account number "is the only confirmation to prove you have the right to use the service" — [Mullvad terms](https://mullvad.net/en/help/terms-service)
- Kagi: token generation needs an active plan, with a monthly limit (2,000 per its blog, 3,000 per its help page, which I presume is newer). Tokens from month X expire at the start of month X+2. Chargebacks are not mentioned — [Kagi blog](https://blog.kagi.com/kagi-privacy-pass); [Kagi help](https://help.kagi.com/kagi/privacy/privacy-pass.html)
- Brave: a pool of SKU credentials, each with its own 24-hour window, considered "spent" after presentation. No official chargeback documentation found — [Brave issue 29345](https://github.com/brave/brave-browser/issues/29345); [Brave support](https://support.brave.app/hc/id/articles/25362740009613-How-do-I-use-Brave-Leo-iOS)
- LLM-Tor: "Server records credit balance linked to user identity", then issues blind tokens. Refunds are not mentioned — [LLM-Tor whitepaper](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- GNU Taler: recoup credits the original reserve, and customers should keep reserve keys. This is the closest analogue found to refunding unspent anonymous value — [Taler recoup API](https://docs.taler.net/core/exchange/post-recoup-withdraw.html)

### Inferences

**The precedents do not transfer directly.** Kagi and Brave limit blast radius through a subscription or account, because they can stop issuing when a card fails. Prepaid bearer credit with long validity has no such lever once issued.

**Fee arithmetic (own calculation; processing fees excluded).** A lost $10 fraud dispute costs $10 + $15 = $25, or $40 if countered and lost, plus any compute consumed. Lower denominations reduce the principal but not the fee, so the loss per dispute cannot fall below $15.

**Refund arithmetic.** Stripe's guidance favours refunding EFWs up to about $15-20 in the US. A refund does not take back bearer credit, so a stolen-card buyer keeps the credit and the operator pays for the compute.

**Small merchants trip thresholds easily.** Three fraud-disputed payments in a month of 600 payments, each appearing in both TC15 and TC40, is a VAMP count of 6 and a ratio of 1.0% (own arithmetic). Both exceed the non-compliant thresholds, though Visa only "may" assess fees there.

**A "future service" clock is a risk.** If prepaid credit counts as a future service, the 120 days could start later than the payment. Unknown; counsel or Stripe should confirm.

**Controls compared**

| Control | Effect | Privacy cost | View |
|---|---|---|---|
| Authorize, review, then capture before issuing | Reversal avoids a fraud report and gives a short review window | None beyond purchase | Good. Also adds delay. Checkout support for this was not verified. |
| Require 3DS (`any` or `challenge`) | Liability shift where eligible, and it deters testers | None beyond Stripe and the issuer | Good, but Visa's stored-value exclusion may remove the shift. Ask Stripe. |
| Per-card and per-period purchase caps | Limits loss from one card | Operator keeps a card fingerprint on the identified side | Good. Needs verified API fields and possibly a Radar plan. |
| Low maximum purchase | Limits loss per card | None | Good. Does not reduce the $15 fee floor. |
| Short hold before credit is usable (hours or days) | Catches some early signals, and also helps linkage | Delay | Useful as a dual-purpose control. EFW timing data was not found. |
| 120-day hold | Would cover the normal window | Severe delay | Not viable. |
| Short credential expiry | Limits the value of stolen credit | Epochs partition users, and Reg E may conflict | Conditional on counsel. |
| Emergency key revocation | Stops all outstanding credit after a mass fraud event | Punishes everyone, and migration partitions users | Last resort only. |
| Reserve | Absorbs losses | None | Size unknown; no fraud rate data found. |
| Purchase-side dispute evidence | Issuance log, email, IP, and CE 3.0 for repeat buyers | Requires keeping purchase records 120+ days | Feasible, but it cannot show usage. |

**The evidence conflict is real but partial.** Stripe wants proof of use, which an unlinkable design cannot give. Purchase-side records can still show the credential was delivered. CE 3.0 is built on purchase data rather than usage, but it only applies to repeat Visa buyers. This requires retaining purchase records longer than Mullvad's 20-day model.

**Refund of unused balance (design sketch, no precedent found for a card-funded system).**

1. The wallet spends the remainder to a burn endpoint over the anonymous path and receives a one-time signed voucher.
2. After a delay, it presents the voucher plus `purchase_secret` over the clearnet path.
3. The operator refunds at most the purchase amount, to the original card only, and marks the purchase refunded.

Consequences:

- The operator learns that buyer's total consumption, but earlier per-request spends stay unlinked if the protocol holds.
- A chargeback after refund must be handled.
- If a credential was transferred or combined, this breaks down.
- The crypto track should review it.

### Gaps

- EFW and TC40 latency distributions were not found.
- No fraud or chargeback rates for prepaid AI credit were found.
- Kagi, Mullvad and Brave publish no chargeback handling that I could find.
- CE 3.0 on Checkout digital goods has not been tested.
- Manual capture on Checkout and card-fingerprint availability in the webhook were not verified.

---

## 5. Regulatory flags (for counsel)

### Takeaway

On their text, credits sold by one seller for its own service look closest to closed-loop prepaid in the US and a two-party purchase in the EU. Transfer, cash-out, third-party acceptance and gift marketing move them toward regulated activity. A USD sandbox PoC avoids these rules. Real money needs counsel first.

### Cited Findings

**US: FinCEN**

- 31 CFR 1010.100 defines prepaid access at (ww) and closed-loop prepaid access at (kkk), meaning use with a "defined merchant or location". Paragraph (ff)(4)(iii)(A) excludes closed-loop programs up to $2,000 maximum value per device or vehicle per day. The no-international, no-transfer and no-non-depository conditions attach only to the $1,000 item at (D). Sellers of prepaid access are covered at (ff)(7), including access usable before identification or more than $10,000 per person per day without policies. Funds "only integral to the sale of goods or the provision of services" are addressed at (ff)(5)(ii)(F), a facts-and-circumstances test — [31 CFR 1010.100](https://www.law.cornell.edu/cfr/text/31/1010.100)
- FIN-2013-G001 (18 March 2013): convertible virtual currency "either has an equivalent value in real currency, or acts as a substitute". An administrator issues it with authority to redeem and is a money transmitter "to the extent that it allows transfers of value". Prepaid access is "limited to real currencies" — [FinCEN guidance](https://www.fincen.gov/resources/statutes-regulations/guidance/application-fincens-regulations-persons-administering)
- ACT §3.4 notes that a spend with s=0 can be used "for securely transferring a token to another party" — [ACT draft](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01)

**US: Reg E gift-card rules (CARD Act)**

- Section 1005.20 excludes cards that are "Reloadable and not marketed or labeled as a gift card" at (b)(2). Expiry may not be less than five years after issuance or last load at (e)(2). Dormancy fees are limited at (d) — [Reg E §1005.20](https://www.consumerfinance.gov/rules-policy/regulations/1005/20/)
- Comment 20(a)-3: cards redeemable for specific goods or services "generally are not subject to the requirements of this section", but a card "issued in a specified or denominated amount" (a certificate "up to $50") or stating a monetary value ("a $50 value") is covered unless excluded. Comment 20(e)-13: the five-year minimum continues to apply to remaining funds after partial use. Points are not mentioned — [Reg E interpretation](https://www.consumerfinance.gov/rules-policy/regulations/1005/interp-20/)
- OpenAI and Anthropic API credits have one-year expiry and are non-refundable per their pages. These are account-based API products, so they are a weak precedent — [OpenAI prepaid billing](https://help.openai.com/en/articles/8264644-how-does-prepaid-billing-work); [Anthropic API payment](https://support.claude.com/en/articles/8977456-how-do-i-pay-for-my-api-usage); [Anthropic credit terms](https://www.anthropic.com/legal/credit-terms) (snippets)

**EU: payments and e-money**

- PSD2 Art 3(k)(i) excludes instruments usable only in the premises of the issuer or a limited network under direct commercial agreement; (ii) covers a very limited range of goods or services. Art 37(2) requires notification above EUR 1 million over 12 months. Recitals 13-14 address specific-purpose instruments evolving into general-purpose ones — [PSD2](https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32015L2366)
- EBA/GL/2022/02 Guideline 3.1: instruments under the premises exclusion "can only be used in physical premises and cannot be used in online stores". Guideline 2.3: a limited network can be online stores only. Guideline 2.5: apply restrictively, so a specific-purpose instrument cannot "develop into a general-purpose payment instrument". Guideline 4.1: limited range requires a functional connection between goods or services. Guideline 4.4 lists volume, maximum credit, instrument count and customer risk as indicators — [EBA final report copy](https://www.finanssivalvonta.fi/contentassets/fe9063c85d0a4a48bd8a155055c99c43/2022_2_en_final-report.pdf); [EBA announcement](https://www.eba.europa.eu/eba-publishes-final-guidelines-limited-network-exclusion-under-payment-services-directive)
- EMD2 Art 2(2): e-money is value "accepted by a natural or legal person other than the electronic money issuer". Art 11(2): redeem at par at any moment — [EMD2](https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32009L0110)
- PSD3 and PSR: final compromise texts were published around 23-24 April 2026. Parliament's ECON committee approved them on 5 May 2026, and Council first-reading material includes a document dated 21 October 2026. I did not confirm Official Journal publication. Application is about 18 months after entry into force per some sources, while PSD3 has 21 months for transposition per another; these may refer to different instruments. A 2025 article calls limited-network changes "fairly minor" but predates the final text — [Arthur Cox (secondary)](https://www.arthurcox.com/insights/psd3-and-psr-final-compromise-texts-published/); [Council document](https://data.consilium.europa.eu/doc/document/ST-8535-2026-ADD-1/en/pdf); [Dittmar (secondary)](https://www.dittmar.fi/news/psd3-and-psr-proposals-close-to-adoption/); [Worldline (secondary)](https://worldline.com/en/home/main-navigation/resources/blogs/2026/the-scope-and-timeline-are-locked-in-for-psd3-and-psr-what-should-psps-know); [Simmons & Simmons (secondary)](https://www.simmons-simmons.com/en/publications/cmanzeyn1009cvch0q9u61uva/relevant-exclusions-under-the-psd3-psr)

**EU: anti-money-laundering**

- AMLR 2024/1624 Art 19(7): supervisors may exempt e-money if non-reloadable and at most EUR 150, usable only for the issuer's or network's goods and services, not exchangeable for cash or crypto, and monitored — [AMLR Art 19](https://www.springlex.eu/en/packages/aml/amlr-regulation/article-19/)
- Art 79 bans anonymous accounts at credit and financial institutions and crypto-asset service providers, and bars acquirers from accepting anonymous prepaid cards issued in third countries — [AMLR Art 79](https://www.springlex.eu/en/packages/aml/amlr-regulation/article-79/)
- Art 3's list of obliged entities does not name merchants selling their own services. Application from 10 July 2027 is per a practitioner source — [AMLR Art 3](https://www.springlex.eu/en/packages/aml/amlr-regulation/article-3/); [Didit (secondary)](https://didit.me/ar/blog/amlr-checklist-by-obliged-entity/)

**EU: consumer rights**

- Consumer Rights Directive: 14-day withdrawal (Art 9); proportionate payment for service performed (Art 14(3)); exemptions for services fully performed with consent (Art 16(a)) and digital content with consent and acknowledgement (Art 16(m)) — [CRD](https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32011L0083)
- A withdrawal-function requirement (Art 11a, via Directive 2023/2673) applies from 19 June 2026, per secondary sources. I did not read the primary text — [iubenda (secondary)](https://www.iubenda.com/en/blog/online-withdrawal-function-eu-directive-2023-2673/); [Mondaq (secondary)](https://www.mondaq.com/ireland/consumer-law/1804286/eu-introduces-mandatory-withdrawal-button-for-online-consumer-contracts)

**EU: VAT**

- Voucher Directive 2016/1065 Art 30a: a single-purpose voucher (SPV) is one where place of supply and VAT due are "known at the time of issue"; otherwise it is multi-purpose (MPV). Art 30b: SPV transfers are taxed as the supply; MPVs are taxed at redemption — [Directive 2016/1065](https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32016L1065)
- Reg 282/2011 (consolidated 14 April 2025) Art 24b(d) and 24f: two non-contradictory pieces of location evidence, such as billing address, IP or bank details. Art 24d covers rebuttal with three — [Reg 282/2011](https://eur-lex.europa.eu/eli/reg_impl/2011/282)
- OSS records "must be kept for 10 years from the end of the year in which the transaction was made", including "information used to determine the place where the customer is established", available electronically on request — [EU OSS record keeping](https://vat-one-stop-shop.ec.europa.eu/one-stop-shop/record-keeping-and-audits-oss_en)
- A non-Union OSS scheme covers B2C services by non-EU sellers — [Irish Revenue](https://revenue.ie/en/tax-professionals/tdm/value-added-tax/part10-special-schemes/vat-ecommerce-rules/non-union-scheme-one-stop-shop-oss.pdf)

**Sanctions**

- OFAC's Kraken settlement (28 November 2022): $362,158.70 for 826 transactions, with geolocation at onboarding only and failure to "implement IP address blocking on transactional activity" — [OFAC](https://ofac.treasury.gov/media/929541/download); [Orrick (secondary)](https://infobytes.orrick.com/2022-12-02/ofac-settles-virtual-currency-exchange-resolve-ip-address-screening-deficiencies/)

**Precedent for general-purpose anonymous value**

- GNU Taler operates through Taler Operations AG under a Swiss self-regulatory body (VQF) for CHF e-money, with customers anonymous and merchants accountable. A German entity was reported as pending a BaFin licence (secondary) — [Taler terms](https://git.taler.net/taler/exchange/file/contrib/exchange-tos-tops-v0.en.rst.html); [Taler exchange manual](https://docs.taler.net/taler-exchange-manual.html); [Zeli (secondary)](https://zeli.app/story/50197404)

### Inferences (not legal conclusions)

**Design features that change the analysis**

| Feature | Direction | PoC position |
|---|---|---|
| Single seller, own service only | Toward closed loop, and not e-money under EMD2's "other than the issuer" element | Keep. Operator remains seller of record and buys from Verda, Tinfoil or others as its own suppliers. |
| Third parties accepting the same credits | Toward limited-network or e-money questions, with a EUR 1 million notification | Avoid. Modular backends are fine if users only ever buy from the operator. |
| Person-to-person transfer | Toward value transfer or convertible virtual currency questions | Do not offer it. ACT technically allows it, and bearer credentials can be handed over anyway. Terms alone may not settle this. |
| Cash-out | Toward money transmission | None. Refund only to the original card. |
| Marketing as a gift | Toward Reg E gift-certificate rules | Do not market as gifts. |
| Monetary labelling ("$10 of credit") | Toward "specified amount" under comment 20(a)-3 | Counsel. Token units may help but are not clearly safe. |
| Credential expiry under 5 years | Possible conflict with Reg E | Counsel. Epoch rotation would need a migration path. |
| Non-reloadable anonymous purchase | Each purchase is a new instrument, so the "reloadable" exclusion may not apply | Counsel. |
| EU consumers | Withdrawal rights and VAT | Avoid EU sales in the first live phase, or get advice first. |

**US consumer status.** An anonymous buyer cannot be shown to be a business, so assume consumer status.

**The "premises" route is closed.** EBA Guideline 3.1 rules it out for online use. The better argument may be that a two-party sale of one's own service is not a payment service at all. That is a question for counsel.

**VAT forces identity data at purchase.** Credits need to be taxed at purchase, as with an SPV, because MPV taxation at redemption does not work if redemption is anonymous. That requires location evidence kept for 10 years under OSS. This does not cryptographically weaken unlinkability, but it retains country or postcode and purchase time, which helps narrow any later timing analysis. Keep credit units identical across countries so tax does not partition credentials. Gross prices can vary.

**EU withdrawal needs the buyer's help.** The operator cannot compute proportional use unless the client reveals its remainder. It needs the refund flow in Question 4 or an express waiver under Art 16(a) or (m).

**Sanctions controls exist only at purchase.** Tor defeats geofencing at the moment of use, and a transferred credential evades purchase-side screening. Kraken concerned an exchange, so this is a question for counsel, not a direct parallel.

**Taler's path signals the boundary.** General-purpose anonymous money has needed regulation. A single-merchant credit is a different case, and it should be kept that way.

**A USD Stripe sandbox PoC avoids all of these.** There is no sale, no money, no VAT and no real dispute. It proves mechanics, not legality or acceptance.

**Questions for counsel**

1. Is a bearer credit for one seller's service closed-loop prepaid access, convertible virtual currency, or neither?
2. Does technical transferability change the answer if terms and product forbid it?
3. Do Reg E's gift-certificate rules cover token-denominated, non-reloadable credit sold anonymously, and what expiry is allowed?
4. Do state gift-card and unclaimed-property laws apply to anonymous balances?
5. Is the EU sale outside PSD2 and EMD2 as a two-party sale, or does it need an Art 3(k) exclusion?
6. Which CRD waiver applies to prepaid inference credit, and how does Art 11a work where no account exists?
7. Is a credit an SPV across all member states when bought with location evidence but redeemed anonymously?
8. Are purchase-time sanctions screening and Tor-based use sufficient?
9. Does the operator face any illegal-content reporting duty that cannot be met by design?

### Gaps

- US state gift-card and escheat laws were not researched.
- FinCEN's 2019 convertible virtual currency guidance was not read.
- AMLR Art 2's "financial institution" definition was not read.
- CRD Art 11a's primary text and scope were not read.
- Official Journal publication of PSR and PSD3 was not confirmed.
- Whether a threshold exists for non-EU VAT sellers was not confirmed.
- UK rules were not researched.
- Exact CHF limits for Taler were not verified.
- GDPR retention for purchase records was not researched.

---

## 6. Abuse handling without identity

### Takeaway

Prepayment and per-request refusal impose cost and block individual requests, but they give no accountability. There can be no per-user ban, strike history or retrospective investigation unless a stable pseudonym is added, and that links a user's requests together. Meta's and LLM-Tor's approaches rest on an account at issuance, which a guest card purchase does not provide.

### Cited Findings

- LLM-Tor §5.1, on per-request tokens: "there is no stable identifier that the proxy can use to rate-limit or ban a misbehaving user. Content moderation rejects individual bad requests, but does not prevent a determined adversary from repeatedly issuing new tokens". The motivation is "largely" "legal compliance and ability to adhere to upstream TOS" — [LLM-Tor whitepaper, March 2026](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- LLM-Tor §5.2 and §5.5: it adds two blind-signed tokens, one permanent per account and one transient per month. Both are checked against blacklists on every anonymous request. "The permanent token creates a persistent anonymous pseudonym: all requests bearing the same A_p are linkable to each other, but not to the paying identity." "without any stable identifier the proxy has no mechanism to enforce abuse bans." — [LLM-Tor whitepaper](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- LLM-Tor moderates at the proxy on plaintext (§7), lists a global passive adversary, endpoint compromise, upstream logging and stylometry as out of scope (§8.3), and lists key rotation as TODO (§9) — [LLM-Tor whitepaper](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- Meta uses anonymous credentials to "rate limit access to our infrastructure", and credentials "can be used a finite number of times" — [Meta whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper/)
- Trail of Bits: "Each client is allowed to request only a limited number of anonymous credentials a day, and each anonymous credential may be used for a limited number of requests." The issuing client is a logged-in WhatsApp client — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf)
- NCC: "Since the ACS rate-limits emission of tokens, this inherently implies rate-limitation of Client requests" — [NCC report](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf)
- Apple uses a "single-use credential, based on RSA Blind Signatures", "to authorize valid requests without tying them to a specific user", through a third-party OHTTP relay. The blog does not discuss abuse or rate limits — [Apple PCC blog](https://security.apple.com/blog/private-cloud-compute/)
- Tor onion PoW defends against introduction floods, not large botnets. It is off by default (`HiddenServicePoWDefensesEnabled`) and dormant until overload. Load after connection needs `HiddenServiceMaxStreams`. It did not work with Onionbalance as of July 2025, and a redesign is proposed in prop 362. The page, updated 2026-05-08, is itself marked "Needs update" — [Onion services PoW](https://onionservices.torproject.org/technology/security/pow/); [Tor blog](https://blog.torproject.org/introducing-proof-of-work-defense-for-onion-services/); [Tor DoS guidance](https://community.torproject.org/onion-services/advanced/dos/)
- Arti's client PoW support conflicts between sources: an FAQ says "not yet available as of July 2025", while the implementations page lists client support from Arti 1.3.0 and experimental server support from 1.5.0 — [Onion services PoW](https://onionservices.torproject.org/technology/security/pow/); [implementations](https://onionservices.torproject.org/dev/implementations/)
- `draft-ietf-privacypass-arc-protocol-01` (2026-03-02) shows as an expired "Dead WG Document" on Datatracker, updated 2026-09-03. ARC should not be assumed as a live standards path (cross-track) — [ARC datatracker](https://datatracker.ietf.org/doc/draft-ietf-privacypass-arc-protocol/)

### Inferences

| Control | Works without identity? | Limit |
|---|---|---|
| Prepayment as cost | Yes | A stolen card makes abuse free for the abuser and costly for the operator. |
| Purchase caps per card | Yes, on the identified side | Many cards defeat it. |
| Ceiling pre-authorization | Yes | Limits one request's cost. |
| Per-request refusal inside the enclave | Yes | Stops only that request. Moderation false positives have no appeal path. |
| Global gateway rate limit | Yes | Protects capacity, but a flood affects everyone. |
| Issuing fewer requests per credential (ARC-style limit) | Partly | Crypto-track question; ARC status is uncertain. |
| Onion PoW | Yes | Connection floods only. |
| Short key epochs | Yes | Limits lifetime, but partitions users. |
| Stable per-purchase pseudonym | Links that purchase's requests | A ban burns only that purchase's balance. It breaks per-request unlinkability and conflicts with the proposal. |

**What cannot be done in the proposed design:**

- Ban or rate-limit one user across purchases.
- Keep a strike history.
- Investigate past use or answer a request about a specific user's use.
- Geofence at use.
- Prove which person made a request.

**Meta's rate limit does not transfer.** It relies on a logged-in account at issuance, while a guest card purchase offers only card-side velocity. Issuance is also free there, so an account ban means something. Here, an abuser who pays is the customer.

**Moderation placement.** Moderation should run inside the enclave so the operator does not see plaintext, unlike LLM-Tor's proxy. Output is then a refusal only, and the operator has no per-user view. Refusal reasons and rates may need bucketed logging (cross-track with the inference track).

**Is the trade-off acceptable?** Compliance with upstream terms and with any illegal-content duty needs a decision by a person with counsel's advice. Technical design cannot resolve it.

### Gaps

- Verda's acceptable-use policy, Tinfoil's terms and model licences were not researched (other tracks may cover them).
- Illegal-content reporting duties were not researched.
- I found no published statement of how Apple handles abuse through its blind credentials beyond rate limits.
- Whether Arti can solve current onion PoW can only be settled by running it.

---

## 7. Wallet technology

### Takeaway

A Rust core with embedded Arti should come first, as a CLI or TUI, with Tauri as an optional thin shell and a browser extension as the weakest choice. ACT's durable pre-spend rule makes a crash-safe journal a core requirement, and restoring a stale backup can lose the remaining balance because bearer credit has no account to recover from.

### Cited Findings

**Protocol requirements**

- ACT §6.6.1: "Before they make a spend request or an issue request, the client MUST store their private state (the nullifier, the blinding factor, and the new balance) durably." "For the issuer, the spend and refund operations MUST be treated as an atomic transaction." "If a nullifier associated with a given spend is persisted to the database, clients MUST be able to access the associated refund." "If they cannot access this, then they can lose access to the rest of their credits." The issuer "MUST inform the client" of its cleanup policy — [ACT draft](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01)
- ACT says the nullifier database grows linearly (§5.1), requires ACID storage (§6.4.1), treats parallel spends as an attack with atomic nullifier handling (§6.5.1), defines no split or merge, and supports L from 1 to 128 (§3.1). Tokens are 192 bytes and proofs 32×(14+4L) bytes. Backup, restore and rollback are not addressed, and no retry message is defined — [ACT draft](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01)

**Precedent**

- Cashu NUT-13: 12-word BIP39 seed, per-keyset counter, restore in batches of 100 until three empty batches, then check spent state via NUT-07. No explicit rollback warning — [NUT-13](https://raw.githubusercontent.com/cashubtc/nuts/main/13.md)
- Cashu NUT-09: the mint stores every BlindedMessage and BlindSignature, with `POST /v1/restore`, which also recovers interrupted swaps — [NUT-09](https://raw.githubusercontent.com/cashubtc/nuts/main/09.md)
- LLM-Tor backup: PBKDF2-SHA256 at 600,000 iterations, 128-bit salt, AES-256-GCM with 96-bit IV, base64(salt ∥ IV ∥ ciphertext) in a local `.llmtorbak` file, with optional ciphertext-only server sync — [LLM-Tor whitepaper](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf)
- Routstr uses a Cashu token as an API key. The node redeems at the mint, deducts cost, forwards upstream, and allows withdrawing the remainder. Per-request change was on the roadmap at blog time. It has no TEE and no card — [Cashu blog](https://blog.cashu.space/routstr/); [OpenSats](https://opensats.org/projects/routstr)
- Brave: an authorization failure after presentation leaves a credential "spent" but unused, and the reporter proposes caching and reuse. This is a real example of the partial-failure problem — [Brave issue 29345](https://github.com/brave/brave-browser/issues/29345)

**Components**

- arti-client 0.48.0 (docs.rs, as fetched): "the APIs for this crate are not all yet completely stable"; `onion-service-client` is a feature flag; some features are experimental and outside semantic versioning — [arti-client docs](https://docs.rs/arti-client/latest/arti_client/)
- Arti latest found is 2.6.0 (undated forum post). 2.5.0 (30 June 2026) made congestion control default and fixed denial-of-service issues TROVE-2026-024 and TROVE-2026-027, with minimum Rust 1.91. 2.4.0 (June 2026) included onion-client connectivity fixes and breaking `TorClient` API changes (secondary). I found no source calling the onion client production-ready — [Arti 2.6.0](https://forum.torproject.org/t/arti-2-6-0-released/22068); [Arti 2.5.0](https://blog.torproject.org/arti_2_5_0_released/); [AlternativeTo (secondary)](https://alternativeto.net/news/2026/6/arti-2-4-released-with-stable-flow-and-congestion-controls-torclient-api-changes-and-more/)
- Tauri's Stronghold plugin page shows no deprecation notice. I could not confirm my recollection of a deprecation, so it is not claimed — [Tauri Stronghold](https://v2.tauri.app/plugin/stronghold/)
- SQLite: `synchronous=FULL` in WAL mode is durable across power loss. With `NORMAL`, a committed transaction "might roll back following a power loss or system crash" — [SQLite pragma](https://www.sqlite.org/pragma.html#pragma_synchronous)

### Inferences

**Form factor comparison**

| Option | Strengths | Weaknesses | View |
|---|---|---|---|
| Rust CLI or TUI with Arti | Smallest attack surface, no webview, easy to test crash behaviour and capture packets | Less friendly | Build first. |
| Tauri shell over the same Rust core | Friendly model selector and budget display | Webview must have no network access. Stripe must open in the system browser. | Add later. |
| Browser extension | Easy install | No raw Tor client, weaker storage durability, shares a browser with identity. Kagi's extension shows Privacy Pass is possible but has no Tor path. | Weakest. Capability limits not verified. |

**Spend journal (ACT-based sketch, needs crypto review)**

| State | Durable write before moving on | Crash recovery |
|---|---|---|
| PREPARED | New secret, blinding factor, expected balance; old credential retained | Send, or retry the same spend |
| SENT | Marked as sent | Retry with the same nullifier; expect the stored refund |
| STREAMING | Receipts so far, if any | Fetch the final refund by nullifier |
| REFUND_RECEIVED | Refund and final receipt | Verify and compute the new credential |
| COMMITTED | New credential active; old state erasable | None |

Use SQLite in WAL mode with `synchronous=FULL`, or equivalent, with the database encrypted at rest. ACT defines no retry message, so the PoC must define one. Without it, a lost response strands the balance. This is a required protocol extension, not a wallet detail.

**Rollback hazard.** Restoring state from before a spend means re-presenting a spent nullifier, which will be rejected. The remainder is lost unless the issuer still holds the refund. So:

- The issuer's refund-retention policy is a user-funds parameter.
- Backups should be automatic at every commit, or the UI should warn that an old backup may be worth nothing.

**Seed restore is a trade-off.** A Cashu-style deterministic seed plus server-held signatures allows recovery, but the restore call discloses a set of blinded messages together, and that links them to one client at restore time. ACT defines nothing like it, so adopting it would be new design work.

**Multi-device and concurrency.**

- A single ACT chain is sequential. Concurrent or multi-device use needs several credentials obtained at purchase (withdrawal splitting), or an explicit transfer with s=0.
- An s=0 spend may itself be a visible, distinctive event.
- A simple rule for the PoC: one device, one chain, plus an export that moves rather than copies.
- Meta's token allows concurrent reuse, per TOB-WAPI-4, but reuse links those requests. It is not a model for this design.

**Loss of funds.** Losing the wallet loses the credit. Mitigate with small purchases, encrypted export and plain UX language such as "treat this like cash". Mullvad's account number and Cashu's seed are the closest UX precedents, but Mullvad's stable number is an account, not bearer value.

**Key storage.** The OS keychain or a passphrase-derived key are the realistic options. Neither was verified here.

### Gaps

- Tauri keyring plugins, SQLCipher and their maintenance status were not verified.
- Taler's wallet backup documentation returned a 404.
- Browser-extension networking limits were not verified.
- The operational reliability of Arti's onion client can only be measured.
- Arti 2.6.0's release date was not stated.

---

## 8. Budget display and reconciliation

### Takeaway

"Reconcile" cannot mean comparing with a server-side account, because none exists. It has to mean a client ledger checked against the issuer-signed credential balance, enclave-signed per-request receipts and a signed price list. I did not find a surveyed system documenting this combination, and it depends on the crypto and inference tracks.

### Cited Findings

- ACT hides the balance but not the amount: "The spent amount s is revealed to the issuer." The refund token gives a new balance of c−s+t, with t chosen by the issuer and 0≤t≤s. t is in the signed refund transcript. The spend proof does not commit to t, and client-side refund construction does not check t≤s in the draft as I read it — [ACT draft §3.4.3, §6.3](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01)
- ACT says a client that cannot access its refund can "lose access to the rest of their credits" — [ACT draft §6.6.1](https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01)
- Brave shows a real case of value spent without service delivered — [Brave issue 29345](https://github.com/brave/brave-browser/issues/29345)

### Inferences

**Three sources of truth**

| Source | Holds | Trust |
|---|---|---|
| Credential | Authoritative balance, signed by the issuer and known only to the client | Protocol |
| Local ledger | Per-request history, purchase records, expected balance | Client only |
| Enclave-signed receipt | Model, input and output token counts, price-list version, bound to the nullifier or request nonce | Attestation (cross-track) |

**Reconciliation steps per request:**

1. Before sending, compute the expected maximum cost from the signed price list.
2. After the stream ends, verify the receipt signature and bind it to this request.
3. Compute expected t from receipt and price list, and check it against the refund. The wallet must itself check t≤s and t equal to the expected value.
4. Verify the new credential's balance matches the ledger, then commit.

**On a mismatch,** the wallet should alert the user and record evidence. There is no account page, so a dispute means presenting a receipt anonymously. The operator can honour it only as a goodwill credit, and this would need its own abuse controls.

**Mid-stream failure.** Define who pays for partial output. A ceiling pre-auth with final metering in a receipt is the natural fit. If the receipt never arrives, the wallet retrieves the refund by nullifier, and if that also fails the ceiling is lost. The issuer's retention policy therefore sets the maximum loss per failure.

**Budget display.**

- Show balance from the credential, with cost per request and session from the ledger.
- Mark the balance "pending" during streaming.
- The server cannot show the balance or send low-balance prompts, so the wallet raises them locally. This local prompt is also the buy-ahead trigger from Question 2.
- Price changes need a signed, versioned list common to all users. Per-user price lists partition the anonymity set.

**Operator-side reconciliation is aggregate only.** Total sold minus total spent gives outstanding liability. That supports accounting and reserves without linking anyone. How to treat breakage and expiry is a question for accountants and counsel.

**Metering leakage (cross-track).** The issuer sees s and t per request, which leaks per-request usage unless settlement happens inside the enclave or amounts are bucketed. Bucketing makes the receipt match less exact, so this trades precision against privacy.

### Gaps

- Tinfoil's usage records and any enclave-signed metering are other tracks' material and were not researched here.
- I did not find a precedent for enclave-signed per-request receipts paired with anonymous credit. This is not proof that none exists.
- The behaviour of `t` in reference implementations was not checked in code.

---

## Proposed validation

None of this was run. Time estimates are rough.

| What to run | Confirms the claim if | Refutes it if | Time |
|---|---|---|---|
| Sandbox Checkout, webhook and single-collect flow, with replayed webhooks and concurrent collects | Exactly one issuance per paid session; a lost response can be re-fetched | Duplicate issuance or stranded payment | 1 day |
| Sandbox test cards for fraud dispute, EFW, Radar block and 3DS | Handlers reach expected states and keep no spend data | Handler needs usage data or fails | 0.5 day |
| Inspect stored Stripe objects after a test purchase | Only `client_reference_id` hash, amount, buyer details, with no wallet or blinded data | Any credential-related field | 1 hour |
| Written Stripe review of a plain product description | Written acceptance for live mode | Refusal or limits (a live-mode blocker) | 1-3 weeks of waiting |
| Packet capture of the wallet through purchase, delay and spend | No clearnet traffic for spend, no resolver query for spend hosts, no shared TLS tickets | Any leak | 0.5-1 day |
| Arti isolation test against a controlled onion service or logging test endpoint | Different isolation tokens give different circuits | Circuits shared | 0.5 day |
| Fault injection: kill or power-cut at every journal state, with lost responses | No balance loss, and retry returns the same refund | Any state that loses balance | 1-2 days |
| Restore backups taken before N spends | Stale state is detected and handled, and losses match the documented policy | Silent loss or wallet corruption | 0.5 day |
| Run a Bayesian linker over synthetic pilot logs in the real schema | Effective set matches Tables A to D | Real logs give extra signals and smaller sets | 1-2 days |
| Measure first-spend delay in any pilot with consenting testers | Buy-ahead delays reach days | Users spend immediately, so the model's worst cases apply | During the pilot |
| Test onion PoW between C Tor service and Arti client | Client solves puzzles under simulated overload | Failure or unsupported path | 0.5-1 day |
| Counsel review of the questions in Question 5 | Written advice on classification and EU sales | Any regulated classification | External, weeks |

---

## Claim status table

"Untested in practice" means supported only by documentation or source reading.

| Claim | Status | Evidence URLs |
|---|---|---|
| NCC flagged issuance-to-use timing for Meta | false as attributed (text search of PDF extraction) | https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf |
| Trail of Bits flagged issuance-to-use timing, and Meta's answer relies on scale | solid, documentation | https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| Buying is the only non-private moment | weak: true cryptographically, not statistically at small scale; refunds, withdrawal and tax also touch identity | https://www.rfc-editor.org/rfc/rfc9576.html ; https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 ; own simulation |
| Best-guess link rate is about 1/(1+k) | conditional: own model, not empirical | `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/purchase_anonymity_set.py` |
| A tens-of-users PoC offers meaningful unlinkability from the operator | false under the model; content privacy is separate | own simulation; https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| ACT is a stable standard | false: expired individual draft | https://datatracker.ietf.org/doc/draft-schlesinger-cfrg-act/ |
| ACT hides balance and reveals the spent amount | solid, untested in practice | https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 |
| Checkout can bind payment to one blind issuance without blinded data at Stripe | conditional, untested in practice | https://docs.stripe.com/checkout/fulfillment.md?ui=full-page ; https://docs.stripe.com/api/checkout/sessions/create |
| Stripe would accept this product in live mode | unknown | https://stripe.com/legal/restricted-businesses ; https://support.stripe.com/questions/restricted-business-list-and-considerations ; https://docs.stripe.com/billing/subscriptions/usage-based/billing-credits |
| A sandbox PoC can test purchase and dispute paths without money | solid, untested in practice | https://docs.stripe.com/sandboxes ; https://docs.stripe.com/testing |
| Stripe Tax can run with no location data | false | https://docs.stripe.com/tax/customer-locations |
| Arti stream isolation is available | solid, untested in practice | https://docs.rs/arti-client/latest/arti_client/ ; https://docs.rs/arti-client/latest/arti_client/isolation/struct.IsolationToken.html |
| Arti's client API is stable | conditional: API churn stated | https://docs.rs/arti-client/latest/arti_client/ ; https://alternativeto.net/news/2026/6/arti-2-4-released-with-stable-flow-and-congestion-controls-torclient-api-changes-and-more/ |
| Small denominations limit chargeback loss | weak: fee floor | https://support.stripe.com/questions/june-2025-pricing-updates-for-disputes |
| 3DS shifts liability for credit purchases | conditional: Visa excludes stored-value load; classification unknown | https://docs.stripe.com/payments/3d-secure/authentication-flow |
| A holding period can outlast chargebacks | false in practice: window is 120+ days | https://docs.stripe.com/disputes/how-disputes-work |
| Disputes can be defended with usage logs | false for an unlinkable design; purchase-side evidence and CE 3.0 partly help, untested in practice | https://docs.stripe.com/disputes/best-practices ; https://support.stripe.com/questions/how-does-stripe-support-visa-compelling-evidence-3-0 |
| Unused anonymous balance can be refunded to card | conditional: design sketch, no card-funded precedent found | https://docs.taler.net/core/exchange/post-recoup-withdraw.html |
| Own-service credits fit US closed-loop treatment | conditional: for counsel; transferability is the risk | https://www.law.cornell.edu/cfr/text/31/1010.100 ; https://www.fincen.gov/resources/statutes-regulations/guidance/application-fincens-regulations-persons-administering |
| Reg E's five-year rule may apply to this credit | conditional: for counsel | https://www.consumerfinance.gov/rules-policy/regulations/1005/20/ ; https://www.consumerfinance.gov/rules-policy/regulations/1005/interp-20/ |
| PSD2 "premises of the issuer" covers online credit | false | https://www.finanssivalvonta.fi/contentassets/fe9063c85d0a4a48bd8a155055c99c43/2022_2_en_final-report.pdf |
| Own-service credit falls outside e-money | conditional: for counsel; breaks if third parties accept it | https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32009L0110 |
| EU VAT can be handled without retaining location evidence | false for OSS sellers | https://eur-lex.europa.eu/eli/reg_impl/2011/282 ; https://vat-one-stop-shop.ec.europa.eu/one-stop-shop/record-keeping-and-audits-oss_en |
| EU withdrawal rights can be met without client help | weak: proportional use is unknown to the operator; waiver route for counsel | https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:32011L0083 |
| PSD3 and PSR are published and in force | unknown: OJ publication not confirmed | https://data.consilium.europa.eu/doc/document/ST-8535-2026-ADD-1/en/pdf ; https://www.arthurcox.com/insights/psd3-and-psr-final-compromise-texts-published/ |
| Sanctions controls can apply at time of use | weak: Tor defeats IP geofencing | https://ofac.treasury.gov/media/929541/download |
| Per-user bans are possible without a stable identifier | false | https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf |
| Prepayment works as abuse and DoS control | conditional: raises cost, but stolen cards undercut it | https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.pdf ; https://docs.stripe.com/disputes/prevention/card-testing |
| Meta's rate limiting carries over to guest purchase | weak: relies on a logged-in account | https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| Onion PoW protects against introduction floods | conditional: untested in practice; docs conflict on Arti | https://onionservices.torproject.org/technology/security/pow/ ; https://onionservices.torproject.org/dev/implementations/ |
| A wallet must durably store state before each spend | solid, untested in practice | https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 ; https://www.sqlite.org/pragma.html#pragma_synchronous |
| ACT specifies crash retry | false: required behaviour stated, no retry message defined | https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 |
| Restoring an old backup is safe | false unless the issuer retains refunds; inference from the protocol, untested | https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 |
| Budget can be reconciled against a server account | false in this design; requires client ledger, signed credential, receipts | https://datatracker.ietf.org/doc/html/draft-schlesinger-cfrg-act-01 |
| Enclave-signed per-request receipts are an established pattern | unknown: none found in this track | none |
