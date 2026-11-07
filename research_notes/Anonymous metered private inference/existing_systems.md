# Existing private inference systems: prior art and reusable components

Research date 2026-11-06. This is a source-only investigation: no third-party code was built or run, and no Tor client was used. One read-only, no-credential GET was made over clearnet to Tinfoil's public attestation endpoint. Nothing here has been tested in practice unless a line says so.

How to read the evidence:
- "Cited Findings" are verified from a primary source. "Inferences" are my reasoning. "Gaps" are unknown.
- Meta, NCC, Trail of Bits, OpenPCC and the WiSec paper were read as PDF text extracted with a stdlib script (`/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/existing_systems_pdftext.py`). Quotes from those are from the document text.
- Web pages and GitHub source were read through a summarising fetch tool. Treat those quotes as close but re-read the file before building on an exact line. Where a fetch reported something as absent, that means absent from the page or file fetched, not absent from the project.
- Quotes, repo names and page numbers in these notes were re-checked on 2026-11-06 against the recorded tool output and the extracted PDF text. That confirms they match what was fetched, not that the fetch summary matched the page.
- Page numbers are printed page numbers. For Meta, printed = PDF page minus 3. For Trail of Bits, printed = PDF page minus 1. For NCC, printed = PDF page.
- The Trail of Bits report was read from a PDF mirror on cyberinsider.com. The canonical listing is the [Trail of Bits library page](https://trailofbits.com/library/meta-whatsapp-private-processing/). The mirror was not compared with any copy from Trail of Bits.

Versions relied on: Meta whitepaper V3 (2026-10-20); NCC report v1.0 (2025-08-27); Trail of Bits report (2025-08-26, published 2026-04-07); Tinfoil docs, Terms and AUP as served 2026-11-06 (Terms and AUP last updated 2026-10-12; V3 attestation in beta); Tinfoil router PRs up to #532 (merged 2026-11-03); tinfoil-go around v0.16.0 (release date not captured); OpenPCC whitepaper November 2025, `main` last commit 2026-01-08 as shown; LLM-Tor whitepaper March 2026; Apple blog 2024-06-10; WiSec '26 PCC analysis arXiv 2605.24239v1 (2026-05-26); Brave post 2026-10-15; Routstr PRs #736 (2026-09-17) and #882 (open at 2026-10-30).

## Tinfoil: architecture, metering, portability, terms

### Takeaway
Tinfoil gives you most of the transport and metering plumbing already: a documented pooled-key proxy pattern, and token counts available to that proxy in a header or trailer without decrypting the body, which contradicts the conversation. But its image and verifier are not a portable appliance for a rented CVM, the usage value is an unsigned string that no third party can check, Tor reachability is undocumented, and the Terms do not clearly permit anonymous resale.

### Cited Findings

**What runs where**
- Clients verify only the router enclave (`inference.tinfoil.sh`); the router then verifies each model and tool enclave before forwarding — [How verification works](https://docs.tinfoil.sh/verification/verification-in-tinfoil.md).
- The router verifies the hardware quote (SEV-SNP or TDX), resolves the model repo's release and verifies its Sigstore bundle including Rekor inclusion, compares measurements, and pins the TLS key from the attestation. "Data is encrypted end-to-end at each hop using attested keys." — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- The model-to-repository mapping is compiled into the router's measured binary, and the Tinfoil backend supplies only hostnames at runtime. Billing and account management are closed-source components that the page says do not receive content — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- The boot process checks each GPU with NVIDIA's `nvattest` and aborts the boot if verification fails. Weights are checked with dm-verity against modelwrap commitments — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- Hardware stated: "AMD SEV-SNP and Intel TDX CPUs" with NVIDIA Hopper and Blackwell GPUs in confidential compute mode — [Security and Privacy FAQ](https://tinfoil.sh/security-and-privacy-faq).
- Attestation proxies are explicitly untrusted: "a proxy can withhold material but cannot forge it" — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- Live read on 2026-11-06 (the one observation in this track): `atc.tinfoil.sh/attestation` returned keys `domain, enclaveAttestationReport, digest, sigstoreBundle, dsseEnvelope, vcek, enclaveCert`, domain `inference.tinfoil.sh`, report format `.../predicate/sev-snp-guest/v2`, predicate `.../snp-tdx-multiplatform/v1`, and certificate SANs under `*.hpke.tinfoil.sh` and `*.hatt.tinfoil.sh` — [atc.tinfoil.sh](https://atc.tinfoil.sh/attestation).
- V3 attestation is beta. CLI attestation verification uses V3, and V3 verification in the Go SDK, Swift SDK and Tinfoil Proxy is beta when a build uses it. The inference API as a whole is not labelled beta, but a V3-based client reaching it is on a beta verification path, and SDK support varies by release — [Feature status](https://docs.tinfoil.sh/resources/feature-status.md).

**EHBP**
- Encrypts bodies only and leaves headers in the clear. HPKE (RFC 9180) with X25519_HKDF_SHA256, HKDF_SHA256 and AES_256_GCM; keys published at `/.well-known/hpke-keys` as `application/ohttp-keys`; framed streaming with a 4-byte length prefix; no end-of-message sentinel — [EHBP docs](https://docs.tinfoil.sh/resources/ehbp.md).
- Verbatim design intent: "Proxies can inspect and route upon request metadata without seeing the body." Go reference plus JS and Swift clients — [EHBP repo](https://github.com/tinfoilsh/encrypted-http-body-protocol).
- EHBP shipped October 2025 as a breaking SDK change — [Changelog](https://docs.tinfoil.sh/resources/changelog.md).

**Pooled-key proxy and usage count**
- The proxy pattern is documented: the proxy holds `TINFOIL_API_KEY`, authenticates users with its own token, swaps it, and forwards. It can read and modify headers but not bodies, and does not see prompts, completions or the `model` field — [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md).
- The Admin API docs recommend exactly this for per-user billing: run a proxy that "tracks token counts via response headers instead of creating a separate API key per user" — [Admin API](https://docs.tinfoil.sh/admin/admin-api.md).
- The proxy opts in with `X-Tinfoil-Request-Usage-Metrics: true`. Counts come back in `X-Tinfoil-Usage-Metrics` as a header (non-streaming) or HTTP trailer (streaming), for example `prompt=67,completion=42,total=109,model=gpt-oss-120b,cost_usd=0.000042`. Optional fields include cached and uncached prompt tokens, web search calls and other costs — [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md).
- Output ceilings: `X-Tinfoil-Max-Output-Tokens` was merged 2026-10-18 so "proxies in front of the router can know and rely on a request's output limit" — [router PR #518](https://github.com/tinfoilsh/confidential-model-router/pull/518). `X-Tinfoil-Completion-Count` was authored by `sh1ftred`, who also authors Routstr PRs such as [#736](https://github.com/Routstr/routstr-core/pull/736), and merged by `sachaservan` on 2026-11-03, because a proxy cannot bound output when the sealed body sets `n`. An AI review summary on the PR notes it is not a request-wide budget, since it does not cover server-side tool loops, retries or repeated calls — [router PR #532](https://github.com/tinfoilsh/confidential-model-router/pull/532).
- The proxy must enforce the cap itself: "Clients can leave the header out or declare any value, so your proxy enforces the cap" — [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md).
- Where the count originates: for streams the router forces `include_usage` and `continuous_usage_stats` so "billing can extract token counts" — [router main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go). The JSON path reads `usage` from the model response (buffer limit 10 MiB) and the SSE path delegates to `tokencount` — [router proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go).
- Field reports: streaming trailers can be emitted twice in a single comma-joined field — [Routstr PR #736](https://github.com/Routstr/routstr-core/pull/736); repeated fields also caused a settlement rejection elsewhere — [possums PR #60](https://github.com/ajbt200128/possums/pull/60).

**Authenticity of usage**
- The proxy guide says nothing about signing or tamper-evidence of metrics — [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md). The router emits a plain formatted string; nothing in `proxy.go` or `usage_writer.go` signs it — [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go), [usage_writer.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/usage_writer.go).
- The EHBP README makes no statement on authenticated trailers — [EHBP repo](https://github.com/tinfoilsh/encrypted-http-body-protocol).
- Router-to-control-plane events are a separate channel: a comment refers to a "signed usage-reports ingestion endpoint", using a `ReporterID` and `Secret`. Event fields include `UserID`, `Model`, token counts, `RequestID`, `Enclave`, `RequestPath`, `Streaming`, `APIKey` — [billing/events.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/billing/events.go). Docs confirm records carry API key, request identifiers, route and enclave, and exclude prompt and response text — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- Implementers acknowledge the trust: Routstr's doc says "the proxy relies on Tinfoil's usage metrics being accurate", and that PPQ's EHBP path is disabled until it has "a trusted usage and model-binding strategy" — [Routstr EHBP doc](https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md).

**What Tinfoil says it can observe**
- Network metadata: "source IP, packet timing, byte volume, the fact that you are talking to Tinfoil", seen by Tinfoil and infrastructure providers (Cloudflare, AWS, GPU clouds). Per-request billing metadata: input and output token counts, model, timestamp. Clerk handles authentication logs — [FAQ](https://tinfoil.sh/security-and-privacy-faq).
- Hardware limits acknowledged: key extraction on SEV-SNP and attestation forgery on TDX have been demonstrated; side channels, I/O patterns and client compromise remain — [FAQ](https://tinfoil.sh/security-and-privacy-faq), [Enclave primer](https://docs.tinfoil.sh/verification/secure-enclave-primer.md).
- Non-targetability, Tor, VPNs and anonymity networks are not addressed. Shared keys for web search "is not a guarantee of anonymity" — [FAQ](https://tinfoil.sh/security-and-privacy-faq).
- Limits are soft and "not a reservation system"; in-flight work does not count — [Admin API](https://docs.tinfoil.sh/admin/admin-api.md).
- Prompt cache: scoped by API identity, `user_cache_secret`, model and prefix. "Without a per-user secret, requests under the same API identity share one cache", and hit timing can leak between users. No off switch is described, but a fresh secret per request prevents hits. State stays in GPU memory of the serving enclave — [Prompt caching](https://docs.tinfoil.sh/sdk/prompt-caching.md).

**What "private inference containers" are**
- Two separate things carry similar names. Hosted model enclaves are built from `confidential-*` repos such as `confidential-kimi-k3-1-pro` — [tinfoilsh org](https://github.com/tinfoilsh). "Tinfoil Containers" run a customer's Docker image inside Tinfoil-operated enclaves; GPU access needs contacting Tinfoil, `tinfoil-config.yml` must live in a public repo, and self-hosting is not mentioned — [Containers overview](https://docs.tinfoil.sh/containers/overview.md).
- Release pipeline: `tinfoil-config.yml` pins firmware, CVM image and weight hashes; `measure-image-action` builds and publishes a signed Sigstore bundle — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md).

**Portability of the image**
- `cvmimage` targets SEV-SNP and TDX, is built with Nix on x86_64 Linux, and claims hermetic reproducibility — [cvmimage README](https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/README.md). Docs state anyone can rebuild and get identical artifacts — [CVM image docs](https://docs.tinfoil.sh/verification/cvm-image.md). Licence AGPL-3.0 — [cvmimage](https://github.com/tinfoilsh/cvmimage).
- Booting on SEV-SNP or TDX "needs Tinfoil host access", per the repo's contributor notes — [cvmimage AGENTS.md](https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/AGENTS.md).
- The kernel, initrd and firmware are "loaded directly and measured at launch", and the dm-verity root hash is on the measured command line — [CVM image docs](https://docs.tinfoil.sh/verification/cvm-image.md).
- TDX platform measurements are reproduced from inputs that must mirror production, including QEMU version and PCI aperture, and the README refers to Tinfoil's `tinfoild` — [hardware-measurements](https://github.com/tinfoilsh/hardware-measurements).
- `tinfoild` is treated as private: a PR changed the docs to "describe what hosts must support to deploy signed releases instead of naming the internal tool" — [measure-image-action PR #85](https://github.com/tinfoilsh/measure-image-action/pull/85). No public repo was found in search; the `tinfoil-config` README names it as sharing the config contract — [tinfoil-config](https://github.com/tinfoilsh/tinfoil-config).
- `measure-image-action` measures with `sev-snp-measure` and `tdx-measure`, and notes "Hosts must support this record before they can deploy these releases" — [measure-image-action](https://github.com/tinfoilsh/measure-image-action).

**Portability of the verifier**
- `VerifyV3(documentBytes, expectedNonce, trustedRepo)` takes a caller-supplied repo reference and works offline with embedded trust roots — [verify README](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md). The CLI accepts `--enclave enclave.example.com --repo owner/workload@v1.2.3`, but the docs do not describe non-Tinfoil hosts — [CLI docs](https://docs.tinfoil.sh/sdk/cli-sdk.md).
- Defaults are Tinfoil-specific: config and freshness verification use "Tinfoil's public config and artifact signing keys" — [verify/verifier.go](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/verifier.go). A doc comment describes `WithFreshnessSigningKeys` as pinning "Tinfoil's platform and runtime approval keys" — [verify/config.go](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/config.go). Freshness witnesses default to a 7-day maximum age — [verify README](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md).
- In production builds "there is no way to replace a trust anchor", which covers the CPU evidence layer's vendor roots — [verify/overrides.go](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/overrides.go).
- Hooks exist in the constructor logic: embedded config, a config authorizer, custom config or freshness signing keys, pinned registers — [verify/verifier.go](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/verifier.go). A PR adds "trusted local verification inputs" — [tinfoil-go PR #222](https://github.com/tinfoilsh/tinfoil-go/pull/222).
- **Licence trap:** the `tinfoil-go` repo is shown as Apache-2.0 — [tinfoil-go](https://github.com/tinfoilsh/tinfoil-go), but `verify/LICENSE` is GNU AGPL v3 — [verify/LICENSE](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/LICENSE).

**Terms**
- "You may not share accounts or API keys." "You are responsible for all activity under your account and API keys." — [Terms §3](https://tinfoil.sh/terms) (updated 2026-10-12).
- Licence is "non-transferable"; "You may not copy, modify, distribute, sell, lease, reverse engineer, or create derivative works from the Services" except as allowed by law or by separate written agreement. "End user", "your users" and "integrate" do not appear — [Terms §10](https://tinfoil.sh/terms).
- You may not use the Services "in, for the benefit of, or for export or re-export to an embargoed" country or person — [Terms §8](https://tinfoil.sh/terms). The AUP bars bypassing "authentication, rate limits, quotas, or billing" and does not mention resale, proxying or anonymity — [AUP](https://tinfoil.sh/terms/aup).

**Licences**
- `encrypted-http-body-protocol` MIT; `tinfoil-js`, `tinfoil-rs`, `tinfoil-cli` Apache-2.0 — [tinfoilsh org](https://github.com/tinfoilsh). `modelwrap` MIT — [modelwrap](https://github.com/tinfoilsh/modelwrap). `confidential-model-router` AGPL-3.0 — [router](https://github.com/tinfoilsh/confidential-model-router). `measure-image-action` AGPL-3.0 — [measure-image-action](https://github.com/tinfoilsh/measure-image-action). `tinfoil-config` AGPL-3.0 — [tinfoil-config](https://github.com/tinfoilsh/tinfoil-config). `hardware-measurements` shows no licence — [hardware-measurements](https://github.com/tinfoilsh/hardware-measurements).

### Inferences
- **The Tinfoil backend and a self-hosted Verda backend are two different backends, not one image on two hosts.** The boot chain expects direct kernel and initrd launch with a measured command line, plus a private host daemon. On a rented CVM where the provider launches the VM, a tenant likely cannot reproduce that launch. This depends on Verda; the TEE and Verda tracks should settle it.
- `cvmimage` is licensed and buildable anywhere, so the lock is practical, not legal. "Needs Tinfoil host access" describes their test process and does not prove impossibility.
- The verifier is provider-neutral in shape and Tinfoil-centred in defaults. Using it for a non-Tinfoil host would require the embedded-config or custom-key paths, which are not documented for that purpose and sit on a beta path. A fork or a separate verifier for own-CVM attestation is the safer assumption. Embedding `verify/` in a client may carry AGPL obligations; get this checked. Not legal advice.
- A gateway that verifies attestation itself and pins the attested TLS key upstream gets transit integrity for the trailer, because TLS terminates in the enclave. That gives the gateway confidence but gives no third party anything checkable. Tinfoil's `TransportMode` split in [tinfoil-go](https://github.com/tinfoilsh/tinfoil-go) suggests the building blocks exist.
- The client can request `include_usage` inside the sealed body and read an AEAD-authenticated count, which it can compare with what the gateway charged. This would let the client detect gateway overcharging without any new receipt format. I inferred this from `X-Tinfoil-Client-Requested-Usage` in [main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go), and the filtering code was not read. See Proposed validation.
- `continuous_usage_stats` means the router likely holds a running count mid-stream, which is relevant to settling a disconnected stream. Not confirmed.
- With a pooled key, Tinfoil still gets per-request timestamp, token counts, model, enclave and request IDs under your key. Anonymity from Tinfoil then reduces to crowd size behind that key, and timing and count correlation with the gateway remains possible.
- The usage header is plaintext to anything in the proxy path. If a gateway sits in the path, exact token counts are visible to it by design. That is the same information ACT reveals at spend, so it adds nothing new for the gateway, but it is a length side channel on content.
- A stable `user_cache_secret` is a per-user partition inside the enclave. In an anonymous design, fresh secrets per conversation avoid both cross-user timing leaks and a stable pseudonym, at the cost of cache discounts.
- **Terms:** Tinfoil's docs encourage metered proxies and it merges PRs supporting them, so metered proxying looks tolerated. Anonymous resale is not addressed, §3 and §10 read against it without a separate agreement, and the operator carries full responsibility for all activity under the key. Anonymous users also make the sanctions representation hard to stand behind. Get written permission. Not legal advice.

### Gaps
- Whether `inference.tinfoil.sh`, `atc.tinfoil.sh`, `github-proxy.tinfoil.sh` or `kds-proxy.tinfoil.sh` answer over Tor exits is undocumented. Docs only note that model enclave hostnames restrict inbound connections — [Backend infrastructure](https://docs.tinfoil.sh/verification/attestation-architecture.md). See Proposed validation.
- Whether any Tinfoil verifier binds to a chip allowlist, the issue in TOB-WAPI-10. Not found.
- `tokencount` filtering and SSE parsing were not read. The `tinfoil-python` verifier and licence were not checked.
- Whether any Tinfoil feature or roadmap item covers signed per-request receipts. None found.
- When the usage-metrics header was introduced: the changelog has no entry for it.

## Meta Private Processing: whitepaper plus NCC and Trail of Bits

### Takeaway
The conversation's reading of Meta holds up, with three corrections: enforcement is at the gateway as well as at issuance, ACS tokens are multi-use so requests sharing a token are not per-request unlinkable, and whitepaper V3 now says some features let Meta see the IP. Both audits make anonymity depend on key consistency, non-collusion and scale, and those are the weak spots for a small deployment.

### Cited Findings
- **Principles:** end-to-end encryption so that "only a single Private Processing endpoint, and no one in between – including Meta, WhatsApp, or any third-party relay – can decrypt the data"; non-targetability: an attacker should not be able to target a user "without attempting to compromise the entire Private Processing system" — [Whitepaper V3 p3](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).
- **Flow:** the client gets an anonymous credential, fetches HPKE keys from a third-party CDN (currently Fastly), opens OHTTP via a third-party relay that hides the IP, and sets up RA-TLS to a TEE chosen by a load balancer with no knowledge of the user. Attestation is cross-checked against a third-party log (currently Cloudflare) — [Whitepaper p4](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).
- **Gateway step:** "Meta's OHTTP gateway decrypts the HPKE request, validates the ACS token, and selects a CVM node" — [Whitepaper p21](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).
- **New V3 nuance:** after node selection, traffic "may continue over the anonymous channel, or travel directly to the pre-selected CVM node ... depending on the feature". Direct requests "may make a user's IP address visible to Meta, in which case non-targetability is preserved through random selection of downstream connections, strict retry limits and minimum selection pool size" — [Whitepaper p21](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).
- **Credentials and quota:** the ACS key config is distributed via Fastly, signed with a Meta key hardcoded on the client, and credentials can be "used a finite number of times" — [Whitepaper p22](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). "Each client is allowed to request only a limited number of anonymous credentials a day, and each anonymous credential may be used for a limited number of requests" — [Trail of Bits report, design overview](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf). NCC agrees: "Since the ACS rate-limits emission of tokens, this inherently implies rate-limitation of Client requests" — [NCC report](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf).
- **Tokens are reusable within limits:** "A client can use a given token ... up to a maximum number of times or up to the token's expiry" and "can make concurrent requests bearing the same token" — [TOB-WAPI-4](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf). ACS is open source, built on VOPRFs, with a server-side redemption counter — [Meta engineering](https://engineering.fb.com/2022/12/12/security/anonymous-credential-service-acs-open-source/).
- **Timing correlation, primary source:** "It is possible that identifying information about the client could be leaked by correlating the time between anonymous credential issuance and back-end TEE requests; the WhatsApp client implements prefetching of anonymous credentials to mitigate this risk" — [Trail of Bits report](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf). So the source is Trail of Bits. The CyberInsider article I checked on the NCC review does not contain this point — [CyberInsider](https://cyberinsider.com/whatsapps-ai-system-passes-security-audit-but-with-asterisks/).
- **TOB-WAPI-4 (informational, unresolved):** a "reasonably consistent delta between an ACS token prefetch and the newly signed token's first use" lets a compromised engineer narrow a target by region, time window and deployment size. Trail of Bits recommends randomising token validity parameters to help "even in a contrived small deployment". Meta intends to reduce first-fetch and first-use correlation, and assesses such attacks as difficult at "complete rollout and a steady-state QPS" — [Trail of Bits report p39-41](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf).
- **Issuer and relay key consistency:** a malicious ACS server serving different keys per client could link redemptions (TOB-WAPI-1, resolved). Client-specific OHTTP keys could de-anonymise (TOB-WAPI-3, resolved). RFC 9458 §7 requires mitigating tracking through client configurations — [Trail of Bits report p33-38](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf).
- **Machine binding (TOB-WAPI-10, high, partially resolved):** compromised VCEKs from any chip can sign attestations, so a client must bind to known chip IDs. Meta publishes Authorized Host Identities to an external log and plans verifiable host attestation — [Trail of Bits report p53-54](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf).
- **Relay-gateway collusion (NCC):** a colluding Fastly relay and Meta gateway "could analyze timing, volume, and frequency patterns of requests to link them to individual users' IP addresses", which could "undermine the 'Non-Targetability' assurance" — [NCC report p25](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf). NCC also says the relay and log providers' "contractual obligations are toward Meta, and not toward the WhatsApp users" — [NCC report p10](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf).
- **Message length (NCC-E021658-TR3, risk accepted):** encrypted sizes reveal partial information, including through the inner TLS ClientHello size — [NCC report p68](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf).
- **Audit totals:** NCC: 21 findings; 16 fixed, 1 not fixed, 4 risk accepted — [NCC report](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf). Trail of Bits: 28 issues, 8 high; 16 resolved, 4 partial, 8 low or informational left deliberately. Lessons for any TEE service: measure everything loaded, bind attestation to the connection, check patch levels against signed certificate data — [Trail of Bits blog](https://blog.trailofbits.com/2026/04/07/what-we-learned-about-tee-security-from-auditing-whatsapps-private-inference/).
- **Transparency and state:** Cloudflare log with footnote "not a Merkle-tree-based transparency ledger but a simple log"; models and prompts private; coordinated disclosure "typically within a 90 day timeframe" — [Whitepaper p16](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). Orchestrator has "no database access", history is resent on each request, persistent KV cache is off, CVMs are regularly rebooted — [Whitepaper p15](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).
- **Hardware:** AMD SEV-SNP and "H100 Tensor Core GPU"; NVLink unencrypted on Hopper, with Blackwell noted as supporting NVLink encryption — [Whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). A text search of V3 finds no token counts or billing. Metrics are service counters, OS counters and GPU health — [Whitepaper p29-30](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).

### Inferences
- Meta's design does not settle whether per-token anonymous metering is possible. It is not a negative result; the feature is simply not metered.
- **Key-consistency attacks apply directly to this project.** Issuer keys for anonymous credits, and any enclave or relay key the client learns, must be identical for all clients and checkable, or the operator can tag a user by key. Binding issuer keys into the measured enclave config or a public log is the natural fix.
- Meta's mitigation for timing correlation relies on prefetching and scale. A small PoC gets neither by default: with few users, purchase time and first spend, or spend amount, can stand out regardless of the cryptography.
- Tor replaces the Fastly relay's role with a non-contractual, multi-operator path, so the "contractual obligations toward Meta" problem goes away. Exit-to-endpoint and guard-to-client correlation remain. The Tor track should quantify this.
- Trail of Bits' VCEK finding transfers directly: on a rented CVM, an SNP report proves "a genuine AMD chip", not "Verda's machine". A leaked VCEK would pass unless the client pins chip IDs or a provider identity.

### Gaps
- The exact numbers of credentials per day and uses per token are not public.
- Whether Meta has since implemented the token-randomisation recommendation is unknown; V3 does not say.

## Apple Private Cloud Compute: what carries over to a small fleet

### Takeaway
Credential separation, client encryption to attested keys, stateless nodes and a transparency log all carry over conceptually. Target diffusion and most of the non-targetability claim do not: they need a large fleet, Apple's hardware supply-chain controls and an OS-integrated client.

### Cited Findings
- Request metadata omits identifying details, requests carry "a single-use credential, based on RSA Blind Signatures", a third-party OHTTP relay hides the IP, and an attacker would need to compromise both relay and load balancer to steer traffic. The load balancer "returns a subset of PCC nodes", and the device "encrypts this request directly to the public keys of the PCC nodes" — [Apple PCC blog, 2024-06-10](https://security.apple.com/blog/private-cloud-compute/).
- Goal: "targeting users should require a wide attack that's likely to be detected" — [Apple PCC blog](https://security.apple.com/blog/private-cloud-compute/).
- Two token tiers: an Identity Service issues a per-user, per-device Token Granting Token (TGT), then a Token Granting Service issues rate-limited batches of One-Time Tokens (OTT). Both use blind signatures; fraud detection and rate limits live at OTT issuance — [Dittmar et al., WiSec '26](https://arxiv.org/pdf/2605.24239).
- **Empirical gap:** researchers reused a used OTT "between 2 and 5 times" and still got responses, and swapped TGT and OTTs from another Mac with services still working, which the authors say contradicts Apple's documentation — [Dittmar et al.](https://arxiv.org/pdf/2605.24239). They note the gateway must record used tokens until expiry — [Dittmar et al.](https://arxiv.org/pdf/2605.24239). Findings were responsibly disclosed to Apple — [Dittmar et al.](https://arxiv.org/pdf/2605.24239).
- Measurements are published in an append-only, tamper-evident log, and a device encrypts only to nodes whose attested measurements match — [Apple PCC blog](https://security.apple.com/blog/private-cloud-compute/), [PCC research post](https://security.apple.com/blog/pcc-security-research/).

### Inferences
- **With one node, or a handful, target diffusion gives no protection.** Every request reaches the same small set, so the honest PoC statement is "an attacker who compromises the node gets all traffic in that window". This matches Meta's "minimum selection pool size" and Trail of Bits' point about shrinking deployments. Non-targetability should be listed as out of scope for the PoC.
- The two-tier token design is a useful pattern: a slow, account-linked purchase step, then short-lived, spend-only tokens. It parallels OpenPCC's AuthBank and BlindBank.
- The reuse result shows that single-use enforcement is a real implementation failure mode, not a detail, including at a very large vendor. Double-spend state and its race conditions need explicit tests.
- Apple's advantages that do not transfer: custom silicon, audited supply chain, OS-level client and scale. The analogues here are AMD or Intel plus NVIDIA attestation with weaker physical guarantees.

### Gaps
- The Apple Security Guide pages (`security.apple.com/documentation/...`) did not render through the fetch tool. Details on target diffusion come from the 2024 blog and may have evolved.
- The extent of any OTT server-side fix after disclosure is unknown.

## OpenPCC by Confident Security

### Takeaway
OpenPCC is the most complete public specification of anonymous credits with spend-and-refund on attested inference, and its `anonpay` code is Apache-2.0, so it is the strongest reference and a credible partial base. It is not a drop-in for this PoC: the compute image assumes a TPM and GCP or Azure metadata under a more restrictive licence, the only bank services I could find in the repo are in-memory, there is no Tor integration, and repo activity appears to have slowed since January 2026.

### Cited Findings
- **Status:** Apache-2.0, 957 stars, 45 commits, "modeled on Apple's Private Cloud Compute", deployable on your own infrastructure — [openpcc repo](https://github.com/openpcc/openpcc). Latest commits shown: 2026-01-08 — [commits](https://github.com/openpcc/openpcc/commits/main). Whitepaper dated November 2025 — [whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Claimed implementations:** the whitepaper says the project provides Apache-2.0 implementations of "the Client, AuthBank, Relay, Gateway, BlindBank, and Router components", and that "a production-ready ComputeNode implementation exists under a source-available license from Confident Security". It also says an unrelated third party must operate the Relay in production, and names Fastly, Oblivious Network and Cloudflare as commercial options — [whitepaper §3](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Components:** AuthBank (identity and funds) is "the single component" tied to a user's identity; BlindBank (pseudonymous accounts over OHTTP); Router; ComputeNode; third-party OHTTP relay and gateway — [whitepaper §2.2-2.3](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf). The whitepaper says fund management is out of scope — [whitepaper §4.1.3](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Credit:** uses RSA Blind Signatures with Public Metadata (RSAPBSSA). Public metadata encodes the value; the private part has a nonce and an hour-quantised timestamp; a central record of used nonces tracks redemption; limited lifespan bounds that store — [whitepaper §2.4.1](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Value set:** 0 to 33,285,996,544 with at most 6.25% relative error — [whitepaper §2.4.1](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf). Code: 9 bits, 512 amounts, stochastic rounding "fair and unbiased over the long run", exact up to 32 — [value.go](https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/currency/value.go).
- **Spend and refund:** the user picks a maximum and supplies a Credit of that value; the system caps at it and returns a Credit with the unused balance, quantised to reduce leakage. The user "unblinds the returned Credit and exchanges it at the BlindBank for a blind Credit", and should delay because "the refund amount exposes the client to a timing attack" — [whitepaper §2.4.2](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Cost source:** the ComputeNode returns the encrypted result "along with the compute cost" to the Router, which issues the refund. "The ComputeNode may include the amount of expected refund Credits to verify that the Router issued the correct refund amount" — [whitepaper §2.2.7, §4.2](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Declared side channels:** AuthBank plus BlindBank (timing, funds) and BlindBank plus Router (timing, Credits). Mitigated by quantisation and wallet delays, and "None of these weaknesses should be possible with a well-behaved client wallet ... and sufficient OpenPCC network traffic" — [whitepaper §2.4.3](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf).
- **Wallet:** retrieve credit for N default-sized requests, wait a random time, deposit to a new blind account, withdraw N, wait again; rotate pseudonym accounts — [whitepaper §2.4.3](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf). There is no mention of Tor in the whitepaper (text search).
- **Code layout:** `anonpay` has `banking`, `currency`, `noncelocking`, `wallet`, `blind_signing.go`, `exchanger.go`, `issuer.go`, `payee.go`, `processor.go`, `unblinded_credit.go` — [anonpay](https://github.com/openpcc/openpcc/tree/main/anonpay). `banking` has `httpapi`, `inmem`, `testcontract` and a contract file — [banking](https://github.com/openpcc/openpcc/tree/main/anonpay/banking). `cmd` has `mem-auth`, `mem-bank`, `mem-compute`, `mem-credithole`, `mem-gateway`, `mem-router`, `ohttp-relay`, `test-client` — [cmd](https://github.com/openpcc/openpcc/tree/main/cmd). The README describes "in-memory services" for exercising the client — [README](https://raw.githubusercontent.com/openpcc/openpcc/main/README.md).
- **Hardware assumption:** TEE (TDX or SEV-SNP), plus "an associated ephemeral virtual TPM" holding the Request Encryption Key and Attestation Key; NVIDIA GPU; Ubuntu 22.04 base — [whitepaper §2.5](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf). The compute image targets GCP and Azure metadata — [compute-images README](https://raw.githubusercontent.com/confidentsecurity/confidentcompute/main/compute-images/README.md).
- **Compute node licence:** Functional Source License 1.1 with ALv2 future: no "Competing Use", converts to Apache-2.0 two years after each version's release — [confidentcompute LICENSE](https://github.com/confidentsecurity/confidentcompute/blob/main/LICENSE). 20 stars, 30 commits — [confidentcompute](https://github.com/confidentsecurity/confidentcompute).
- **Relay:** the README recommends Oblivious.network — [README](https://raw.githubusercontent.com/openpcc/openpcc/main/README.md).

### Inferences
- **Refund design matters:** OpenPCC's refund is initially issued in a form the issuer can link and must be exchanged to re-anonymise, with a delay. ACT's change credential is designed differently, and the credentials track should compare them. Either way, change handling is where linkage returns.
- **Exact per-token budget and good anonymity pull against each other.** OpenPCC deliberately quantises to 512 amounts with stochastic rounding, so an individual reconciliation is approximate while long-run totals are fair. A PoC showing an exact remaining budget needs either exact change, which is a higher-entropy signal, or a UI that shows an estimate.
- OpenPCC's value is `anonpay` plus the protocol. Replacing the TPM-rooted compute path with EHBP or RA-TLS to a non-TPM CVM is a substantial change. Using OpenPCC on Verda depends on whether Verda exposes a vTPM rooted in the TEE; TEE/Verda tracks should check.
- For a card purchase, AuthBank is the seam: card payment funds a balance, then credits are withdrawn. It is out of scope in the spec, so it is greenfield work. Commercial use of the compute image could hit the FSL "Competing Use" clause. Not legal advice.
- The compute-node-reported expected refund is close prior art for a client-checkable settlement, without a third-party-verifiable receipt.

### Gaps
- The whitepaper claims AuthBank and BlindBank implementations exist in the repo, but the `cmd` and `banking` listings I read show in-memory services and an HTTP API layer. I found no durable-storage bank, and I did not read every package, so this is unresolved rather than refuted. Router and gateway production readiness was not determined.
- No independent security audit of OpenPCC or `anonpay` was found.
- `anonpay` was only partly read: `credit.go`, `payee.go` and `value.go` only. Exact wire format, and the method and location of refund issuance in code, were not confirmed.
- Whether commit activity continues elsewhere (for example under `confidentsecurity`) is unknown.

## LLM-Tor and other systems

### Takeaway
LLM-Tor confirms the conversation's account: it deliberately keeps a stable pseudonym and its proxy handles plaintext, so it is a useful issuance and Tor reference but not a base. Others show pieces (Routstr for pay-per-token, Brave for OHTTP with unlinkable tokens, Kagi for Privacy Pass plus onion), and none found has the whole combination.

### Cited Findings

**LLM-Tor** (single-author whitepaper, March 2026; not peer reviewed as far as found)
- Users buy credits with a standard payment provider against their identity; tokens are blind RSA signed with a model-specific key; spent tokens are stored as hash(T) — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex).
- Abuse tokens: permanent (per account) and transient (monthly), both sent on every anonymous request: "all requests bearing the same $A_p$ are linkable to each other, but not to the paying identity" and "The scheme intentionally trades full per-request unlinkability for pseudonymity within a session-less context" — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex).
- It "performs content moderation prior to forwarding requests upstream". Moderation of the prompt at the proxy, then forwarding upstream, means the proxy handles plaintext — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex), [README](https://github.com/prince776/LLM-Tor).
- Upstream logging, global adversaries, endpoint compromise and stylometry are out of scope: "LLM-Tor does not provide absolute anonymity" — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex).
- Runs over standard HTTPS reached via a Tor exit, with onion-only planned. AGPL-3.0, 18 stars — [README](https://github.com/prince776/LLM-Tor).
- The document does not specify tiers, max tokens per request or credit-to-token pricing, so per-token metering is not part of it — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex).
- Internal tension: another section claims issued tokens cannot be linked to redeemed tokens, while abuse tokens make requests linkable to each other. It does not address timing correlation between issuance and redemption — [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex).

**Routstr + Cashu** (detail in next section)
- FastAPI reverse proxy in front of OpenAI-compatible APIs, using Cashu tokens and Nostr discovery; GPL-3.0 — [routstr-core](https://github.com/Routstr/routstr-core).

**Brave Leo** (2026-10-15)
- Encrypts in the browser with OHTTP and HPKE to a gateway inside NEAR AI's TEE. Brave's AI Gateway is the relay and sees the IP; the TEE does not. Premium is validated with "unlinkable tokens, so payment details can't be tied to conversations". Verification runs on Brave's servers today, with browser-side checks planned. Metering is not described — [Brave blog](https://brave.com/blog/browser-ai-e2ee/).
- `brave/ai-gateway` is MPL-2.0, routes to `local, vLLM, LiteLLM, Bedrock, Anthropic` via a `MODELS` JSON env var — [ai-gateway](https://github.com/brave/ai-gateway).

**Kagi Privacy Pass**
- Privacy Pass issuance is RFC 9578, with deployment choices in RFC 9576 — [RFC 9578](https://www.rfc-editor.org/rfc/rfc9578.html). Press coverage says Kagi deploys it for paid plans plus an onion address; Kagi's own technical docs were not read — [secondary report](https://rss.boorghani.com/?p=282034).

**Confer** (2026-01-05)
- CVM with kernel, initrd and command line measured, dm-verity root hash on the command line, Noise Pipes from device into TEE with ephemeral keys, reproducible nix and mkosi builds, append-only release log. Payment, metering and anonymity are not covered — [Confer blog](https://confer.to/blog/2026/01/private-inference).

**Phala / dstack, NEAR AI**
- dstack: Apache-2.0, Docker in CVMs. "Intel TDX is the production path; AMD SEV-SNP is new and experimental"; H100 and Blackwell; VMM "Runs on bare-metal TDX hosts"; also GCP Confidential VMs; audited by zkSecurity — [dstack](https://github.com/Dstack-TEE/dstack).
- NEAR AI: TDX plus NVIDIA TEE GPUs, NRAS for GPU attestation, TLS terminated in the TEE — [NEAR docs](https://docs.near.ai/cloud/private-inference). Intel Trust Authority tokens were added August 2026 — [NEAR blog](https://near.ai/blog/near-ai-cloud-integrates-intel-trust-authority).

**Maple**
- Docs say inference runs in TEEs with end-to-end encrypted prompts; built on OpenSecret — [Maple docs preview](https://45afd2b8.opensecret-sdk.pages.dev/docs/maple-ai). A `MaplePrivacyLabs/tinfoil-rs` fork exists — [fork](https://github.com/MaplePrivacyLabs/tinfoil-rs). Thin evidence.

### Inferences
- Reuse from LLM-Tor is limited to the card-to-blind-token flow and desktop Tor client approach. Its abuse and moderation choices are exactly what this project must drop.
- **Dropping abuse tokens is a real cost:** there is no blacklist path for an anonymous abuser. With a pooled Tinfoil key, abuse risk falls on the key holder under Terms §3. This needs a decision, not just an implementation.

### Gaps
- Brave's token construction was not read. Search did not confirm blind signatures specifically.
- Payment anonymity for Maple, NEAR AI, Phala and RedPill was not found in searches.

## Closest prior art for anonymous per-token metering, Tor and attested GPU inference

### Takeaway
No single system found has all of it, but two public projects together cover nearly every piece, with Tinfoil actively merging support. Routstr has blind ecash, ceiling reservation, change and EHBP to Tinfoil's attested GPUs. Possums has an attested gateway on Tinfoil and an onion route with no clearnet fallback. The remaining gap is card-funded anonymous credits, a self-hosted or modular backend, and audited privacy.

### Cited Findings

**Routstr (closest on metering)**
- For encrypted requests the node detects `Ehbp-Encapsulated-Key`, skips JSON parsing, reads the model from `X-Routstr-Model`, and forwards raw bytes. The reservation or redemption is capped at `max_cost_for_model`. Bearer mode finalises at actual cost from `X-Tinfoil-Usage-Metrics`. `X-Cashu` mode redeems the token and refunds the difference, or everything if upstream fails or usage is unavailable — [EHBP doc](https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md).
- The SDK attests before the main token is spent, by default against Tinfoil's public ATC, and calls the proxy "a blind relay"; multi-user apps must set a per-user `userCacheSecret` — [SDK TinfoilImpl.md](https://raw.githubusercontent.com/Routstr/routstr-sdk/main/TinfoilImpl.md).
- Live billing validated on 2026-09-17 by the PR author: "E2EE requests are billed at the Tinfoil rate" — [PR #736](https://github.com/Routstr/routstr-core/pull/736).
- Cap-based lower reservations are scaffolded but disabled: the author measured `n=2` producing 16 completion tokens against a cap of 8 — [PR #882](https://github.com/Routstr/routstr-core/pull/882). PR #532 upstream addresses that — [router PR #532](https://github.com/tinfoilsh/confidential-model-router/pull/532).
- Tor: compose docs mention a "public/Tor stack" and the SDK has a `torMode` option; neither page describes behaviour — [routstr-core](https://github.com/Routstr/routstr-core), [routstr-sdk](https://github.com/Routstr/routstr-sdk).
- Payment is Cashu over Bitcoin Lightning; USD and card are not mentioned — [routstr-core README](https://raw.githubusercontent.com/Routstr/routstr-core/main/README.md), [Cashu blog](https://blog.cashu.space/routstr/).

**Possums (closest on Tor and attested gateway)**
- "cryptographically verifiable no log inference provider" proxying to Tinfoil, with the gateway hosted in Tinfoil's TEE. The client verifies the gateway before any prompt. Billing uses a reservation and an end-of-stream receipt: "Without one, the charge is unknown — not zero, and not refunded" — [possums](https://github.com/ajbt200128/possums).
- Payment is explicitly not anonymous: the intake store outside the attested boundary "is not anonymous and not hardware-verifiable" and sees payment-to-account association. No licence yet, 0 stars — [possums README](https://raw.githubusercontent.com/ajbt200128/possums/main/README.md).
- Tor: user-run SOCKS, onion address pinned, "a fresh 32-byte nonce is signed together with the onion identity", TLS over the socket with the endorsed SPKI proven before HTTP, bodies stay EHBP-encrypted, and the client "never falls back to clearnet". Also states no per-connection circuit isolation and no anonymity guarantee — [tor-guide](https://raw.githubusercontent.com/ajbt200128/possums/main/docs/tor-guide.md).

**PayPerQ**
- Measured Nitro enclave as API entry point, relays `private/*` ciphertext to Tinfoil's router, settles from the usage header or trailer. Closed 2026-09-23; anonymity not discussed — [issue #210](https://github.com/PayPerQ/ppq-enclave-proxy/issues/210).

**Gap versus the user's goal**

| Requirement | Routstr | Possums | OpenPCC | LLM-Tor | Brave Leo |
|---|---|---|---|---|---|
| Purchase with card, USD | No (Bitcoin) | Rails not named | Out of scope | Yes (standard provider) | Subscription |
| Blind or unlinkable spend | Yes (Cashu) | No (account) | Yes (RSAPBSSA) | Yes (but pseudonym) | Yes (tokens, construction unread) |
| Per-token settlement with change | Yes | Yes, not anonymous | Yes, quantised | Not specified | Not described |
| Client-to-enclave encryption | Yes (EHBP) | Yes (EHBP) | Yes (TPM REK + HPKE) | No (plaintext proxy) | Yes (OHTTP to TEE) |
| Attested GPU | Via Tinfoil | Via Tinfoil | Yes (own image) | No | Via NEAR AI |
| Tor | Option, behaviour unread | Yes, onion | No (OHTTP) | Yes, exit | Not described |
| Self-hosted backend | Any provider, not attested | Not standalone yet | GCP or Azure images | Proxy only | No |

### Inferences
- The proposal is novel only in composition and in specifics such as USD and ACT. It should be framed as "unify and harden known components", and Routstr's and Possums' issue trackers are a cheap source of real failure modes: duplicate trailers, failover re-reservation, `n` overrun, missing usage.
- **Change linkage in Routstr-like designs:** a node that issues a refund token knows which request it belongs to. Respending it at the same node without an intervening swap at the mint would link requests. OpenPCC handles this explicitly by exchange and delay. I did not read Routstr's wallet code, so I do not know if its SDK swaps.
- Routstr refunds in full when usage is missing. This is a free-riding path if a client can induce loss of the trailer, for example by disconnecting. Possums picks the opposite rule, with the unknown charge not refunded. Neither is obviously right; the PoC must choose and state it.
- Possums shows onion-bound attestation is implementable at this level without any Tinfoil change.

### Gaps
- Whether any of these has been independently audited. None found.
- Whether Routstr's `torMode` routes via onion or exit, and whether Tinfoil's ATC is fetched through Tor.

## Modularity: what differs across backends

### Takeaway
Three things vary by backend and need separate seams: how trust is established (evidence plus code identity), how bytes are sealed, and where the usage number comes from. OpenAI-compatible JSON is the shared application layer in all the systems reviewed.

### Cited Findings

| Backend | Evidence and verifier | Code identity | Transport | Usage source |
|---|---|---|---|---|
| Tinfoil | SEV-SNP or TDX, vendor chain, `nvattest` in guest; client checks router, router checks models — [docs](https://docs.tinfoil.sh/verification/attestation-architecture.md) | Sigstore and Rekor, GitHub repo, freshness witnesses — [docs](https://docs.tinfoil.sh/verification/attestation-architecture.md), [PR #217](https://github.com/tinfoilsh/tinfoil-go/pull/217) | EHBP or attested TLS — [tinfoil-go](https://github.com/tinfoilsh/tinfoil-go) | Router trailer from vLLM usage — [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go) |
| OpenPCC | TEE plus vTPM AK and REK — [whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf) | Transparency log and identity policy (OIDC regex) — [repo](https://github.com/openpcc/openpcc) | OHTTP plus HPKE to TPM-held REK — [whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf) | ComputeNode cost returned to Router — [whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf) |
| Meta | SEV-SNP, H100; RA-TLS — [whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) | Cloudflare log, host identities — [whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) | RA-TLS over chunked OHTTP — [whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) | None per user |
| Brave and NEAR AI | TDX plus NVIDIA; Brave oracle rebuilds images — [Brave](https://brave.com/blog/browser-ai-e2ee/) | Sigstore, GitHub, deployment log — [Brave](https://brave.com/blog/browser-ai-e2ee/) | OHTTP with HPKE — [Brave](https://brave.com/blog/browser-ai-e2ee/) | Not described |
| Confer | Quote over kernel, initrd, cmdline — [Confer](https://confer.to/blog/2026/01/private-inference) | Append-only release log — [Confer](https://confer.to/blog/2026/01/private-inference) | Noise Pipes — [Confer](https://confer.to/blog/2026/01/private-inference) | Not described |

Abstractions the prior art exposes:
- Tinfoil: `verify.Verifier` with caller-supplied nonce and repo, and `EmbeddedReferences`; `TransportMode` for TLS or EHBP — [verify README](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md), [tinfoil-go](https://github.com/tinfoilsh/tinfoil-go).
- OpenPCC: `anonpay` `issuer`, `payee`, `processor`, `exchanger`, `wallet` and a `blind_bank_contract` — [anonpay](https://github.com/openpcc/openpcc/tree/main/anonpay); `attestation/{attest,evidence,verify}` — [attestation](https://github.com/openpcc/openpcc/tree/main/attestation); a pluggable node finder and transparency verifier in the client — [repo](https://github.com/openpcc/openpcc).
- Routstr: reserve and finalise against a model registry price, keyed by a client-declared header — [EHBP doc](https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md).
- Brave: backends declared in JSON config — [ai-gateway](https://github.com/brave/ai-gateway).

### Inferences
- A backend at concept level is: verify evidence against a policy and yield keys; seal and open bytes under those keys; report a final usage count with stated trust; enforce a ceiling. Payment should know only about usage and ceiling, never about backend specifics.
- The usage source deserves a declared trust level per backend, because they differ: an own enclave counts inside the same attested boundary as the gateway; Tinfoil's count arrives from an attested router over TLS as an unsigned string; a non-attested gateway reporting counts is an honest-operator assumption.
- Tinfoil's router already has a model-to-repo map compiled in. A model selector across backends could follow the same pattern: map model IDs to expected code identity in measured or signed config, so the choice cannot silently be redirected.

### Gaps
- Whether a single client verifier abstraction can cover V3 Tinfoil documents and a generic Verda CVM without Tinfoil-specific endorsements depends on the TEE track's findings.

## Proposed validation, claim table, verdicts and reuse matrix

### Takeaway
Nothing below has been run. Several claims can only be settled by running them: Tor reachability above all, then trailer behaviour, booting the image off Tinfoil's fleet, and OpenPCC's in-memory flow.

### Proposed validation

**V1. Verifier against the public attestation endpoint (clearnet baseline).**
- Run: `tinfoil attestation verify -e inference.tinfoil.sh -r tinfoilsh/confidential-model-router -j -l verification.json` with an installed `tinfoil-cli` release ([CLI docs](https://docs.tinfoil.sh/sdk/cli-sdk.md)). Separately, run a small Go program calling `verify.NewVerifier()` and `VerifyV3` on a saved document.
- Confirms: status success, plus an audit record with code and enclave measurements, `crypto_material`, and `freshness_expires_at`.
- Refutes: errors on this path, since V3 is beta, or the served format not being V3 (the live ATC read returned `sev-snp-guest/v2`, so V2 and V3 paths may coexist).
- Time: about 30 minutes.

**V2. Same over Tor.**
- Run: a local Tor client, then `curl --socks5-hostname 127.0.0.1:9050` against `https://atc.tinfoil.sh/attestation`, `https://inference.tinfoil.sh/.well-known/tinfoil-attestation?nonce=<hex>`, `https://inference.tinfoil.sh/.well-known/hpke-keys`, and `https://inference.tinfoil.sh/.well-known/tinfoil-proxy`, then V1's command via `torsocks`. Repeat with `NEWNYM` across at least 20 circuits. Also test the other dependencies: `github-proxy.tinfoil.sh`, `kds-proxy.tinfoil.sh`, GitHub, and Sigstore TUF and Rekor. Log status, body type and latency.
- Confirms: at least 95% of circuits return 200 with the correct JSON, and the full verify succeeds with no clearnet traffic (check with a packet capture or a firewall that blocks non-Tor egress).
- Refutes: 403s, challenge pages or a subset of exits blocked; any step needing clearnet.
- Also: an unauthenticated `POST /v1/chat/completions` should return an auth error rather than an edge block, distinguishing "exit blocked" from "unauthorized". Using a real key should be the user's explicit decision, because the key is account-linked.
- Time: 1 to 2 hours.

**V3. Metering through a local relay (user's own key, user's decision).**
- Run: a local proxy forwarding EHBP ciphertext with `X-Tinfoil-Request-Usage-Metrics: true`. Test non-streaming, streaming, client disconnect at 10%, 50% and 90%, `X-Tinfoil-Max-Output-Tokens`, `X-Tinfoil-Completion-Count`, and `n` in the body. Set `include_usage` inside the sealed body and compare the in-body count with the trailer.
- Confirms: trailer is present on normal completion and in-body equals trailer; caps enforced; disconnect behaviour known.
- Refutes: trailer lost on disconnect, or in-body and trailer disagree, or a cap can be exceeded.
- Time: half a day.

**V4. Boot `cvmimage` outside Tinfoil's fleet.**
- Run on a Verda CC instance: determine whether the tenant can choose kernel, initrd, command line and firmware, and whether a report reflects expected values from `sev-snp-measure` or `tdx-measure`.
- Confirms: reported launch measurement equals the computed one, and the GPU attests inside the guest.
- Refutes: the tenant cannot control launch, or the measurement covers only provider firmware.
- Time: half a day plus instance cost. Coordinate with the TEE/Verda track.

**V5. OpenPCC spend and refund, in memory.**
- Run: `mage runMemServices`, then `mage runClient` per [README](https://raw.githubusercontent.com/openpcc/openpcc/main/README.md).
- Confirms: a credit is spent, a refund is returned and exchanged, and the wallet delays are observed.
- Refutes: build no longer works, or the refund is missing.
- Time: 1 to 2 hours.

**V6. Written confirmation from Tinfoil.** Not code. Ask whether anonymous end users behind a pooled key are permitted, how sanctions and abuse duties apply, and whether signed per-request usage receipts are planned or welcome. Time: one email plus waiting.

### Claim table (claims beyond the conversation's own statements)
Unless a row says otherwise, status comes from documentation or source reading only and is **untested in practice**.

| Claim | Status | Evidence URLs |
|---|---|---|
| A proxy can learn token counts without decrypting the body | solid (untested) | https://docs.tinfoil.sh/guides/proxy-server.md ; https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go |
| Third parties can run this in production: EHBP reservation settled from the usage trailer | conditional (reported by implementers, not independently tested) | https://github.com/Routstr/routstr-core/pull/736 ; https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md |
| Usage values returned to a proxy are cryptographically checkable by a third party | false as far as found (untested) | https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go ; https://docs.tinfoil.sh/guides/proxy-server.md |
| Client can independently check the count through the sealed body | conditional (inferred, untested) | https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go |
| Output can be capped by a header the proxy sets | conditional: per call only, not tool loops or retries (untested) | https://github.com/tinfoilsh/confidential-model-router/pull/518 ; https://github.com/tinfoilsh/confidential-model-router/pull/532 |
| Tinfoil's service is reachable from Tor exits | unknown | https://tinfoil.sh/security-and-privacy-faq ; https://docs.tinfoil.sh/verification/attestation-architecture.md |
| Tinfoil's Terms permit anonymous reselling through a pooled key | weak: not addressed, §3 and §10 cut against it | https://tinfoil.sh/terms ; https://tinfoil.sh/terms/aup ; https://docs.tinfoil.sh/admin/admin-api.md |
| `cvmimage` can run on a rented non-Tinfoil CVM | weak or unknown (untested) | https://raw.githubusercontent.com/tinfoilsh/cvmimage/main/AGENTS.md ; https://github.com/tinfoilsh/hardware-measurements ; https://github.com/tinfoilsh/measure-image-action/pull/85 |
| Tinfoil's production verifier is provider-agnostic | conditional: repo is caller-supplied, keys and roots default to Tinfoil, roots not replaceable (untested) | https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/verifier.go ; https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/overrides.go |
| Tinfoil's verifier is Apache-licensed | false for `verify/`: its LICENSE file is AGPL v3 | https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/LICENSE ; https://github.com/tinfoilsh/tinfoil-go |
| Tinfoil's attestation endpoint answers publicly without credentials | solid (observed once over clearnet, 2026-11-06) | https://atc.tinfoil.sh/attestation |
| Blind ecash, reservation, change and attested GPU are already combined | conditional: Routstr with Tinfoil, Bitcoin only (untested) | https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md |
| An onion-bound, attested gateway proxying to attested GPU inference exists | conditional: Possums, early, not anonymous payment (untested) | https://raw.githubusercontent.com/ajbt200128/possums/main/docs/tor-guide.md ; https://raw.githubusercontent.com/ajbt200128/possums/main/README.md |
| Credit spend with ceiling and refund on attested inference is specified | solid as a spec (untested) | https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf |
| OpenPCC can be a drop-in PoC base on Verda | weak: TPM, GCP/Azure image, FSL compute node, in-memory bank (untested) | https://raw.githubusercontent.com/confidentsecurity/confidentcompute/main/compute-images/README.md ; https://github.com/confidentsecurity/confidentcompute/blob/main/LICENSE ; https://github.com/openpcc/openpcc/tree/main/cmd |
| Single-use anonymous tokens are reliably enforced in deployed systems | weak: researchers reused PCC OTTs 2 to 5 times | https://arxiv.org/pdf/2605.24239 |
| Target diffusion or non-targetability can be claimed on a small fleet | false (reasoned, supported by Meta's and ToB's size assumptions) | https://security.apple.com/blog/private-cloud-compute/ ; https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| Per-client issuer or relay keys can de-anonymise | solid (audit finding) | https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| An SEV-SNP report alone proves a specific operator's machine | false: requires chip-ID binding | https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf |
| Exact per-token reconciliation is free of privacy cost | weak: OpenPCC deliberately quantises refunds | https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf ; https://raw.githubusercontent.com/openpcc/openpcc/main/anonpay/currency/value.go |

### Claim-by-claim verdicts on the conversation
Same table shape. All are documentation or source only, **untested in practice**.

**Meta**

| Statement in the conversation | Status | Evidence and note |
|---|---|---|
| Content is E2E between device and TEE; relay cannot decrypt | solid | [Whitepaper p3](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). Quote accurate. |
| Relay exists for metadata privacy and non-targetability, not content | solid | [Whitepaper p21](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| Provider doesn't see your IP or traffic metadata (table) | conditional | True on the anonymous path; V3 says direct-to-CVM paths may expose IP, depending on feature. [Whitepaper p21](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| ACS authenticates you as a real client without identifying you | conditional | Design true. Earlier implementation allowed linking via ACS keys (TOB-WAPI-1, resolved). [Trail of Bits](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf) |
| Non-targetability quote and intent | solid as a stated goal; conditional as a property | [Whitepaper p3](https://ai.meta.com/static-resource/private-processing-technical-whitepaper); [NCC p25](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf) relies on non-collusion. |
| Limited credentials per day; each valid for limited requests | solid | Source is [Trail of Bits](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf). Numbers unpublished. |
| Timing between issuance and use can leak identity | solid | Source is [Trail of Bits](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf), not the NCC summary. Client prefetches to mitigate. |
| "Meta never rate-limits your requests" | false | Gateway validates the token and uses are finite. [Whitepaper p21-22](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). The conversation's own correction is right. |
| Relay-gateway collusion and traffic correlation can undermine non-targetability | solid | [NCC p25](https://www.nccgroup.com/media/ymskbe40/ncc_group_metaplatforms_whatsapp-message_summarization_report_2025-08-27_v10.pdf) |
| Meta does no per-user token accounting; metrics are aggregate capacity data | conditional | No billing or token counts in V3 text. "Aggregate" is not the paper's word: it describes counters and health metrics. "Free consumer feature" is inference, not stated. [Whitepaper p29-30](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| Per-user metering "might violate" Meta's guarantee (user's framing) | conditional | Paper does not discuss it. Multi-use tokens mean Meta already accepts some request linkability within a token. [TOB-WAPI-4](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf) |
| Third-party Cloudflare log, not Merkle; artifacts to eligible researchers; up to 90 days | solid | [Whitepaper p16](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). The 90 days applies to some cases, "typically". |
| Models and prompts are private artifacts | solid | [Whitepaper p16](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| No database, KV cache off, history resent, regular reboots | solid | [Whitepaper p15](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| SEV-SNP plus H100; Fastly for key configs | solid | [Whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |

**Tinfoil**

| Statement in the conversation | Status | Evidence and note |
|---|---|---|
| CC enclave, client-verified attestation, HPKE body encryption | solid as design; conditional on TEE assumptions | [Docs](https://docs.tinfoil.sh/verification/verification-in-tinfoil.md); [FAQ](https://tinfoil.sh/security-and-privacy-faq) lists demonstrated attacks. |
| EHBP designed so proxies can route on metadata without seeing body | solid | [EHBP repo](https://github.com/tinfoilsh/encrypted-http-body-protocol). Quote accurate. |
| Tinfoil sees source IP, packet timing, bytes, token counts, model, timestamp | solid | [FAQ](https://tinfoil.sh/security-and-privacy-faq). The usage record also includes API key, request ID, route and enclave. [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md) |
| API key, IP and model are visible as plaintext metadata to routing | conditional | Key and IP yes. The model is in the sealed body, so an outer proxy cannot see it, but the router enclave and billing do. [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md) |
| No anonymous relay or credentials in the architecture | solid | [FAQ](https://tinfoil.sh/security-and-privacy-faq) |
| Non-targetability not claimed | solid | [FAQ](https://tinfoil.sh/security-and-privacy-faq) does not address it. |
| "Fully open-source" | weak | Security-critical parts are open; billing and accounts are closed; host daemon appears private. [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md), [PR #85](https://github.com/tinfoilsh/measure-image-action/pull/85) |
| Byte-for-byte reproducible image | conditional | Documented claim with a recipe; not reproduced here. [CVM docs](https://docs.tinfoil.sh/verification/cvm-image.md) |
| dm-verity images of pinned HF revisions | solid | [modelwrap](https://github.com/tinfoilsh/modelwrap), [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md) |
| Checks are against what Tinfoil publishes, not a third-party log | false | Sigstore and Rekor are used; the conversation's correction is right. V3 also adds Tinfoil-signed freshness. [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md), [PR #217](https://github.com/tinfoilsh/tinfoil-go/pull/217) |
| SDKs verify on every connection and refuse on mismatch | solid | [Docs](https://docs.tinfoil.sh/verification/verification-in-tinfoil.md). V3 path is beta. |
| Prompt caching exists, partitioned by identity and per-user secret | solid | [Prompt caching](https://docs.tinfoil.sh/sdk/prompt-caching.md) |
| Usage record contains model, tokens, timestamp, no content; router delivers it; limits are soft | solid | [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md), [Admin API](https://docs.tinfoil.sh/admin/admin-api.md) |
| "The enclave counts tokens" | conditional | Count comes from the model server's `usage`, relayed by the attested router, rather than an independent count. [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go), [main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go) |
| "The gateway cannot simply read the token counts out of the encrypted response" | false | It reads them from the documented header or trailer. Literally true of the body, but misleading. [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md) |
| "That receipt mechanism would be additional protocol work" | conditional | True only if you need third-party-checkable receipts, because the value is unsigned. Not needed for basic settlement. [Routstr doc](https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md) |
| Tinfoil documents a proxy adding its key without decrypting | solid | [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md). The Terms do not align cleanly. [Terms](https://tinfoil.sh/terms) |
| First protected endpoint is the router, which verifies the model enclave | solid | [Docs](https://docs.tinfoil.sh/verification/verification-in-tinfoil.md) |
| Router could accept anonymous pre-authorizations; proposed, not offered | solid that it is not offered | Admission uses API key via control plane. [main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go), [Admin API](https://docs.tinfoil.sh/admin/admin-api.md) |
| Fronting with your own relay or VPN composes with EHBP | conditional | Composes at the protocol level; Tor reachability unknown; an API key on each request stays linkable. |
| A VPN does not supply anonymous credentials (correction) | solid | Follows from per-key accounting. [Docs](https://docs.tinfoil.sh/verification/attestation-architecture.md) |
| Hardware flaws, side channels and client compromise are listed limits | solid | [FAQ](https://tinfoil.sh/security-and-privacy-faq) |

**LLM-Tor**

| Statement in the conversation | Status | Evidence and note |
|---|---|---|
| Retains a stable pseudonym across requests, deliberately | solid | [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex). It is a self-published whitepaper, not a reviewed paper. |
| Long-lived abuse-prevention tokens make requests mutually linkable but not linked to paying identity | solid | [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex). Timing correlation is not addressed in that document. |
| Proxy processes plaintext for moderation | solid by clear implication | [whitepaper.tex](https://raw.githubusercontent.com/prince776/LLM-Tor/main/whitepaper.tex), [README](https://github.com/prince776/LLM-Tor) |
| Already uses blind-signed payment tokens; ACT would replace rather than layer | solid on the first half; second half is reasonable inference | [README](https://github.com/prince776/LLM-Tor) |
| Using it means redesigning for client-to-enclave encryption and dropping the pseudonym | solid as inference | Dropping it loses the blacklist. [README](https://github.com/prince776/LLM-Tor) |

**The conversation's overall proposal**

| Statement | Status | Note |
|---|---|---|
| Payment for compute does not inherently need a stable identifier | solid in concept, with working partial implementations | [Routstr doc](https://raw.githubusercontent.com/Routstr/routstr-core/main/docs/ehbp-proxy-support.md), [OpenPCC whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf) |
| Not "completely anonymous" | solid | Timing, amounts, crowd size. [Trail of Bits](https://cyberinsider.com/wp-content/uploads/2026/04/2025-08-meta-whatsapp-privateprocessing-securityreview.pdf), [OpenPCC whitepaper](https://github.com/openpcc/openpcc/blob/main/whitepaper/openpcc.pdf) |
| ACT properties (hidden balance, visible spend) | unknown in this track | Covered by the credentials track. |
| Tor correlation limits | unknown in this track | Covered by the Tor track. |

### Reuse matrix
Licence notes are not legal advice. Maturity reflects public signals, and nothing was run.

| Component | Candidate | Licence | Maturity | Fit |
|---|---|---|---|---|
| Client-to-enclave body encryption | [EHBP](https://github.com/tinfoilsh/encrypted-http-body-protocol) | MIT | In production at Tinfoil since October 2025; small repo | High. Same on Tinfoil and own enclave. Headers stay open by design. |
| Alternative transport | OHTTP (Brave, Meta), [Noise Pipes (Confer)](https://confer.to/blog/2026/01/private-inference) | Varies | Deployed | Medium. OHTTP's IP hiding overlaps with Tor (inference). Meta uses chunked OHTTP for streaming. |
| Tinfoil client | [tinfoil-js, tinfoil-rs](https://github.com/tinfoilsh), [tinfoil-go](https://github.com/tinfoilsh/tinfoil-go) | Apache-2.0 shown; `verify/` AGPL v3 in Go; others unchecked | Active; V3 beta | High for the Tinfoil backend. Check licence of the verifier parts in each language. |
| Verifier for own CVM | [tinfoil-go/verify](https://raw.githubusercontent.com/tinfoilsh/tinfoil-go/main/verify/README.md) | AGPL v3 | Beta path; Tinfoil defaults | Low to medium. Reference or fork. TEE track covers alternatives. |
| CVM image | [cvmimage](https://github.com/tinfoilsh/cvmimage) | AGPL-3.0 | Active, Tinfoil-focused | Low to medium on rented CVM, pending V4. Strong design reference. |
| Alternative CVM stack | [dstack](https://github.com/Dstack-TEE/dstack) | Apache-2.0 | Described as production on TDX; audited | Medium if host control or TDX is available; SEV-SNP experimental. Pending Verda. |
| Measurement CI | [measure-image-action](https://github.com/tinfoilsh/measure-image-action) | AGPL-3.0 | v0.14.x referenced | Medium. Tied to Tinfoil config format. |
| Model packaging | [modelwrap](https://github.com/tinfoilsh/modelwrap) | MIT | 19 stars; used by Tinfoil | High. |
| Router | [confidential-model-router](https://github.com/tinfoilsh/confidential-model-router) | AGPL-3.0 | Active | Low as code (control plane coupling); high as reference for usage extraction, caps and model pinning. |
| Tinfoil as backend through a pooled key | [Proxy guide](https://docs.tinfoil.sh/guides/proxy-server.md) | Terms apply | Documented; used by three projects | High technically; blocked on written permission and Tor test. |
| Host a gateway on Tinfoil | [Containers](https://docs.tinfoil.sh/containers/overview.md) | Terms apply | Some features beta | Medium. Gives an attested gateway on Tinfoil but not on Verda. |
| Anonymous credit | [OpenPCC anonpay](https://github.com/openpcc/openpcc/tree/main/anonpay) | Apache-2.0 | Whitepaper claims implementations; I only found in-memory banks; no audit found; last commit January 2026 | High as a reference; medium as a base. Compare with ACT in credentials track. |
| Ecash reservation and change | [Routstr](https://github.com/Routstr/routstr-core) | GPL-3.0 | Very active | High as a behaviour reference; low as a base (Bitcoin, GPL). |
| Onion plus attested gateway | [Possums](https://github.com/ajbt200128/possums) | None yet, so not reusable | Early | Design reference only. |
| Card-to-blind-token flow | [LLM-Tor](https://github.com/prince776/LLM-Tor) | AGPL-3.0 | Small, whitepaper | Low to medium as reference. Drop abuse tokens and plaintext. |
| Compute node | [confidentcompute](https://github.com/confidentsecurity/confidentcompute) | FSL-1.1-ALv2 | 30 commits | Low. TPM and GCP/Azure assumptions. |
| Code identity log | Sigstore and Rekor (public good) via [Tinfoil's pattern](https://docs.tinfoil.sh/verification/attestation-architecture.md) | n/a | Mature | High. Issuer keys should use the same discipline. |
| Credentials library | [facebookresearch/acs](https://engineering.fb.com/2022/12/12/security/anonymous-credential-service-acs-open-source/) | MIT | Last push April 2024 | Low. Reference only; no value semantics. |
| Multi-backend routing | [brave/ai-gateway](https://github.com/brave/ai-gateway) | MPL-2.0 | Production at Brave | Low to medium. Config pattern only. |

### Gaps
- Live behaviour for all of the above is unverified until V1 to V5 are run.
- Cross-track: Verda launch control, vTPM and attestation (TEE track); ACT versus RSAPBSSA versus Cashu semantics (credentials track); Tor correlation and streaming fingerprinting (Tor track); card purchase correlation (payments).
