# 08. Decisions and validation

This records why the design is the way it is, the decisions you took on 7 November 2026 and what each costs, and how to find out early if it's wrong. The top-level statement of the design is [`ARCHITECTURE.md`](../../ARCHITECTURE.md), which wins where the two disagree.

## 1. Decision log

Entries that name a decision carry one of the six taken on 7 November 2026 ([section 3](#3-decisions-taken)). D-numbers are stable, so new entries are appended.

| # | Decision | Main reason | Main cost | Revisit if |
|---|---|---|---|---|
| D1 | Two planes, meeting only at blind EC-ACT issuance inside the gate | Purchase learns nothing about use | Issuance is identified by design, so unlinkability rests on EC-ACT's blinding and one shared context | Never, this is the core idea |
| D2 | EC-ACT credentials, held and verified by the gate inside the CVM (decision 1) | Exact per-token settlement with one balance per purchase, serialised with no merging, and no usage data in an operator-readable database | Draft cryptography, version 0.1.0, issuer key next to the GPU | EC-ACT fails review ([section 2](#2-fallback-if-ec-act-fails-review)) |
| D3 | Inner attested TLS from the first Tor integration, and on the purchase plane | C tor stays out of the plaintext path, and the storefront needs no domain or public certificate, for about 150 lines shared by both servers | One more round trip | Measured latency is unacceptable |
| D4 | Onion key not a confidentiality secret | Attestation inside the tunnel protects content, so no sealing is needed | The host can read the key and front the service, seeing stream boundaries and TLS record sizes, or deny service | Location hiding becomes a goal, or record sizes prove revealing |
| D5 | Evidence stapled, fresh report per connection without a fresh verdict | The wallet never contacts AMD or NVIDIA | Firmware report rate is unknown, so one semaphore. The verdict cache leaves a weak marker. | M6 measures it |
| D6 | GPU appraised by the measured guest | `nvattest` does months of work, and Blackwell has no usable Rust verifier | Wallet trusts the image's appraisal, and evidence ages | A real wallet-side verifier appears |
| D7 | Operator-held key K opens the store and the issuer seed (decision 2) | Survives updates and chip moves. The spent set and issuance records move with the store, so nothing replays across an update. | Operator can mint and read the store's metadata. Every restart needs an unlock. | The issuer key has to leave the GPU VM ([section 3](#2-operator-held-key-k)) |
| D8 | Sealed results, no pruning, no timestamps | Without the wallet's bytes, a K-holder can't read `t` at rest | Store grows by about 200 B per session. Doesn't hide timing from someone holding K and disk copies ([05 section 4](05-component-gate.md#4-money-store)). | Disk becomes an issue |
| D9 | Config inside the image, issuer key in the manifest | Products, prices and catalogue are attested, and the key only exists after first unlock | Any price change means a new image | Prices change often |
| D10 | Settle exact token charges | You asked for per-token metering, and inside the enclave the exact count isn't exposed | Refund `t` reveals a precise value to the gate | Grid settlement proves useful |
| D11 | Session tier ladder compiled into the wallet: `s` is the largest tier not above `min(balance, maximum_spend)` | Most spends show the same `s`, and `t` is never echoed by the next spend | Descending tiers show a chain is nearly empty, and a balance below the smallest tier is stranded | Real usage suggests other tiers |
| D12 | Crash refunds in full | Exact charges were in memory only, and users shouldn't pay for our failures | Crash loops could farm inference | A real abuse path appears |
| D13 | C tor service, Arti client | Arti lacks onion-service PoW and stability; a Rust wallet with no sidecar is simpler | The pairing has never run | M4 failure rate is too high |
| D14 | `walletd` and `wallet-cli` in separate processes | Two network stacks can't mix, enforced by dependencies | Two processes share a journal | Never for purchase privacy |
| D15 | Default of one session per request, each on its own circuit | Nothing in the protocol links requests unless the client asks | Slower. A first byte had a 4.33 s median on a fresh circuit. Forced retries and the verdict cache still leak signals ([limit 17](01-system-and-data-flow.md#accepted-limits)). | Users always want conversations |
| D16 | A few fixed products in `release.json`, each starting one chain, no merging | Simplest correct thing, and credit design supports more later | Fixed amounts, stranded small balances | After the PoC |
| D17 | Rust for every component we write (decision 5) | Follows `act-rs`, Arti and `sev`, so one language holds the crypto, client and verifier | Slower to write than Go | The credit library changes |
| D18 | vLLM behind a Unix socket, allowlisted | Real engine, and `PrivateNetwork` removes a whole class of leaks | Python and proprietary wheels in the TCB | Tinfoil backend replaces it |
| D19 | Hand-written Stripe client | Two calls, and `async-stripe` can stop fulfilment on parse errors | We own the HTTP code, tested against `fake-stripe` | Stripe's surface grows |
| D20 | Update by moving state to a new instance | No SSH in the final image. The storefront has no state to move. | User downtime of a drain (up to 15 min) plus export, boot and unlock, unmeasured. Separately, two instances bill for perhaps an hour. Needs capacity. | Handoff is built |
| D21 | Credential issued at purchase time, over clearnet, through the storefront and `issue.sock` (decision 6) | Blind EC-ACT issuance already hides which credential an order became, so issuing on the purchase plane costs no unlinkability, and buying needs no Tor and no second signature scheme | Issuance is tied to the buyer's IP and time, which purchase already reveals. A compromised storefront can mint ([limit 20](01-system-and-data-flow.md#accepted-limits)). | EC-ACT's blind issuance fails review |
| D22 | Manifest fetched over Tor at use time, or from the storefront during a purchase, and applied only with user approval | An update check on the purchase plane at use time is a use-linked request at a time the operator chooses. During a purchase that moment is already identified. Approval makes "wallets pin releases" true. | A user who doesn't approve can't reach a new release. Approval can't help someone who approves everything. | Wallet-enforced log exists |
| D23 | Storefront on the same Verda instance, as its own process and user, on TCP port 8443, with attested TLS (decision 3) | No second server, no domain, no public certificate. The wallet checks the storefront the way it checks the gate. | A clearnet-facing server inside the trusted boundary, and the host sees both planes on one machine ([limits 2 and 21](01-system-and-data-flow.md#accepted-limits)) | M0 shows the port doesn't reach the instance |
| D24 | Stripe sandbox only (decision 4) | Stored value needs Stripe's approval before live keys | Test cards only | Before live money |
| D25 | Payment confirmed by polling the Checkout Session at collect | The CVM needs no inbound connection from Stripe, no shared secret and no public certificate, and the storefront stores nothing | Each poll is a Stripe call | Polling load matters |

## 2. Fallback if EC-ACT fails review

EC-ACT's draft is preliminary and its security arguments are unfinished. **Treat it as a measured bet: no user money is at stake in the PoC, and its failure mode is known.** The failure could be a break in the scheme, a bug in `act-rs`, or simply a cryptographer's advice not to rely on it.

What stays is almost everything: the storefront and the purchase flow, `issue.sock`, Tor, TLS, attestation, gate state machine, metering, wallet journal structure and the kill tests. What changes is the scheme behind `Moderator` and `Holder`.

| Option | How it works | Keeps | Loses | Rough change |
|---|---|---|---|---|
| **Gate-signed blind tokens** | The gate holds blind-RSA keys in several denominations. At purchase, `issue.sock` carries the wallet's blinded tokens instead of an EC-ACT issue request. A session spends tokens summing to `s`, and close returns change as newly blinded tokens. The gate signs the change, so it can still mint. | One CVM, in-band refund, issuance at purchase time. At a crash, only the spent tokens are at risk rather than a whole chain, and there's less pressure from forced retries. | Single balance, since the wallet now does coin selection. Which tokens sum to `s` replaces whole chains as the qualifier. Denominations leak spending patterns. Settlement rounds to the smallest denomination or takes many signatures. It also brings back a second signature library. | 1 to 2 weeks, a guess |

**If it fails,** this option preserves the shape of what's built. Before anything real, moving the issuer key out of the GPU VM is worth weighing as well ([section 3, decision 2](#2-operator-held-key-k)).

**Keep the door open by:**

- Hiding `act-rs` behind `credit`, with no other crate naming its types.
- Having `SchemeDescriptor` in the manifest, so a wallet refuses an unknown scheme.
- Testing the money loop against invariants rather than EC-ACT details.
- Getting an outside cryptographer's view before any live money. Trigger this at M1's end at the latest, so a change costs little.

## 3. Decisions taken

You took these six on 7 November 2026. [ARCHITECTURE section 2](../../ARCHITECTURE.md#2-decisions) states them. Here is what each one does to the design and what it costs.

### 1. EC-ACT credentials in the gate

**Consequence.** Exact per-token metering with a hidden balance per purchase, and no usage records outside the enclave. The gate holds and verifies credentials, and holds the issuer key while unlocked. `act-rs` ran on the laptop test at 3,499 B and about 7 ms per verify at L=24. [tested on a laptop]

**What it costs.** The cryptography is a draft, with version 0.1.0 of its one implementation ([limit 10](01-system-and-data-flow.md#accepted-limits)). That's the reason for the fallback in [section 2](#2-fallback-if-ec-act-fails-review). It has other weaknesses:

- The host can roll back the store.
- Losing the store loses every balance.
- The issuer can't be rotated without voiding every balance, because no scheme here migrates a balance to a new key.
- Minting with a stolen issuer secret is undetectable, because credit totals are hidden by design. Comparing `issued` rows with Stripe's paid sessions catches minting through the storefront, not minting with the secret.
- Losing a journal loses a whole chain.

Forged credit costs more than GPU time. Fixed capacity limits compute consumption, but total damage also depends on outstanding forged credits, response time and any contractual obligations. With a sandbox, there's no exposure yet.

### 2. Operator-held key K

**Consequence.** K opens the money store and the issuer seed. Both survive image updates and chip moves, and the spent set and issuance records move with the store.

**What it costs.** Every restart needs an unlock ([limit 11](01-system-and-data-flow.md#accepted-limits)). The operator can mint and read the store's metadata, as the operator could anyway. A gate compromise can mint. The options weighed:

| | Operator-held K | Measurement-sealed key | Separate CPU CC VM | Separate public-verification mint |
|---|---|---|---|---|
| Hidden from operator | No | Yes | No, with K as here | No |
| Survives image update | Yes | **No, every image voids all balances** | Yes, with K as here | Yes |
| Survives chip move | Yes | No | Yes, with K as here | Yes |
| Unattended restart | No | Yes, on one chip | No | Yes |
| Replay after a new image | Closed, since the spent set and `issued` rows move | `issued` rows are lost, so paid orders could be collected again [inferred] | Closed | Closed |
| Gate compromise can mint | **Yes** | **Yes** | Can't steal the key or mint without limit, but can misstate settlement within each spend's `s` | No |
| Extra cost | None | None | About $36/month, plus an attested link. Same persistence problem. | A settlement design |

The update problem rules out sealing, since Tor releases force new images, unless you're happy to void test balances. The separate CPU CC VM and the public-verification mint both remove this design's largest weakness, a gate compromise that can mint. They cost another service, and the mint also costs exact settlement or needs another design. Either is the path if the issuer key has to leave the GPU VM. Later, a handoff between enclaves could also hide the key from the operator.

### 3. Storefront on the same instance

**Consequence.** No second server. The storefront runs as its own process and user, `storefront`, on TCP port 8443, with attested TLS and role `storefront`. It needs no domain name, no public certificate and no TLS terminator, because the wallet checks it the way it checks the gate. The CVM gets one clearnet inbound port and outbound HTTPS for one user, and the gate and engine keep `PrivateNetwork=yes`.

**What it costs.**

- A clearnet-facing HTTP server inside the trusted boundary ([limit 21](01-system-and-data-flow.md#accepted-limits)).
- A compromised storefront can mint through `issue.sock` ([limit 20](01-system-and-data-flow.md#accepted-limits)).
- The host sees both planes on one machine, which makes timing correlation at small scale slightly easier ([limit 2](01-system-and-data-flow.md#accepted-limits)). What one operator sees doesn't change, because one person runs both.
- Purchases depend on the gate being Ready ([limit 22](01-system-and-data-flow.md#accepted-limits)).
- It rests on Verda passing an inbound port beyond SSH, and on a stable public IP, both unchecked ([limit 23](01-system-and-data-flow.md#accepted-limits)).
- A browser-facing storefront page would need a domain and a certificate on port 443. That's deferred.

### 4. Stripe sandbox only

**Consequence.** Test cards only. The PoC completes in the sandbox.

**What it costs.** No live payments. Stored value is on Stripe's restricted list, and the seller-maintained case may be allowed. Going live means asking Stripe, and getting NVIDIA's commercial terms in place ([section 6](#outside-the-build)).

### 5. Rust

**Consequence.** One locked workspace. The credential library, Arti and `sev` are all Rust, so one language holds the crypto, client and verifier, and the test fakes share the same code paths.

**What it costs.** Slower to write than Go.

### 6. Issuance on the purchase plane, over clearnet, at purchase time

**Consequence.** The storefront confirms payment by polling the Checkout Session, then asks the gate over `issue.sock` to issue an EC-ACT credential against the wallet's blinded issue request ([01 section 4.4](01-system-and-data-flow.md#44-purchase-and-issuance-clearnet)). A purchase ends with a Ready chain in the wallet, and Tor is needed only for use. Blind EC-ACT issuance already gives the unlinkability that a separate blind-signature layer would, so there's one signature scheme, no second set of keys, and no records of its own to retain or purge.

**What it costs.**

- Issuance is tied to the buyer's IP and time. The purchase plane already knew both.
- Unlinkability rests on EC-ACT's blinding and one shared context alone ([limit 10](01-system-and-data-flow.md#accepted-limits)).
- The storefront can ask for issuance, so a compromised storefront can mint ([limit 20](01-system-and-data-flow.md#accepted-limits)).
- Purchases can't complete while the gate is Locked or Draining ([limit 22](01-system-and-data-flow.md#accepted-limits)).

## 4. Design rules

These rules hold across the components, whichever milestone is being built.

- An inner attested channel on both planes. Nothing secret is sent before attestation passes.
- Holds use separate rates: `H = in_rate × prompt + out_rate × allowed_output`.
- Build first around `issue → reserve → execute → settle → recover` with a fake engine, with kill tests at each durable transition.
- The wallet and the gate each keep an invariants list and a failure-policy table ([04 section 9](04-component-wallet.md#9-invariants), [05 section 9](05-component-gate.md#9-invariants)).
- Config is pinned in the measured image, and wallets pin accepted versions: a manifest applies only after the user approves it.
- Local recount is a diagnostic with no threshold.
- A measurement mismatch means diagnose first.
- Updates mustn't expire purchased credit.
- Nothing fetches remote content because of model output. The gate has no outbound client, and the local page loads no third-party scripts.
- Constant-size frames are a later, measured feature.
- A crash refunds in full, once, and a repeated admission maps to the same session.
- One terminal transition per session, so a late settlement can't debit a refunded session.

## 5. Milestones

Time figures are my guesses for one experienced developer, not measurements. Each milestone is usable on its own, and none depends on later ones to prove something.

| # | Goal | Exit criteria | Rough effort |
|---|---|---|---|
| M0 | First-instance checks on Verda | [Section 6's list](#6-what-still-needs-checking) answered, with results written down, including whether an inbound port beyond SSH reaches a CC instance and whether the public IP survives Shutdown and Start. First readings are on the stock image, and anything image-dependent is re-tested at M6 or M7. | About 2 h, or about $3 on spot and $6 on demand, once a minimal disk is ready. Building that disk (an EROFS root and a marker on a Verda-shaped layout) is about half a day, beforehand. |
| M1 | Money loop, no Tor, no TEE | `issue → reserve → execute → settle → recover` works over Unix sockets with a fake engine. Every crash point passes the invariants, as do the concurrency tests. SQLCipher read latency measured. | 2 to 3 weeks |
| M2 | Stripe sandbox flow with clearnet issuance | A sandbox card payment, confirmed by polling the Checkout Session, becomes an EC-ACT credential through `issue.sock`, and the credential spends in M1's loop. Retries return identical bytes, a different request gets `409 order_issued`, and a wrong secret gets `401`. Checkout's collected fields are recorded. Needs a sandbox key from you. | Under a week |
| M3 | Real metering | `llama-server` and SmolLM2 behind `InferenceBackend`, with golden chat-template tests and reconcile checks | 1 week |
| M4 | Tor and inner TLS | Wallet reaches gate through C tor, Arti and a `unix:` target with attested TLS against a fake evidence source. Failure rate and latency measured. Chutney flood test run, which decides whether wallets carry a PoW solver. The whole workspace builds. | 1 to 2 weeks, plus 2 to 3 days for Chutney |
| M5 | Custom image and measurement | Reproducible image boots on Verda, and a real report matches the predicted digest. A mismatch triggers diagnosis, then go or no-go. States whether it ran on the final or bring-up image. | 1 to 2 weeks; first boot attempt 1 to 2 days |
| M6 | vLLM and attestation on Verda | vLLM with GPU-CC, `nvattest` in boot order, wallet verifying a real report, with report rate measured. CC against non-CC overhead measured. States which image. CRL check turned on. | 1 to 2 weeks |
| M7 | Full demo, fault tests, update | Final image. Buy in the sandbox through the storefront on port 8443, chat over Tor, reconcile to the credit. Kill the gate, the wallet and the engine. Run the update path and the unreachable-gate path once. CRL enforced. | 1 week |
| Later | Tinfoil backend, handoff, traffic shaping | Needs contact with Tinfoil for GPU containers | Unknown |

M0 is independent and cheap, and it could change the image design or where the storefront runs, so do it first. M1 tests the design's core with no cloud bill. Pausing after M1 and M5 for a decision would be sensible.

## 6. What still needs checking

Each of these could change the design.

### First instance, M0

| Check | Could change |
|---|---|
| Which CPU signature matches a real report, 26/0/0 or 26/2/1 | Measurement tool settings |
| Extended report certificates | Whether VCEK comes from the host or from KDS |
| Whether a `grub.cfg` marker moves the command line, and which partition the extractor picks with an EROFS root | Disk layout, and possibly the entire approach |
| Whether an inbound TCP port beyond SSH, such as 8443, reaches a CC instance | Whether the storefront can run on the instance ([decision 3](#3-storefront-on-the-same-instance)) |
| Whether the public IP survives Shutdown and Start | Whether wallets pin an IP or a DNS name for the storefront |
| `chip_id` and a derived key across Shutdown and Start | Later key options |
| `author_key_digest` and `id_key_digest` on a real report | Whether signer-based sealing is possible |
| Stock `uname -r` and `dpkg -l`, including which CUDA wheels fit the driver | Package pins, wheels |
| `systemctl cat nvidia-persistenced`, `ps`, and `/etc/modprobe.d/` on stock | Persistence and LKCA units |
| `nvattest` with OCSP blocked, without root, and reading its own collected file | Boot failure behaviour, collector design |
| Whether Shutdown delivers a signal to the guest | Exact settlement versus full refund on Shutdown |
| Policy bits on a real report, and report rate | Manifest policy, the semaphore |
| `act-rs` timing on the instance, x86-64 with 30 vCPUs | Limits. So far it's only on an M5 Max. |
| External DNS from the guest with a static `resolv.conf`, under the boot rules and for the `storefront` user at runtime | Boot and runtime network units |
| Serial console access, and whether Verda treats an instance with no SSH as healthy | Bring-up plan, whether the image works at all |
| Volume clone or attach on a CC instance | Unreachable-gate path, and so whether a tor exit strands the store |

**A question for Verda, not a check:** what the Schedule 4 clause on refreshing NVIDIA CC output "at least once in any twenty-four (24) hour period" means for a tenant's boot-time appraisal. Ask before any instance runs past 24 hours.

Note for planning: Verda listed maintenance for FIN-03 on 8 November 2026, 19:00 to 23:00 UTC. [read]

### During the build

| Check | Milestone |
|---|---|
| SQLCipher read cost in the duplicate check, on a cold page | M1, then M6 on the target |
| `--replace` import over a store with a surviving `-wal`, on SQLCipher | M1 |
| What Checkout collects with a card-only filter, and the session fields collect reads (`payment_status`, `amount_total`, `currency`, `client_reference_id`, `metadata`) | M2 |
| Whether `hs-pow-full`'s LGPL dependencies are acceptable in the wallet binary | M4 |
| C tor, Arti and `unix:` failure rate, as "3 of 42 failures" wasn't measuring this pairing | M4 |
| Whether Arti's `exit(1)` behaves as read | M4 |
| Flood test, deciding the PoW solver question ([limit 14](01-system-and-data-flow.md#accepted-limits)) | M4 |
| Whole workspace builds, including `xtask` and `gate-conn`, which weren't in the resolver run | M4 |
| Two clean image builds match, and the initrd path works | M5 |
| The runtime firewall: port 8443 in, DNS and HTTPS out for the `storefront` user only, tor otherwise | M5 |
| tor's `Sandbox 1` on the image's glibc and kernel | M5 |
| `PrivateNetwork=yes` with NVIDIA's stack | M6 |
| vLLM 0.33.0 usage fields and abort timing under CC | M6 |
| CRL enforcement | M6, M7 |
| Full upgrade flow, `SIGTERM` behaviour, and `VACUUM INTO` or backup API on a raw-key store | M7 |
| `rsa` 0.9.10 and `crypto-bigint` 0.5.5 are verify-only in wallet and gate | Before first release |

### Outside the build

| Check | Before |
|---|---|
| A cryptographer's view of EC-ACT and `act-rs` | Live money. Ideally by the end of M1. |
| Stripe's view on stored value | Live money |
| A minimum session charge, so free sessions can't grow the store | Live money |
| NVIDIA commercial licence and attestation service terms | Any paid service |
| Whether Verda's contract covers tenants on NVIDIA CC terms | Any paid service |
| A supported AMD TCB floor | Removing the "experimental" label |

## 7. Kill criteria

These are the results that should change the plan, and what to do about each.

| Result | Then |
|---|---|
| EC-ACT fails review | [Section 2](#2-fallback-if-ec-act-fails-review) |
| No inbound port beyond SSH reaches a CC instance | Revisit [decision 3](#3-storefront-on-the-same-instance) before building the storefront into the image. [unknown] |
| The public IP changes across Shutdown and Start | Pin a DNS name in wallets instead of an IP. It still needs no certificate. |
| Verda's measurement can't be matched after diagnosis, or the command line or marker can't be pinned | Stop building on Verda. Tinfoil Containers becomes the candidate, which is [unassessed](#9-tinfoil-containers-as-a-target) and needs contact with them for GPUs. |
| GPU-CC and vLLM fail together, or are too slow to use | Try the smaller engine in the guest, then Tinfoil's backend. |
| C tor and Arti failure rate is too high | C tor as a client sidecar |
| Stripe refuses stored value | Blocks live payments only. Sandbox still completes the PoC. |
| NVIDIA licensing blocks a paid service | Blocks commercial use only. Development continues. |
| A forged-credit or inflation bug shows up in `credit` | Stop, whichever scheme. It's the core property. |
| Update flow can't work reliably | Accept voided balances on each update for the PoC, and bring forward the handoff design. |

## 8. What this design doesn't try to prove

- It doesn't prove anonymity. It's confidential inference with IP hiding at weak anonymity, in a pool of one or a few people.
- It doesn't prove operator-independent money. The operator can mint, and could refuse to honour credit.
- It doesn't stop a compromised storefront from minting. It only lets the operator detect that afterwards, with K.
- It doesn't prove the host is powerless. SNP has known physical attacks, and traffic timing stays visible.
- It doesn't detect a falsified usage count from a compromised gate, beyond gross inflation.
- It doesn't address Stripe's knowledge of the buyer.

## 9. Tinfoil Containers as a target

You want modular backends that ultimately include Tinfoil's private inference containers. This section is an assessment from Tinfoil's documentation only. Nothing has been tried. [read]

**What its documents say:** config must live in a public repository. GPU access needs contacting Tinfoil. Private secrets, encrypted volumes, attested keys, attested SSH and fresh attestation are listed as beta features. Tinfoil's description of persistent volumes excludes rollback protection.

| Part of this design | On Verda | On Tinfoil Containers | Status |
|---|---|---|---|
| tor service | In the measured image | A container that needs outbound Tor access, or a Tinfoil-terminated connection instead | Unknown whether containers can run tor and accept the network policy |
| Money store | SQLCipher on `data` | Encrypted volume in beta. Same rollback limit. | Should fit |
| K delivery | Operator over attested TLS | Could use the same flow, or private secrets | Same flow should work, if the attested-key feature exposes a usable SPKI |
| Attestation reports | Fresh SNP report bound to SPKI and nonce | Depends on Tinfoil's fresh attestation feature, in beta | Needs a `tinfoil-container` verifier, and possibly a change to `REPORT_DATA` |
| GPU evidence | `nvattest` in our boot | Tinfoil's stack | Unknown who appraises, and what a wallet can check |
| Image | Ours, by mkosi | Docker image, with Tinfoil's CVM image beneath it | Our boot units and firewall would mostly go away, along with some of their guarantees |

The abstractions that apply: `AttestationSource` and `AttestationVerifier`, the `platform` per release in the manifest, `KeyCustody`, and `InferenceBackend` where vLLM stays inside the container. The gate should move almost unchanged. The boot units and tor setup wouldn't. Tinfoil's hosted pooled API is a different case, because its usage counts aren't attested ([03 section 7](03-code-architecture.md#7-change-recipes)).

Before building on this, ask Tinfoil about GPU access, network egress for tor, and whether the beta features would be stable enough.
