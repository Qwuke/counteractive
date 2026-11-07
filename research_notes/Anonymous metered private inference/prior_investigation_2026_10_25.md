# Prior investigation by the user, 25 October 2026 (text copy)

**Status: a source of ideas, not evidence.** This is the text of the user's own earlier artifact, "Paying for Inference Without a Name" (https://claude.ai/artifact/RrzfxQrwbuN9txUDzdjDCA), which the user supplied on 2026-11-06 after the research tracks and spikes had finished. It was produced by AI agents, and it says its figures come from its own model rather than from measurement. None of the tracks or spikes consulted it. Where it conflicts with a spike, a primary source or `fact_check.md`, those win. See `prior_investigation_review.md` for an independent check, if that file exists.

**Differences already visible from the other notes (unverified beyond those notes):**
- ACT status: it says the drafts expired in August 2026. The current text is `draft-schlesinger-mole-ec-act-00`, dated 2 Nov 2026, after this document (`credentials_crypto.md`).
- Physical and firmware attacks: it does not mention Fabricked or BREAKFAST (`tee_gpu_attestation.md`).
- Session opening: it says about 1 to 2 s. `spike_tor.md` measured a 4.33 s median first byte on a fresh circuit and 1.37 s on a reused one, with cold starts around 7.7 s, on one laptop during a reported Tor incident.
- Spend proof size: it says about 2.5 KB. `spike_act.md` measured a 4,919 B spend at L=32. The balance bit length may explain some of this.
- Deployment: it assumes four H100 servers across two clouds at about $20k a month. The user's PoC targets Verda, where RTX PRO 6000 CC is about $2.03k a month in one location (`verda_browser_notes.md`). Its cost and custody figures do not transfer.
- Scope: it proposes a full architecture and anonymous payment rails. The current request is for technologies and concepts only, with card purchase.

**Topics no other note covers:** spending once per session with a balance held in the enclave, uniform credential chains, weekly purchase cohorts, paid cover sessions, host-side GPU and PCIe observation, a fixed-shape model engine, operator and custodian separation across providers, and zkAPI, Open Anonymity, ProxyGPT and funion as prior art.

---

Investigation · 25 October 2026

  Paying for Inference Without a Name
  How to sell metered LLM compute from confidential hardware so that the provider knows who bought credit, but not who used it.

    Anonymity loves company.

    Roger Dingledine and Nick Mathewson, title of their 2006 paper on usability and the network effect in anonymity systems

Confidential computing has changed what it means to trust an AI provider. A service such as Tinfoil runs models inside confidential virtual machines and GPUs, publishes the code it runs, and lets every client check by remote attestation that the machine answering its request runs exactly that code [ref 1]. If the attestation holds, nobody at the provider can read the prompt.

That protects what you said. It does nothing for who said it. Every request still carries an API key tied to an account and a card, and the provider records each request's time and token counts alongside it. Over months, that metadata describes a person: when they work, how heavily they rely on the model, when the pressure is on. Meta's Private Processing and Apple's Private Cloud Compute go further and hide the user's identity behind third-party relays and anonymous tokens [ref 2,3]. But they serve free, rate-limited features. Neither lets a provider sell arbitrary amounts of compute to customers it cannot identify.

This document asks whether we can have both. We want a service that sells metered tokens, where the provider may learn who buys credit but cannot learn who uses it, how much any buyer consumes, or which requests come from the same person. The investigation had three stages: a survey of prior art with an independent fact-check of each topic, four competing architectures attacked by adversarial reviewers, and a synthesized design that went through a further red-team round.

The answer is a qualified yes. The cryptography needed exists and is surprisingly small. The hard part is everything around it: timing, the shape of traffic, the physical custody of hardware, and the small number of users that any new service starts with.

## Contents

    What confidential inference hides

    Two planes

    Credit without accounts

    Sessions are the unit

    Where amounts leak

    Replacing the relay with Tor

    The stream is a fingerprint

    Anonymity loves company

    How far to trust the enclave

    Abuse, fraud and compulsion

    What remains unsolved

    How it compares

    Building it

    How this investigation was done

    Summary

## What confidential inference hides

It helps to be precise about what an observer can learn from a confidential inference request. There are three kinds of information. The content is the prompt and the response. The identity is an account, a card, or an IP address. The metadata links the two over time: timestamps, sizes, token counts, model choice. Attestation protects the content. Anonymity is about the other two.

Tinfoil is a useful baseline because it is unusually open about its design. Its router enclave forwards each request's API key and model name to a closed-source control plane, which decides whether to admit the request [ref 4]. After each response the enclave reports a usage record: the timestamp, API key, model, prompt, cached and completion tokens, the enclave that served it, and a request ID [ref 5]. Billing is settled afterwards against soft limits, and in Tinfoil's words "there is no per-request pre-authorization against the remaining balance" [ref 6]. Its FAQ lists the source IP, packet timing and byte counts among the things it can see [ref 1]. None of this is hidden or dishonest. It is what an account-based business needs.

Tinfoil has argued that oblivious HTTP (OHTTP) "only protects who sent the data, not the data itself" [ref 7]. That is a fair point about confidentiality, and it is also why OHTTP and attestation complement each other. Tinfoil's web-search feature shows the remaining difficulty. When the service proxies searches for everyone under one shared key, Tinfoil concedes that with a small user base, timing correlation stays viable "even if using additional proxy mechanisms like Tor or OHTTP" [ref 8]. That sentence, written about one feature, turns out to describe the central problem of this whole design.

Meta's Private Processing, which runs AI features in WhatsApp, adds the missing anonymity layer [ref 2]. Clients obtain anonymous credentials from an Anonymous Credential Service and send requests through an OHTTP relay operated by Fastly. Meta's gateway sees the credential but not the IP address; the relay sees the IP address but only ciphertext. Apple's Private Cloud Compute has the same shape, with RSA blind-signed single-use tokens and a random choice between relays run by Cloudflare and Fastly [ref 3]. Both are designed for features that cost the user nothing and are rate-limited per device, so neither needs to know how much compute a particular person consumed.

Two newer systems come closer to metering. Confident Security's OpenPCC sells credits as partially blind RSA signatures whose value is visible, charges the most a request could cost, and refunds the difference [ref 9]. That spend-then-refund pattern suits language models, because nobody knows how long a response will be until it ends. But OpenPCC issues refunds unblinded, its anonymity depends on client-side delays of five seconds by default, and it still routes through OHTTP relays. Apple has added cost tokens that correct over-estimated charges against a device's rate limit, and admits that because they are reconciled at its identified token issuer, "an observer of Apple-operated metering endpoints could try to re-link that identity to an otherwise unlinkable usage profile" [ref 3].

The relay is the other common weakness. Meta's relay is one company that sees the IP address and timing of every request. It is separated from Meta by contract and by cryptography, but it is still a single party, one subpoena or one data-sharing agreement away from undoing the separation.

  [figure]

  Figure 1. Who learns what. Amber marks information that identifies the user. Tinfoil identifies every request. Meta hides the IP address from itself but hands it to one relay company. In the proposed design the only identified step is the purchase, which nothing on the inference path can link back to.

## Two planes

The starting point is to accept that the purchase will be identified. People pay with cards, and card payments carry names. What we can do is make sure nothing about the purchase survives into the use of what was bought. We split the system into two planes:

  The identified plane takes payment. It learns who bought how much credit, and when.

  The anonymous plane serves inference. It processes what is asked, inside attested hardware, and never learns who asks.

The only thing that crosses from one plane to the other is a blind signature [ref 10]. The buyer's wallet generates random tokens, blinds them, and submits them with the payment. The purchase server signs the blinded tokens without seeing them. Later the wallet unblinds the signatures and presents the tokens on the anonymous plane, where they verify under the purchase server's public key but cannot be matched to the signing request. Cryptographically, the two planes are disconnected. Statistically, as we will see, they are not, and much of this design is about the statistics.

Figure 2 shows the components. A wallet daemon runs on the user's machine and exposes an ordinary OpenAI-compatible API on localhost, so applications need not change. It keeps two separate Tor states, one for purchases and one for use, so the two planes never share a circuit or a guard relay. On the anonymous side, each GPU server is a confidential VM that runs its own Tor onion service, metering, traffic shaping and the model. A separate ledger enclave holds the key that issues credit and the set of spent tokens. A public transparency log, cosigned by independent witnesses, records every key and software measurement the wallet is willing to trust.

  [figure]

  Figure 2. The two planes. The purchase server never talks to the GPU servers or the ledger. Every wallet downloads every cohort file over Tor, so even the download says nothing about who bought. The wallet refuses any key, address or measurement that the log does not contain.

## Credit without accounts

How should value move on the anonymous plane? The simplest answer is digital cash in the style of David Chaum, as in Privacy Pass [ref 11] or Cashu. Each token is a coin of fixed value, spent once. That works for an API call with a fixed price, but token compute has a variable price. A request's cost depends on how many tokens the model emits, which is unknown until the response ends. With fixed coins you either overpay or need change, and change in a coin system is an exchange of old coins for new ones that the issuer can watch. You could offer many denominations, but each denomination needs its own key, and each key splits the users into a smaller crowd.

What we want is a credential that carries a hidden balance. The holder should be able to spend some amount from it, prove the balance was sufficient without revealing it, and receive a fresh credential for the remainder that nobody can connect to the one they spent. That is the shape of a Zcash shielded transaction, and it is what Anonymous Credit Tokens (ACT) provide [ref 12].

An ACT credential is a keyed-verification credential over a hidden balance c. To spend s credits, the client reveals a random nullifier k and the amount s, and proves in zero knowledge that it holds a valid credential with c ≥ s. The issuer checks that k has never been seen, records it, and returns a blinded credential for c − s + t, where the refund t ≤ s is chosen by the issuer. That last parameter is the hook we need for unknown output lengths: reserve the price of the maximum output up front, and return the unused part when the stream ends. A credential and its successor are unlinkable in the information-theoretic sense, so the property holds even against an adversary with unlimited computing power, including a quantum computer. Unforgeability is not post-quantum. That threatens the operator's revenue, not the user's privacy.

ACT is not yet a standard. Its IETF drafts expired in August 2026, the editor's copy is not wire-compatible with the last published draft, and the Rust crate (version 0.4.2, which does ship partial refunds) has not been audited [ref 12,13]. Its closest standards-track relative, Anonymous Rate-limited Credentials, was abandoned by the Privacy Pass working group in 2026 because nobody could make it unlinkable against quantum adversaries [ref 14]. Any deployment therefore needs an audit and a fork it controls. The construction is small, though, and nothing better suited exists.

### Do we need Zcash?

  The idea that started this investigation was to describe value changes the way Zcash does: prove in zero knowledge that a change is valid instead of recording a transaction history, even though the ledger runs on centralized servers. That instinct is right. The question is how much of Zcash we need.

  A Zcash shielded spend proves several things at once [ref 15]. The spent note exists, shown by a Merkle path into a tree of every note commitment ever created, proven inside a zk-SNARK so the path stays secret. The note has not been spent before, shown by revealing its nullifier. Value is conserved, shown with homomorphic value commitments and a binding signature. And the recipient can find the new note, which is encrypted to them and posted on the chain. All of this exists because Zcash has no trusted party and many possible payees.

  Our setting is easier in one decisive way: the operator is always the payee. It can be trusted with the integrity of its own revenue, because forging or double-spending credit only hurts itself. So the Merkle tree can be replaced by the issuer's own signature on the balance commitment, as the Platypus central-bank design argues [ref 16]. The SNARK becomes a sigma protocol, and note encryption becomes handing the change straight back. What remains is a nullifier, a hidden balance, and a zero-knowledge proof that the balance covers the charge. That is ACT. In other words, ACT is a Zcash shielded pool specialised to a single verifier.

     | Zcash (Orchard) | ACT | Privacy Pass | 

      Proves value exists | Merkle path, inside a SNARK | Issuer's MAC on hidden balance | Issuer's signature on a token | 

      Stops double spends | Nullifier | Nullifier | Token serial | 

      Amounts | Hidden (value commitments) | Balance hidden, spend s revealed | Fixed per key | 

      Change | New output note | Blinded refund credential | None | 

      Who can verify | Anyone | Only the issuer | Depends on variant | 

      Client cost | SNARK proof, about a second | ~2.5 KB sigma proof, milliseconds | One blinding | 

  A full shielded pool would earn its cost in three situations: if several mutually distrusting operators had to share one anonymity pool, if keeping a minting key online were unacceptable, or if amounts had to stay hidden even from a physically compromised ledger. One of the four architectures we evaluated took that route, with a Groth16 circuit for a single two-in, two-out action. Reviewers found it bought little extra privacy against the attacks that actually worked, all of which went around the credential layer. It also added a custom circuit in which a soundness bug means undetectable inflation, a trusted setup ceremony, proving on phones, and the regulatory risk of running something that looks like a cryptocurrency. The recommendation is to keep the value layer behind one interface and switch to a shielded pool if one of those three situations arises.

## Sessions are the unit

Where in a conversation should a spend happen? The obvious answer, once per request, turns out to be the wrong one. Chat clients resend the whole conversation on every turn, so the turns of one conversation are linkable by their content anyway, at least to the enclave. Spending on every turn would add a ledger event per turn and buy no extra unlinkability.

Instead the session is the unit of unlinkability. One spend opens a session, bound to one Tor circuit, one encryption context, one server and one model. Inside the session, the enclave meters each turn against a balance held in its own memory. When the session ends, the unused value comes back once, as an ACT refund. Two recent designs arrived at the same unit: zkAPI, launched by the Ethereum Foundation with Open Anonymity in October 2026, and Open Anonymity's earlier unlinkable-inference architecture both use one anonymous spend per session to mint a short-lived, capped key [ref 17,18,19].

  [figure]

  Figure 3. One session. The ledger sees one spend and one refund, both carrying amounts but no identity, and the new credential is unlinkable to the old one. Every message rides the constant-cadence channel described later, so the host learns neither the amounts nor how many rolls the session needed.

Several details make this work.

Uniform chains. Every credential chain starts with the same value, $5, and the main spend tier is $0.50, a tenth of that. The ACT proof that c ≥ s reveals a lower bound on the balance, so if chains had different sizes, a large first spend would reveal a large purchase. With uniform chains every spend looks the same. A $20 voucher becomes four chains.

Rolls. A long job can exhaust the $0.50 in a session. The enclave then asks for another spend, from the same chain or the next idle one, before the balance runs out. This roll travels inside the encrypted session and the constant-rate traffic, so the host cannot count how many rolls a session needed and so cannot estimate its cost.

Concurrency. An ACT chain is strictly sequential, because the next spend needs the refund from the previous one, and there is no operation to split one chain into two. Concurrency therefore comes from holding several chains, which also makes chain count the rate limit: a wallet with four chains can run four sessions at once. Agent frameworks that fan out to many sub-agents share one session per application, which keeps the fan-out from becoming a fingerprint.

Failures. If the Tor circuit drops, the wallet reconnects on a fresh circuit and resumes with a session token, at most three times. If the GPU server dies, the ledger enclave notices the missing traffic within 60 seconds and refunds every pending spend in full. Every accepted spend has a deadline after which it is refunded automatically, so the operator cannot quietly keep a hold. Stored refunds are sealed to a key the wallet chose, so a refund fetched later, on a fresh circuit, is useless to anyone else.

  Table 1. Main parameters of the synthesized design. Every number is a starting point that the first build phase measures.

  Parameter | Value | Why | 

    Voucher | $20, one key per ISO week | One denomination, one crowd per week | 

    Chain | $5 face value, 4 per voucher | Spends cannot reveal purchase size | 

    Spend tier | $0.50, or $0.05 when a chain runs low | The only amount signal is “tail” | 

    Refund | Computed in the enclave, rounded to the cent | Coin from a commit-reveal between client and server | 

    Cadence | 10 frames/s down, 4 up, one Tor cell each | Timing and length invisible | 

    Session | 5-minute quanta, closes after 20 min idle | Duration reveals little | 

    Activation | Week W purchases usable from week W+2 | Payments settle first; first use blends into the cohort | 

    Key epoch | 6 months, overlapping issuance | Bounds forgery and nullifier storage | 

    Stored refunds | 45 days, sealed to the wallet | Offline wallets recover change | 

## Where amounts leak

A hidden balance does not hide the amounts that move. An ACT spend reveals s, and the refund t is computed by the server. Every metered system we surveyed concedes some form of this leak. OpenPCC exposes credit values and refunds, zkAPI reveals the total cost of each session, and Apple's cost reports flow back to the identified issuer. Once credentials are unlinkable, amounts combined with timing become the main way to link requests.

The design closes this leak in layers. The spend amount takes one of only two values. The refund is computed inside the GPU enclave and sent to the ledger enclave inside an encrypted, fixed-size message. The host and the transparency log never see an amount, and the ledger publishes only weekly aggregates. Discounts for cached tokens apply only within a session and are folded into the refund, so they cannot reveal that one session continues another.

Amount privacy against a live, physically compromised ledger enclave still rests on the hardware. That is the price of choosing ACT over a shielded pool. It is acceptable because such an attacker learns what each anonymous session cost, and that is still not who ran it.

## Replacing the relay with Tor

OHTTP provides anonymity by splitting knowledge between two parties: the relay knows who you are, and the gateway knows what you asked. Tor splits the same knowledge across a circuit of relays run by thousands of independent volunteers. An onion service removes the exit relay entirely: the client and the service each build a circuit to a rendezvous point, and no relay sees both ends [ref 20]. Nobody has to be contracted to protect anyone's metadata.

Can the existing Tor network carry inference, including streamed tokens? The measurements say yes. Tor Metrics for July to October 2026 put the median round trip to an onion service at 0.29–0.70 seconds and bulk throughput at 3–5 Mbit/s [ref 21]. A token stream needs a few tens of kilobits per second. Opening a session costs about one to two seconds, most of it the rendezvous, and the wallet hides part of that by building its half of the circuit in advance at random times. The real constraint is upstream. A 100,000-token agent context is about 410 KB and takes 1.3–3.2 seconds to upload over an onion circuit. Resending it every turn would be ruinous, so the server keeps the session's transcript and KV cache in enclave memory, and the wallet sends only what is new.

The onion service runs inside each GPU server's confidential VM. Its key, and the key for the inner end-to-end encryption, are generated there and never leave. Both are bound into the attestation report, so a wallet that verifies the report knows the onion address it reached terminates inside attested code, and that the host cannot impersonate it. Wallets fetch attestation evidence as a self-contained bundle over Tor instead of calling AMD's or NVIDIA's servers from their own IP address. Inside Tor, requests are encrypted again under a hybrid X25519 and ML-KEM key, because Tor's own handshakes are not post-quantum and recorded traffic may be decrypted in the future.

  [figure]

  Figure 4. The path of one session: about seven relays, no exit. The guard sees the wallet's IP address but not the destination, and the service's guard sees the server but not the wallet. Binding both keys into the attestation report means the onion address itself is evidence of the code behind it.

Some appealing shortcuts do not exist yet. Tor's single onion mode, which would shorten the service-side path, is labelled experimental and non-anonymous in the C implementation, and Arti, the Rust rewrite, removed it [ref 22,23]. Arti can host onion services, but only for testing, so the service runs C tor and the wallet embeds Arti. A proposal to carry anonymous tokens inside Tor's introduction handshake, Proposal 331, is unimplemented and has about 200 bytes to spare, too few for an ACT proof of about 2.5 KB [ref 24]. So the credential is presented in the first bytes of the stream under a ten-second deadline, and Tor's proof-of-work defence stays on to absorb floods from people who never pay.

## The stream is a fingerprint

Encryption hides the bytes of a token stream but not its shape. Streaming APIs typically send one event per token, so packet sizes track token lengths. Weiss and colleagues used this to reconstruct responses from encrypted traffic to commercial assistants [ref 25]. Tor's fixed-size cells blur individual token sizes, but timing alone still leaks. Microsoft's Whisper Leak study trained classifiers to detect a chosen topic in encrypted streams, and timing features alone averaged about 70 percent precision-recall; for heavily batched models, timing did as well as size [ref 26]. Adding random padding to each event, as Tinfoil's proxy currently does, helps little. The noise averages out over many events, and the count and timing of events remain.

The defence is to make the stream's shape independent of its content. During a session the server sends one Tor-cell-sized frame every 100 milliseconds, whether or not it has a token to put in it. Small requests ride a slower upstream cadence of the same kind, and larger uploads are padded to one of a few sizes, 8 KB times a power of four. Sessions last a whole number of five-minute quanta. Time to first token and response length become invisible on the wire. The cost is about 41 kbit/s downstream for each open session, roughly 18.5 MB per hour.

[figure]

  Figure 5. The same response, unshaped and shaped. Unshaped, an observer reads the time to first token, the token lengths and the length of the response. Shaped, the wire carries the same frame every 100 ms from the moment the session opens, and nothing changes when the response ends.

The wire is not the only place the shape shows. The host of a confidential VM can see when the GPU is busy and how much data crosses the PCIe bus, which reveals the same lengths. So the enclave runs the model in a fixed-shape loop: requests are admitted on 250-millisecond ticks, every decoding step processes a fixed batch, padded with dummy sequences when there is less work, and output moves in fixed-size transfers. This costs an estimated 20–35 percent of throughput at high load. The acceptance test is empirical: a classifier in the style of Whisper Leak, trained on captures from the host side as well as from Tor, must do no better than chance.

Constant cadence suits chat and desktop agents. Phones cannot keep a connection busy in the background, so they get a weaker profile in which the cadence runs only while a request is active, and the rhythm of an agent loop stays visible. The wallet labels that mode accordingly.

## Anonymity loves company

Suppose Alice buys $20 of credit with her card on Monday morning, and at 9:05 the first new session of the hour arrives at the service. No cryptography links the two events, but the operator does not need a proof. It needs a good guess, and when there are few users, timing makes guesses good. NCC Group's review of Google's Private AI Compute found exactly this pattern: a correlation between the timestamps of credential issuance and of inference, inside one operator, which no relay can remove [ref 27].

Cryptography makes the link impossible to prove. Making it hard to guess requires a crowd, and a new service has none. So the design removes the timing links it can and dilutes the rest:

  Vouchers bought in week W are signed together after the payments settle, and become redeemable at the start of week W+2. First use can then be attributed only to the whole cohort of that week's buyers.

  There is one voucher denomination and one signing key per week, published two weeks ahead. A key issued just to Alice would tag her, and the transparency log makes such a key visible to every wallet.

  Each voucher is redeemed at its own random time within a week of activation, and each chain then matures for a further random delay.

  The wallet buys on a calendar schedule and keeps a reserve of about six weeks of expected use. Purchases are never triggered by a low balance, because running out and buying more is itself a timing link.

  The wallet runs cover sessions that are paid for like real ones, half shaped like the user's own traffic and half like the population's, so a user's presence does not stop abruptly when they do.

The price is latency. Credit bought with a card becomes usable one to five and a half weeks after purchase, three weeks on average. Users who pay through non-identifying rails, such as Lightning, Monero or cash vouchers from resellers, do not need the delay, because there is no identity to decorrelate.

How large is the crowd? Figure 6 comes from a simple model built during the synthesis. For one user, it counts the other wallets active in the same hour that look the same to the operator after shaping. The features that survive shaping are the platform, timezone band, habitual hours, model and traffic profile, together worth 5–10 bits per wallet. At 300 wallets, a chat user has about three look-alikes and an agent user fewer than one, which means the operator can probably link an agent user's sessions to each other. At 30,000 wallets those numbers become 93 and 27.

    Chat
    Agent, desktop (constant cadence)
    Agent, mobile (cadence only while active)

[figure]

  Figure 6. Look-alike wallets active in the same hour, by growth phase, on a log scale. Below the dashed line, a user's sessions are likely linkable to each other by the operator. These are model estimates from the synthesis, not measurements.

  Table 2. The numbers behind Figure 6, with the heavy-user figure: how many candidates share the sessions of a user who consumes twenty times the typical rate (1 means fully attributable).

  Phase | Wallets | Chat | Agent, desktop | Agent, mobile | Heavy chat user | 

    Bootstrap | 300 | 2.9 | 0.9 | 0.3 | 1.23 | 

    Early | 3,000 | 13 | 3.7 | 1.1 | 2.0 | 

    Target | 30,000 | 93 | 27 | 8.3 | 8.3 | 

    Scale | 300,000 | 618 | 178 | 55 | 49 | 

Heavy users stay exposed the longest. The purchase server knows how much each buyer spends, and a user who consumes twenty times the typical rate stands out in usage too. At 300 wallets, such a user's sessions are attributable with probability around 0.8. Card purchase caps during the bootstrap phase, and steering heavy agent fleets toward anonymous payment rails, are the only mitigations.

The honest product response is a label. The service calls itself “confidential; weakly anonymous beta” until the numbers support more, and only a logged software release can remove the label. One rule matters more than it first appears: counts published by the operator may lengthen a delay or add a warning, but may never shorten a delay or remove a label, because the operator can inflate any count with orders and sessions of its own.

## How far to trust the enclave

A confidential VM protects its memory from the host's software. It was never designed to resist someone with physical access to the machine, and 2025 and 2026 showed how cheap that access has become. Battering RAM and WireTap used memory interposers on DDR4 servers to defeat Intel and AMD memory encryption [ref 28,29]. TEE.fail did the same on DDR5 with a passive interposer costing under $1,000. On Intel it extracted the attestation key in some cases, enough to forge quotes; on AMD SEV-SNP it recovered signing keys from inside a VM [ref 30]. DDRop drops memory writes with about $159 of parts, breaking several protections of Intel TDX and, more narrowly, of SEV-SNP [ref 31]. Intel and AMD consider such attacks out of scope.

The first principle of this design is therefore that unlinkability must never depend on the TEE. The blind signatures and ACT credentials stay unlinkable even if the server sees everything it receives in plaintext. The TEE is trusted for three narrower things: keeping prompts confidential from the operator's software and staff, metering honestly, and limiting what the host can log. A physical attacker who breaks a server reads the content passing through it, and content can identify its author. A recent preprint re-identified at least one conversation for 50–75 percent of heavy users in a dataset of coding-assistant chats, given a labelled history of each user's past conversations [ref 32]. Even then, the attacker learns nothing from the purchase records.

The second principle follows from the first: the operator must not be the custodian. It rents confidential capacity from cloud providers instead of owning hardware, and serves each model from several of them. The wallet tracks where its sessions actually land. If one provider's share is clearly larger than it should be, the wallet stops sending prompts in anonymous mode, because a single compelled provider would then see too much. The synthesis first set this threshold at half. The integrity review pointed out that with exactly two providers, one of them almost always holds more than half, so the rule needs at least three providers and a threshold with a statistical margin before the wallet claims anything.

The ledger enclave holds state, and state can be rolled back. A monotonic counter would protect only sealed state across restarts; it does nothing against an attacker who replays memory in a running machine. The design therefore replicates the nullifier set synchronously to a standby at a different provider before acknowledging any spend, commits state roots to the transparency log every five minutes, and accepts only state that extends the last logged root, in the spirit of ROTE and Signal's SVR3 [ref 33,34]. A later phase moves to a Byzantine-fault-tolerant quorum. Rollback mostly threatens the operator, since a double spend costs it money. The ledger inserts nullifiers at a fixed rate, padding with dummies, so replicas learn nothing about load.

Finally, a transparency log in the C2SP format, cosigned by three of five witnesses in different jurisdictions [ref 35], records every issuing key, credential context, onion address, server measurement and model weight hash. The wallet refuses anything unlogged and adopts new server software only after it has been in the log for a week. That blocks the cheapest targeted attack on a known buyer, which is to give them a key, a context or a server of their own.

Table 3 brings the threat model together. The adversaries are the operator's software and staff, the physical custodian of a server, an observer of the user's network or Tor guard, the operator and that observer together, and legal compulsion aimed at a buyer whose identity is known.

  Table 3. Which properties hold against which adversary, after both red-team rounds. “Partial” means the property holds only statistically, or with a bounded loss.

  Property | Operator | Physical custodian | Network observer | Operator + network | Compulsion vs. a buyer | 

    Prompt confidentiality | 
      Holdsattested keys, log adoption delays | 
      Failson tapped servers, at most that provider's share | 
      HoldsTor plus hybrid PQ encryption | 
      Holds | 
      Holdsnothing stored; changes visible in log | 

    Purchase ↔ use | 
      Holdscryptographic; statistical residue at bootstrap | 
      Partialcontent can re-link users with history | 
      Holdssees Tor use only | 
      Failsend-to-end timing correlation | 
      Partialfails where ISP and cloud flow records are kept | 

    Session ↔ session | 
      Partial5–10 bits per wallet; agents linkable early | 
      Partialcontent and style link heavy users | 
      Holdssees circuits, not destinations | 
      Fails | 
      Failssame as operator + network | 

    Balance integrity | 
      Partialatomic nullifiers; failover needs fencing, retention fix | 
      Partialkey theft: forgery until audits catch it; operator's loss | 
      n/a | n/a | 
      Holds | 

    No per-user keys | 
      Holdsfails only if 3 of 5 witnesses collude | 
      Partialstolen server key: that server, ≤ 2 days | 
      Holdswallets fail closed | 
      Holds | 
      Holdstagging is visible to all | 

    Refund fairness | 
      Partialholds once refund inputs persist and redemption is idempotent | 
      Partialcan short refunds, ≤ $5 per chain; detectable | 
      Holds | 
      Holds | 
      Holdsthe buyer's chains are unknown | 

## Abuse, fraud and compulsion

Bearer credit bought with a card invites chargebacks. Because vouchers are signed only after the payment settles, a chargeback during the holding period simply means the voucher is never signed. But card disputes can arrive 120 days or more after a payment, long after a hold of one or two weeks, so most disputes would land after the credit is already anonymous and spent. The synthesis priced that loss at about five percent, which the integrity review notes is far above the dispute rates at which card networks put a merchant under monitoring. The card rail is fragile, and the business has to be viable on irreversible rails: Lightning, Monero, cash vouchers and resellers. There is no cash-out, ever. A refund to a card or bank account would reveal consumption, which is the lesson of Apple's cost tokens, and it would turn any undetected forgery into money.

Without accounts, there is no one to ban. Tinfoil's moderation exports a violation bit tied to an account and suspends repeat offenders [ref 36]. LLM-Tor attaches a permanent blind-signed abuse token to every request, which makes all of one user's requests linkable [ref 37]. Neither fits here. The design runs an open-weights policy model inside the enclave, logged like any other code, and its only sanction is to refuse within the stream. A refusal changes nothing observable: the cadence, duration and refund follow their usual distributions. Anonymous blocklisting schemes could add a stronger sanction without linking users, but none is mature enough yet.

What could a court compel? The operator holds payment records and the week in which each purchase was signed, and nothing that links further. The ledger holds random nullifiers without timestamps, and refunds sealed to wallet keys. The weak point lies outside the operator. If the user's ISP and the cloud providers both keep flow records, the start and end times of the user's Tor connections can be matched with the start and end times of sessions at the servers, after the fact. An optional flat-link mode keeps the wallet's connection to Tor at a constant rate while it is online, at about 0.6 GB per day, which defeats this passive matching. It needs a source of downstream padding that the operator does not control, and whether the live Tor network can provide one is still to be measured.

## What remains unsolved

Some problems this design does not solve, and it is better to state them plainly.

An operator colluding with, or compelled together with, the user's ISP or Tor guard can correlate traffic end to end, and the host can make that easy by injecting timing watermarks into the stream. This is the classic limit of low-latency anonymity networks. Only a mixnet resists it, at a cost of around ten seconds per turn and no streaming [ref 38]. A mixnet mode for high-risk users stays on the research track.

Physical custodians can read content on the machines they tap. Spreading servers across providers limits how much any one of them sees, and that is all it does.

A new service has too few users for strong anonymity. During bootstrap, agent sessions are linkable to each other and heavy users are exposed. No protocol fixes this. More users, anonymous payment rails and honest labelling are the only remedies.

Several components are immature. ACT is unaudited and unstandardised, neither ACT nor RSA blind signatures resist quantum forgery, Arti's onion hosting is for testing only, and flat-link padding is unproven on the live network.

Agents that execute model output on the user's machine can be steered into contacting a host from the user's own IP address. The wallet offers a Tor egress proxy for agent tools and flags commands that touch the network, but a framework that bypasses the proxy is outside what the design can protect.

Refunds have no cryptographic recourse. A compromised server or ledger can shortchange a refund, by at most the balance of the chains in that session. The wallet recomputes every charge and detects this, but it cannot recover the money.

Finally, the bookkeeping needs another pass before anything ships. Each high-severity finding of the integrity review has a direct fix. Keep nullifiers for as long as any key can still be spent or migrated. Persist the inputs that deadline refunds need. Make redemption idempotent by storing each response under its token. Give paying traffic priority on the channel between servers and ledger. Size the engine for the model actually served. None of these touches the cryptography, but each would cost honest users money if shipped as written.

## How it compares

Each system we surveyed makes one of three properties strong: anonymity, confidentiality, or metered payment. LLM-Tor is most likely the “LLM Tor” work this investigation set out from. It is an eight-page whitepaper and open-source proxy published in March 2026, and it has not been peer reviewed [ref 37]. Users buy credit under their identity, receive RFC 9474 blind-signed single-use tokens, and redeem them over Tor at a proxy that forwards plaintext prompts to commercial APIs, at a flat price per request. It has the right split between purchase and use, which this design keeps, while dropping the plaintext proxy, the exit relay and the permanent pseudonym. Open Anonymity's unlinkable inference is the other plausible referent [ref 19], and ProxyGPT, a research system that pairs Tor with volunteer browser proxies and Chaumian e-cash, is the closest research relative [ref 39].

  Table 4. The proposed design against prior art, as of October 2026.

   | Tinfoil | Meta PP | OpenPCC | zkAPI | OpenAnonymity | LLM-Tor | This design | 

    Identity at use | API key on every request | ACS token, limited reuse | Value-bearing blind RSA credit | ZK proof, session key | Blind ticket, ephemeral key | Blind token plus permanent abuse token | One ACT spend per session | 

    Provider learns per use | Time, key, model, exact tokens | Request timing | Credit value, refund | Dollars per session | Fixed ticket price | All of a user's requests linked | Host: open time, duration quantum. Ledger enclave: tier and refund | 

    Metering and change | Billed afterwards, soft limits | None; free | Maximum spend, unblinded refund | Receipt-signed refund | None | Flat price | Maximum spend, blinded refund | 

    Network | Clearnet | One OHTTP relay | Third-party OHTTP | Advises fresh Tor circuit | Several proxies | Tor to a clearnet proxy | Onion service inside the CVM | 

    Confidentiality | Attested TEE | Attested TEE | Attested TEE | Provider sees content | Optional TEE gateway | None | Attested TEE, shaped GPU engine | 

    Traffic shaping | Random padding | None | None | None | None | None | Constant cadence, buckets | 

Others do some things better. zkAPI's on-chain vault gives users a refund path the operator cannot censor. The RSA-based designs rest on published RFCs and audited libraries. And Meta's enormous fleet makes it genuinely hard to target one user's requests, which four servers cannot match.

## Building it

The synthesis proposes building in phases, each gated on measurement. The first two months measure the things this document estimates: onion upload times from several continents, Arti's circuit isolation, the throughput cost of the constant-shape engine on confidential GPUs, and whether a Whisper-Leak-style classifier on host and Tor captures does better than chance. They also produce a fork of the ACT crate with one small change, binding each spend proof to the session, and a legal memo on closed-loop prepaid access and gift-card rules. A closed alpha follows in months two to six, with one model on four servers at two cloud providers and an external audit, then a public beta with anonymous payment rails, cover traffic and flat-link mode.

It is not cheap. Four confidential H100 servers cost around $20,000 a month, while 300 wallets spending $25 a month bring in $7,500, so the bootstrap phase runs at a loss that resellers and enterprise bundles would have to fill. The synthesis estimated serving cost at about $5.40 per million tokens including the shaping overhead, which would mean prices two to four times the market rate for open models. The integrity review found that this figure, and the fixed-shape engine's 20-millisecond step, hold only for small models of around eight billion parameters. A 32–70 billion parameter model cannot meet that step time on one H100, and long contexts fit only one to four per server, so larger models need multi-GPU confidential servers and cost correspondingly more per token. Privacy has a price, and here it is paid in GPU hours and in patience.

## How this investigation was done

The investigation ran as two orchestrated workflows of AI agents, with one more review at the end. Each stage was designed to be checked by an agent that had not produced it.

  [figure]

  Figure 7. The investigation as a pipeline. Each stage's output was attacked by an agent that did not write it.

In the research stage, seven agents each surveyed one topic: Tinfoil; Meta, Apple and OpenPCC; anonymous credentials; shielded value on a central ledger; Tor for streaming; the “LLM Tor” paper and the wider landscape; and stateful TEEs. Each wrote notes with primary sources, and an independent fact-checker then re-verified the four to six claims in each topic that would most change the design. Of 42 claims checked, 20 were confirmed as written and 22 needed qualification. None was refuted outright, but the qualifications mattered:

  ARC, the obvious standards-track tool for anonymous rate limiting, had been declared dead by its working group. The design uses chain count as its rate limit instead.

  TEE.fail extracted attestation keys only on Intel, and only in some cases; DDRop's effect on SEV-SNP is limited to copying pages. The design's claims about physical attacks were narrowed accordingly.

  Whisper Leak's headline numbers come from detecting one chosen topic, which is narrower than general topic classification but still enough to justify shaping.

  Apple's limit on how many servers can see a request applies only to the first hop; inside the data centre a gateway picks a single node.

  A realistic mixnet mode costs about ten seconds per turn, several times the first estimate.

  Tinfoil's remark that timing correlation defeats Tor or OHTTP was about its web-search provider specifically, and about a small user base.

A completeness critic then read everything and found eight gaps, from the lack of a shared threat model to the absence of any design for hiding amounts. Those gaps became requirements for the design stage.

In the design stage, four agents each wrote a complete architecture from a different starting point: standards first, the shielded pool, traffic analysis first, and adversaries and business reality first. Three adversarial reviewers attacked all four, each through one lens. No proposal scored above 6.5 out of 10 on any lens.

  Table 5. Adversarial review scores out of 10. The synthesis took the standards-first proposal as its base and grafted the best surviving ideas from the others.

  Proposal | Network and timing | Protocol | Custodian and business | 

    Standards first | 5.0 | 6.5 | 5.5 | 

    Shielded pool | 4.5 | 6.5 | 5.0 | 

    Traffic analysis first | 6.0 | 5.0 | 4.5 | 

    Adversary and business first | 4.0 | 4.5 | 6.0 | 

The standards-first proposal contributed uniform chains and the cap on spend tiers. The traffic-analysis proposal contributed weekly cohort vouchers, the constant cadence and the constant-shape GPU engine. The business proposal contributed the separation of operator from custodian and the inventory of records that would exist under compulsion. The shielded-pool proposal contributed per-server keys and the idea of letting the enclave settle charges without exporting them.

Two red teams then attacked the synthesis. The anonymity red team reported one critical attack, five of high severity and two of medium severity. The critical attack was the retrospective matching of ISP and cloud flow records described above. It led to purchases over a separate Tor state, stripping timestamps and server identifiers from the ledger, and flat-link mode. Among the high-severity ones, roll messages had been large enough to show up as bursts, letting the host count them and so estimate each session's cost; every control message now rides the cadence. Two of the eight were fixed outright. The other six were weakened, and what remains of them appears among the unsolved problems above. The red team also caught eight factual errors in the synthesis, such as a spend proof needing six Tor cells rather than five.

The second red team, on balance integrity and the economics of the business, was stopped mid-run by a content filter, so the revision ran its own integrity pass in its place. That pass added a fee for holding sessions idle and reconciliation of minted value against processor settlement statements. A rerun of the review, worded around design weaknesses instead of attacks, then found nineteen issues, seven of high severity. The cryptographic core held up: a refund can never exceed its spend, and two refunds of one spend share the next nullifier, so only one can ever be used. The problems were in the bookkeeping around it. Nullifiers were to be deleted after an epoch while old credentials could still be migrated for five years, so a kept copy could be spent twice. Deadline refunds needed data the ledger did not store, and a lost redemption response cost the buyer a whole voucher. The 50 percent rule per cloud provider fires almost always when there are only two. Unpaid work could crowd the narrow channel between servers and ledger. And the engine and cost figures fit only small models. These findings arrived after the synthesis was revised. Where they change a claim on this page, the text above has been corrected; the rest are listed as open work.

## Summary

Confidential inference hides what you say. This investigation asked whether it can also hide who you are while still charging you by the token. The core of the answer is small. Pay under your own name for blind-signed vouchers, turn them into Anonymous Credit Tokens with a hidden balance, spend once per session, and let an attested enclave meter the session and hand the change back as a fresh, unlinkable credential. That is the Zcash idea specialised to a single verifier, and it needs no blockchain and no SNARK. Carry the whole session, streamed tokens included, over a Tor onion service whose keys live inside the confidential VM, and the centralized relay disappears.

The cryptography leaves the purchase unlinkable to the use even if every enclave is broken. What it cannot remove is the statistics. Token streams leak through their timing, so the stream must run at a constant cadence, on the wire and on the GPU. Purchases leak through their timing, so credit must arrive in weekly cohorts, weeks after payment. And a small crowd leaks through its size, so a new service must say honestly that it is weakly anonymous until enough people use it. The design is buildable with parts that exist in 2026, at a premium price and with a few research problems left open. Its strongest guarantee does not depend on the hardware, and its weakest point is the one Tor has always had: an adversary who can watch both ends.

## References

  Tinfoil, “Security and privacy FAQ,” updated 12 October 2026. tinfoil.sh/security-and-privacy-faq 

  Meta, “Private Processing technical whitepaper,” version 3, October 2026. ai.meta.com/static-resource/private-processing-technical-whitepaper 

  Apple, “Private Cloud Compute” security documentation, including request flow and cost tokens. security.apple.com/documentation/private-cloud-compute 

  Tinfoil, confidential-model-router, route context and admission. github.com/tinfoilsh/confidential-model-router 

  Tinfoil documentation, “Attestation architecture.” docs.tinfoil.sh/verification/attestation-architecture 

  Tinfoil documentation, “Admin API: how usage is recorded.” docs.tinfoil.sh/admin/admin-api 

  Tinfoil, “OHTTP is often not the right tool for AI privacy,” 10 May 2025. tinfoil.sh/blog/2025-05-10-ohttp 

  Tinfoil, “Private AI web search,” 22 January 2026. tinfoil.sh/blog/2026-01-22-private-ai-web-search 

  Confident Security, “OpenPCC” whitepaper and anonpay. github.com/openpcc/openpcc 

  F. Denis, F. Jacobs and C. A. Wood, “RSA Blind Signatures,” RFC 9474, 2023. rfc-editor.org/rfc/rfc9474 

  A. Davidson et al., “The Privacy Pass Architecture,” RFC 9576, 2024. rfc-editor.org/rfc/rfc9576 

  S. Schlesinger et al., “Anonymous Credit Tokens,” draft-schlesinger-privacypass-act-01, expired August 2026. datatracker.ietf.org/doc/draft-schlesinger-privacypass-act 

  anonymous-credit-tokens Rust crate, version 0.4.2 (unaudited). github.com/SamuelSchlesinger/anonymous-credit-tokens 

  IETF Privacy Pass working group, draft-ietf-privacypass-arc-crypto, history (Dead WG Document, 2026). datatracker.ietf.org/doc/draft-ietf-privacypass-arc-crypto/history 

  D. Hopwood et al., “Zcash Protocol Specification.” zips.z.cash/protocol/protocol.pdf 

  K. Wüst et al., “Platypus: A Central Bank Digital Currency with Unlinkable Transactions and Privacy-Preserving Regulation,” ACM CCS 2022. eprint.iacr.org/2021/1443 

  D. Crapis and V. Buterin, “ZK API usage credits: LLMs and beyond,” ethresear.ch, 11 February 2026. ethresear.ch/t/zk-api-usage-credits-llms-and-beyond/24104 

  Ethereum Foundation, “Introducing zkAPI,” 1 October 2026. blog.ethereum.org/2026/10/01/introducing-zkapi 

  Open Anonymity, “Unlinkable inference as a user privacy architecture,” February 2026. openanonymity.ai/blog/unlinkable-inference 

  Tor Project, “Rendezvous specification: protocol overview.” spec.torproject.org/rend-spec 

  Tor Metrics, OnionPerf latency and throughput, July–October 2026. metrics.torproject.org 

  Tor manual, HiddenServiceSingleHopMode. gitlab.torproject.org/tpo/core/tor 

  Arti, “Onion services” documentation and changelog. gitlab.torproject.org/tpo/core/arti 

  Tor Project, “Proposal 331: Res tokens for anti-DoS” (draft). spec.torproject.org/proposals/331 

  R. Weiss, D. Ayzenshteyn, G. Amit and Y. Mirsky, “What Was Your Prompt? A Remote Keylogging Attack on AI Assistants,” USENIX Security 2024. usenix.org 

  Microsoft, “Whisper Leak: a side-channel attack on remote language models,” November 2025. arxiv.org/abs/2511.03675 

  NCC Group, “Public report: Google Private AI Compute review,” November 2025. nccgroup.com 

  “Battering RAM.” batteringram.eu 

  “WireTap.” wiretap.fail 

  “TEE.fail,” IEEE S&P 2026. tee.fail 

  “DDRop,” ACM CCS 2026. github.com/ddropattack/ddrop 

  Pham et al., “Can Prompt Anonymity Protect Your Identity From LLM Providers?” arXiv:2609.33903, preprint under review, September 2026. arxiv.org/abs/2609.33903 

  S. Matetic et al., “ROTE: Rollback Protection for Trusted Execution,” USENIX Security 2017. usenix.org 

  G. Connell et al., “Secret Key Recovery in a Global-Scale End-to-End Encryption System” (SVR3), OSDI 2024. usenix.org 

  C2SP, “tlog-witness.” github.com/C2SP/C2SP 

  Tinfoil, “Safety without compromising privacy,” 14 September 2026. tinfoil.sh/blog/2026-09-14-safety-without-compromising-privacy 

  P. Gupta, “LLM-Tor: Unlinkable Access to Commercial LLM APIs via Blind Signatures and Tor,” whitepaper, March 2026. github.com/prince776/LLM-Tor 

  “funion,” anonymous inference over a mixnet, arXiv:2501.02933. arxiv.org/abs/2501.02933 

  D. Pham et al., “ProxyGPT: Enabling User Anonymity in LLM Chatbots via (Un)Trustworthy Volunteer Proxies,” arXiv:2407.08792. arxiv.org/abs/2407.08792 

  Written from research notes, four design proposals, three adversarial reviews and two red-team reports produced by orchestrated AI agents, then edited into a single account. Figures marked as estimates come from the synthesis's own model and are targets for measurement, not results.
