# Engine and client dependencies for the PoC

Read on 2026-11-06. No downloaded code was run, so every behaviour below comes from reading source or docs. "Not run" means exactly that. vLLM line numbers are from tag `v0.33.0` and the PyPI sdist (its SHA-256 `f4c33c25...` matched the PyPI digest). Crate facts come from crates.io JSON and the published tarballs. Last-commit dates come from each repo's GitHub commits feed. Work estimates are my own and are INFERRED.

| Component | Decision | Version (date) | Licence | What it saves us | Main risk |
|---|---|---|---|---|---|
| vLLM | ADOPT | 0.33.0 (2026-11-02) | Apache-2.0 | The whole GPU engine: scheduler, streaming, per-chunk usage, token IDs. Months of work | Large Python and CUDA trusted base; releases about every 2 weeks; nothing upstream on CC mode |
| llama.cpp `llama-server` | ADOPT, dev and CI only | v0.9.0 (2026-11-04); per-merge b12179 (2026-11-06) | MIT | Laptop engine with no GPU, already spiked | Differs from vLLM: no exact IDs while streaming |
| Qwen3-0.6B | ADOPT | commit c1899de289a0 (2025-07-26) | Apache-2.0 | Same model on both engines; native vLLM support; smallest serious permissive model | Thinking on by default; modest answers |
| uv (image build) | ADOPT | 0.13.7 (2026-11-03) | MIT OR Apache-2.0 | Hash-locked Python environment | FlashInfer kernels sit outside the lock |
| `tokenizers` crate | ADOPT, wallet only | 0.23.2 (2026-09-03) | Apache-2.0 | Real HF tokenizer in Rust; weeks | `onig` is a C dependency; 1.0 is in RC |
| `minijinja` + `minijinja-contrib` | ADOPT, plus ~80-line BUILD shim | 3.0.5 (2026-10-29) | Apache-2.0 | Jinja engine with Python string methods; weeks | Differences from Python Jinja; first thing to drop |
| `hf-hub`, `tiktoken-rs`, mistral.rs as a dependency | AVOID | 1.0.0; 0.12.1; n/a | Apache-2.0; MIT; MIT | Nothing we need | Extra network code or wrong tokenizer family |
| `axum` | ADOPT | 0.8.9 (2026-04-14) | MIT | Server, routing, streaming bodies, custom listener; about 2 weeks | Last release is 7 months old |
| `hyper` | ADOPT directly for upstream and Tor-side HTTP | 1.12.0 (2026-10-06) | MIT | HTTP/1.1 over any byte stream; about 2 weeks | Lower level; we own the connection task |
| `reqwest` | ADOPT, purchase plane only | 0.13.5 (2026-09-08) | MIT OR Apache-2.0 | HTTPS client with rustls and system DNS | System proxy is on by default |
| OpenAI subset structs + SSE splitter | BUILD | ours | ours | ~160 lines replace the type crates | Must track vLLM wire changes |
| `async-openai` | AVOID in gate and wallet; dev-dependency at most | 0.43.0 (2026-11-01) | MIT | Nothing in production | Silently drops the vLLM fields we need |
| `openai-api-rs` | AVOID | 10.0.2 (2026-10-03) | MIT | Nothing | No types-only build; pulls websockets and reqwest |
| `eventsource-stream` | AVOID | 0.2.3 (2022-02-17) | MIT OR Apache-2.0 | ~40 lines | No buffer limit; no commits since 2022 |
| `rusqlite` with SQLCipher | ADOPT | 0.41.1 (2026-10-28) | MIT (SQLCipher: BSD-style) | Durable encrypted ledger; about 1 week | OpenSSL C build; Rust 1.89 minimum |
| `keyring` | ADOPT, with passphrase fallback | 4.2.0 (2026-08-29) | MIT OR Apache-2.0 | OS keychain on three platforms | No store on headless Linux |
| `secrecy`, `zeroize` | ADOPT | 0.10.3 (2024-10-09); 1.9.1 (2026-10-06) | Apache-2.0 OR MIT | Redacted secrets, wipe on drop | Cannot cover copies in SQLite or TLS buffers |
| `clap`, `ratatui`, `directories` | ADOPT | 4.6.7; 0.30.2; 6.0.0 | MIT OR Apache-2.0; MIT; MIT OR Apache-2.0 | CLI, TUI, per-OS paths | `directories` is quiet since 2025-01, but small |
| Open WebUI | AVOID as shipped UI | v0.13.0 (2026-11-03) | Open WebUI License (branding clause) | Full UI, but no balance display without patching | Licence, and a heavy Python stack |
| LibreChat, LobeHub, chatbot-ui, AnythingLLM | AVOID | see section | MIT; custom; MIT; MIT | Nothing we need for the PoC | Size, licence, or abandonment |
| Embedded page in walletd | BUILD | ours | ours | ~300 lines, no npm, no third-party JS | Plain UI |
| xtask | ADOPT | convention, not a crate | n/a | Replaces shell scripts | None worth listing |
| `cargo-deny` | ADOPT | 0.20.2 (2026-07-09) | MIT OR Apache-2.0 | Licences, bans, sources, advisories in one check | Config upkeep |
| `cargo-audit` | AVOID (covered by cargo-deny) | 0.22.2 (2026-06-05) | Apache-2.0 OR MIT | Nothing extra | Duplicate |
| `cargo-vet` | AVOID for the PoC | 0.10.3 (2026-10-22) | Apache-2.0/MIT | Later value only | Audit labour |
| `assert_cmd` | ADOPT | 2.2.2 (2026-05-11) | MIT OR Apache-2.0 | CLI tests | None worth listing |
| `testcontainers` | AVOID for the PoC | 0.28.0 (2026-08-06) | MIT OR Apache-2.0 | Nothing plain processes do not give | Needs Docker |

## vLLM: ADOPT

- Version 0.33.0, uploaded to PyPI 2026-11-02T10:13:42. VERIFIED (https://pypi.org/project/vllm/0.33.0/). Licence Apache-2.0. VERIFIED (`pyproject.toml:19`, `LICENSE`).
- Org `vllm-project`. Last commit on main f8b11640, 2026-11-06T20:33:59Z. VERIFIED (GitHub).
- The GitHub release "v0.33.0" hangs off tag `v0.33.0rc3`, and `releases/tags/v0.33.0` returns 404. A git tag `v0.33.0` also exists. Pin by PyPI hash, not by release page. VERIFIED.
- Cadence: 0.30.0 on 09-22, 0.31.0 on 10-05, 0.32.0 on 10-18. VERIFIED (PyPI). Expect breaking notes: this release made requests during a pause return HTTP 503. VERIFIED (release notes).
- Rust frontend exists but is off by default: `VLLM_USE_RUST_FRONTEND: bool = False` (`vllm/envs.py:168`). Everything below applies to the Python frontend. VERIFIED.
- Trusted base: 104 `requires_dist` entries, including `torch==2.14.0`, `transformers>=5.16.1,<5.21.0`, `fastapi[standard]`, `flashinfer-python==0.7.2.post1`, `openai`, `anthropic`, `mcp` and OpenTelemetry. VERIFIED (PyPI JSON, `requirements/cuda.txt`).

### Behaviours the design depends on

| Need | Finding | Mark and source |
|---|---|---|
| Per-chunk usage | `stream_options.continuous_usage_stats` exists, default False. Each chunk gets `UsageInfo` with `completion_tokens=processor.num_output_tokens`, which grows by `len(delta_token_ids)` | VERIFIED: `entrypoints/generate/base/protocol.py:293`, `entrypoints/openai/chat_completion/serving.py:~836`, `stream_processor.py` |
| Final usage | `include_usage` adds a chunk with `choices=[]` after the loop | VERIFIED: `serving.py:864` |
| Force usage on the server | `--enable-force-include-usage` makes `should_include_usage` return `True, True`, whatever the request says | VERIFIED: `entrypoints/serve/utils/api_utils.py`, `entrypoints/common/cli_args.py:162`, `docs/features/per_request_metrics.md:93-95` |
| `stream_options` without streaming | Rejected: "Stream options can only be defined when `stream=True`." So the gate must always stream upstream | VERIFIED: `validate_stream_options` |
| `return_token_ids` | "In streaming mode, prompt_token_ids is included only in the first chunk, and token_ids contains the delta tokens for each chunk." A chunk is still sent when `delta_message` is None, so no IDs are lost | VERIFIED: `chat_completion/protocol.py:436`, `serving.py:739-745` |
| Reasoning parser hides IDs | `hide_metadata = parser is not None and not request.include_reasoning` drops IDs and logprobs. This closes the gap at `inference_metering.md:87` | VERIFIED: `serving.py`. Not run |
| `cache_salt` | Field says 1 to 1024, but `validate_cache_salt` limits it to 128 characters and bans `@ / \ NUL`. Injected into the first block's hash | VERIFIED: `chat_completion/protocol.py:507`, `_MAX_CACHE_SALT_LENGTH = 128`, `docs/design/prefix_caching.md:87` |
| Prefix cache default | **On.** `--enable-prefix-caching` defaults to None, then takes `model_config.is_prefix_caching_supported`. The feature doc's "Set enable_prefix_caching=True to enable" misleads. This resolves `inference_metering.md:181-183` | VERIFIED: `engine/arg_utils.py:1403, 3045, 3075-3076`, `config/cache.py:257`. True for text decoders is INFERRED (only the pooling branch was read) |
| Prefix cache off | `--disable-prefix-caching`. `EnableDisableAction` pairs `--enable-x` with `--disable-x`; `--no-enable-x` still works but is hidden | VERIFIED: `utils/argparse_utils.py:120-123`, `docs/models/structured_decisions_models.md:390` |
| Telemetry | On unless `VLLM_NO_USAGE_STATS`, `VLLM_DO_NOT_TRACK` or `DO_NOT_TRACK` equals exactly `"1"`, or `~/.config/vllm/do_not_track` exists. `DO_NOT_TRACK=true` does **not** opt out. Default target `https://stats.vllm.ai`, re-sent every 600 s | VERIFIED: `envs.py:840-850`, `usage/usage_lib.py`, `docs/usage/usage_stats.md:54-59` |
| Offline | `HF_HUB_OFFLINE` becomes `local_files_only` | VERIFIED: `transformers_utils/repo_utils.py:83, 291, 352` |
| Prompt logging | `enable_log_requests` and `enable_log_outputs` default False. Access log has `--disable-uvicorn-access-log` | VERIFIED: `engine/arg_utils.py:3247`, `config/observability.py:174`, `cli_args.py:319` |
| Disconnect | `AsyncLLM.generate` catches `CancelledError` and `GeneratorExit` and calls `self.abort`. For `StreamingResponse`, `with_cancellation` leaves disconnect detection to the response object. A short tail of a few steps was reported | VERIFIED: `v1/engine/async_llm.py:807-809`, `api_utils.py:85`; tail from `inference_metering.md` (issue 61918). Timing on CC is UNKNOWN |
| Finish reasons | `("stop", "length", "abort", "error", "repetition")`, wider than OpenAI's list | VERIFIED: `v1/engine/__init__.py:37` |
| Token limits | `max_tokens` is deprecated; `max_completion_tokens` wins if set. If both are missing, the limit is the remaining context | VERIFIED: `generate/base/protocol.py:244`, `serving.py:349`, `get_max_tokens` |
| Server ceiling | `--override-generation-config '{"max_new_tokens": N}'` sets `override_max_tokens`, which `get_max_tokens` takes the minimum of | VERIFIED: `arg_utils.py:1044`, `serving.py:182`. Not run |
| Unknown request fields | `ConfigDict(extra="allow")`, debug log only. vLLM will not reject anything, so the gate is the only filter | VERIFIED: `entrypoints/serve/engine/protocol.py:37-39` |

### Endpoints not to expose

All of these are registered with no off switch that I found: `/health` (200/503; for the gate's internal use only), `/metrics`, `/load`, `/version`, `/kv_event_sources`, `/ping`, `/invocations`, `/tokenize`, `/detokenize`, `/v1/load_lora_adapter`, `/v1/unload_lora_adapter`, `/start_profile`, `/stop_profile`, `/scale_elastic_ep`, `/is_scaling_elastic_ep`, `/v1/chat/completions/batch`, `/v1/responses`, `/v1/completions`, `/v1/messages`. `/docs` and `/openapi.json` go away with `--disable-fastapi-docs`. VERIFIED (`serve/__init__.py`, `instrumentator/`, routers).

`--api-key` protects only "`/v1`, `/v2`, `/inference`, and `/cohere`" prefixes, and `/invocations` "routes to the same inference functions". The doc's advice is a proxy that "explicitly allowlists only the endpoints you want to expose". VERIFIED (`docs/usage/security.md:152, 189-200, 280-286`). The gate already is that proxy, so `--api-key` adds nothing once the engine sits on a private socket.

`--uds`: "Unix domain socket path. If set, host and port arguments are ignored." VERIFIED (`cli_args.py:313`). `--sse-keep-alive-interval` defaults to 0 and emits `: keep-alive` comments, so our SSE parser must skip comments. VERIFIED.

### Image route

1. **CUDA spelling.** The Verda form and API, our notes, vLLM source and NVIDIA's pages all use the same spelling, so it is not a typo. VERIFIED.
2. **Userland.** PyPI `torch==2.14.0` requires `cuda-toolkit==13.0.3`, so CUDA 13.0 comes from wheels and no system toolkit is needed. VERIFIED (PyPI JSON). `VLLM_MAIN_CUDA_VERSION` is `"13.0"` (`envs.py:91`). VERIFIED. The install doc still says 12.9 (`gpu.cuda.inc.md:4, 53, 67`), and it is stale: the release notes say wheels are "CUDA 13.0 only". VERIFIED.
3. **Driver.** "CUDA 13 requires an R580 or newer driver." VERIFIED (`gpu.cuda.inc.md:346`). Verda's sample shows 580.126.09 (`verda_browser_notes.md:66`), and R595 is CUDA 13.2 (`tee_gpu_attestation.md:14`). NVIDIA's minor-version compatibility lets apps from the same major family run on a newer driver "with some feature limitations". VERIFIED (https://docs.nvidia.com/deploy/cuda-compatibility/). So torch's 13.0 on `cuda13.2.cc` or `cuda13.4.cc` should work. INFERRED, unverified on a running instance. Prefer `cuda13.2.cc`, which the attestation notes already cover. INFERRED. The branch behind 13.4 is UNKNOWN.
4. **Ubuntu.** System Python is 3.12.3 on 24.04 and 3.14.3 on 26.04. VERIFIED (packages.ubuntu.com). vLLM allows 3.11 to 3.14, and torch ships cp310 to cp314 `manylinux_2_28` wheels. VERIFIED. Pin 3.12 with uv-managed CPython, so the Ubuntu choice stops mattering to the engine. Use the 24.04 guest. INFERRED (uv's Python management was not read today).
5. **Lock.** `uv pip compile --generate-hashes`, then install with `--require-hashes`. VERIFIED (https://docs.astral.sh/uv/reference/cli/). `--torch-backend` exists only in the `uv pip` interface (`docs/guides/integration/pytorch.md:530`) and is not needed, because PyPI torch is already the CUDA 13.0 build. VERIFIED, INFERRED for "not needed".
6. **FlashInfer kernels are the hole in the lock.** `flashinfer-jit-cache` is 404 on PyPI, and `flashinfer-cubin` stops at 0.6.13 while vLLM pins `flashinfer-python==0.7.2.post1`. VERIFIED (PyPI). `flashinfer download-kernels` pulls from `https://flashinfer.ai/whl` (`flashinfer/__main__.py:315-322`), and cubins default to `edge.urm.nvidia.com` (`vllm/utils/flashinfer.py`). VERIFIED. Without kernels, FlashInfer "downloads and compiles kernels at startup"; with no cubin package and no nvcc or ninja, `has_flashinfer()` returns False and warns "FlashInfer kernels are disabled" (`utils/flashinfer.py:88-101`). VERIFIED. Upstream's image bakes them at build (`docker/Dockerfile:949-952`). VERIFIED. Plan: start with the disabled path, which needs no compiler and no fetch, and measure. Native-attention fallback performance is UNKNOWN. If it is too slow, bake them once, archive by hash, and consume the archive. Hosts with no visible GPU need `--sm` (`__main__.py:188`). VERIFIED.
7. **Read-only root.** "Cache contents are loaded without cryptographic integrity verification, including formats that support arbitrary code execution." VERIFIED (`security.md:414-416`). Put `VLLM_CACHE_ROOT`, `HF_HOME`, `~/.triton` and `~/.cache/flashinfer` on tmpfs owned by the engine user. INFERRED. Weights live on the verity volume and are served from a local path.
8. **No network for the engine.** systemd `PrivateNetwork=yes` leaves only loopback, while AF_UNIX sockets "located in the file system will continue to be accessible". VERIFIED (https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html). vLLM's internal ZMQ uses `ipc://` paths (`utils/network_utils.py:175`). VERIFIED. That makes telemetry and media fetches impossible rather than merely configured off. Interaction with the NVIDIA driver is UNKNOWN; test first.

Media fetching is real code: `vllm/connections.py` and `--allowed-media-domains`, described as an SSRF guard. VERIFIED (`security.md:63-66`). The gate must pass text only.

### RTX PRO 6000 Blackwell

The repo holds tuned configs named `device_name=NVIDIA_RTX_PRO_6000_Blackwell_Server_Edition`, and 168 merged PRs mention it. VERIFIED (`layers/fused_moe/configs/`, GitHub search). Blackwell needs CUDA 12.8 or newer. VERIFIED (`gpu.cuda.inc.md:51`). `grep -i confidential` in the docs finds nothing about CC mode. VERIFIED. Performance under CC rests on the third-party measurement in the earlier notes (arXiv 2606.23969). What `device_name` the CC SKU reports is UNKNOWN.

### Simpler alternative

A CUDA build of `llama-server` is one binary instead of a Python tree, and the backend trait already allows it. Drop vLLM for it if the Python environment blocks a reproducible image, or if vLLM fails under CC. The cost is losing `return_token_ids` while streaming, plus lower batching throughput. Do not do this first. INFERRED.

## llama.cpp `llama-server`: ADOPT for dev and CI

- Stable `v0.9.0`, 2026-11-04T13:08:06Z; its only asset is `nightly-tag.txt`. Binaries come from per-merge tags such as `b12179` (2026-11-06T20:43:24Z, marked prerelease, 38 assets). Pin one `b` tag and check its SHA-256 in xtask. VERIFIED (GitHub).
- MIT, "Copyright (c) 2023-2026 The ggml authors". Org `ggml-org`. Last commit 669204ee, 2026-11-06T19:51:57Z. VERIFIED.
- `tools/server/README.md` on master, VERIFIED: `--offline` (111), `--cache-ram`, 0 disables (168), `--host` also takes "UNIX socket paths ending in .sock" (193), `--no-webui` (210), `--no-cache-prompt` (223, default on), `--metrics` default off (225), `--no-slots` (227, default on), `--jinja` default on (235), `/health` public (480), `n_predict` with its overrun note (533), `cache_prompt` (597), `/tokenize` (687), `/apply-template` (738), `chat_template_kwargs` (1448), `timings` (1524).
- UDS on both engines means the gate has one upstream code path in dev and production. INFERRED.
- Limits from `spike_metering.md` (b12110): no exact IDs while streaming, `max_tokens=0` gives 1 token, 2-token overrun, stalled readers are not back-pressured. vLLM was not run in that spike.
- Dev model: SmolLM2-135M-Instruct Q4_K_M, 105 MB, Apache-2.0, already spiked. For Qwen parity, unsloth Q2_K is 296 MB and Q4_K_M 397 MB. CI smoke model: `ggml-org/test-model-stories260K` at 1.19 MB, which llama.cpp's own tests use (`tools/server/tests/utils.py:564`). VERIFIED. Its licence is UNKNOWN (no card metadata), so do not redistribute it.
- The fake backend carries most CI. Run real `llama-server` as a nightly job. INFERRED.

## Model: Qwen3-0.6B, ADOPT

| Model | Licence | Params / weights | vLLM 0.33.0 | Tokenizer and template | GGUF |
|---|---|---|---|---|---|
| **Qwen/Qwen3-0.6B** (c1899de289a0) | Apache-2.0, LICENSE file | 752M / 1.5 GB | Native (`registry.py:202`) | `tokenizer.json` 11.4 MB; template works with stock minijinja + pycompat | Official Q8_0 639 MB; unsloth Q4_K_M 397 MB |
| HuggingFaceTB/SmolLM3-3B (a07cc9a04f16) | Apache-2.0 on card, no LICENSE file | 3.08B / 6.15 GB | Through Transformers backend (`registry.py:766`), not native | 17.2 MB; template has `{% generation %}` and `strftime_now`, which minijinja cannot parse unaided | Q4_K_M 1915 MB |
| google/gemma-4-E2B-it (4adf15748de9) | Plain Apache 2.0 (ai.google.dev/gemma/docs/gemma_4_license), resolving `inference_metering.md:35` | 5.12B / 10.25 GB | Multimodal `Gemma4ForConditionalGeneration` (`registry.py:427`) | 32 MB; 18.5 KB template with macros | Q4_0 2841 MB |
| microsoft/Phi-4-mini-instruct (cfbefacb9925) | MIT, LICENSE file | 3.84B / 7.67 GB | Native `Phi3ForCausalLM` (`registry.py:197`) | 15.5 MB | Q4_K_M 2492 MB |

All VERIFIED (HF API, repo files, vLLM registry). Dropped: `meta-llama/Llama-3.2-1B-Instruct` (own licence, manual gate) and `google/gemma-3-270m-it` (Gemma licence, gated). VERIFIED.

- **Pick Qwen3-0.6B.** It is the same model on both engines, smallest, native in vLLM, 21.47M downloads, and already used on B200 in vLLM issue 61918 (`inference_metering.md`). This replaces SmolLM3-3B as the demo model in `inference_metering.md:33-39`.
- Thinking is on by default. The template emits an empty think block when `enable_thinking is false` (lines 86-87). VERIFIED. Both vLLM and llama-server accept `chat_template_kwargs`. VERIFIED.
- Changing the model should be one catalogue entry plus a new verity volume. `Qwen/Qwen3-1.7B` (4.06 GB) has the same tokenizer files and is the obvious upgrade if answers are too weak. VERIFIED for size and files, INFERRED for ease.

## Tokenizer recount: `tokenizers` ADOPT, `minijinja` ADOPT with a shim

**Who needs it.** The gate does not. It can compute holds through the engine's `/tokenize` on the private socket: `TokenizeChatRequest` takes `messages` and `chat_template_kwargs` (`entrypoints/serve/tokenize/protocol.py:49, 96`). VERIFIED. That method matched `usage.prompt_tokens` in 434/434 on llama.cpp (`spike_metering.md:272`). On vLLM it is not run. Only the wallet recounts.

**How much to trust it.** Text recount mismatched 242/434 requests, 1.5% low overall (`spike_metering.md`, Test 1). It can only be a tolerance check. That still catches gross inflation such as the 3.27x per-character split (`spike_metering.md:302`). INFERRED.

- `tokenizers` 0.23.2, Hugging Face, last commit 2026-11-06. crates.io newest is `1.0.0-rc.3`, 2026-10-19. VERIFIED. Real API: `Tokenizer::from_file`, `from_bytes`, `encode(input, add_special_tokens)`, `decode` (`src/tokenizer/mod.rs:1566, 1582, 871, 935`). VERIFIED.
- Default features are `progressbar`, `onig`, `esaxx_fast` (C++). Use `default-features = false, features = ["onig"]`; one of `onig` or `fancy-regex` is compulsory (`src/utils/mod.rs:14-15`). VERIFIED. `http` is opt-in, so no network by default. VERIFIED.
- `minijinja` 3.0.5, Armin Ronacher, last commit 2026-11-06. `minijinja_contrib::pycompat::unknown_method_callback` (`pycompat.rs:50`) provides `strip`, `lstrip` and `rstrip` with a chars argument (125-147), plus `split`, `startswith`, `endswith`. VERIFIED.
- Everything the Qwen3 template uses is present: `namespace` (`defaults.rs:246`), `is false` (`defaults.rs:212`), `is string`, `is defined`, `[::-1]` (`value/ops.rs:114`). VERIFIED. That it renders identically to Python is INFERRED; guard it with golden vectors from `apply_chat_template` checked in as fixtures.
- Real-world proof: mistral.rs (`pipeline/chat_template.rs:728, 822-824`, MIT) and TGI (`router/src/infer/chat_template.rs:35, 52-53`) both use pycompat plus `raise_exception`, `strftime_now` and template rewrites. VERIFIED. Copy the pattern, about 80 lines. Do not depend on them.
- `tojson` differs: minijinja HTML-escapes `< > & '` and uses spaced style (`filters.rs:1532`), and mistral.rs overrides it because "HF's tojson does not HTML-escape" (`chat_template.rs:406`). VERIFIED. It only affects tools, which are off in the PoC.
- AVOID: `llm-chat-template` (404 on crates.io), `hf-hub` 1.0.0 (files are pinned in the catalogue, so no download code), `tiktoken-rs` 0.12.1 (wrong tokenizer family for HF models). VERIFIED for facts, INFERRED for fit.

**Simpler alternative and when to drop.** Recount output text only and drop `minijinja`. Accept `prompt_tokens` from the attested gate. Do this if the golden test is awkward; nothing else depends on prompt recount.

## Gate HTTP and types: axum + hyper + BUILD

- `axum` 0.8.9, `tokio-rs`, last commit 2026-11-06. `axum::serve::Listener` (`serve/listener.rs:9`, `accept`, `local_addr`) lets an Arti-backed listener plug in. `Body::from_stream` (`axum-core/src/body.rs:59`) forwards rewritten frames. VERIFIED. The Arti side belongs to the Tor track.
- `hyper` 1.12.0, last commit 2026-11-02. `client::conn::http1::handshake` returns `(SendRequest, Connection)` over any IO (`src/client/conn/http1.rs:150`, `send_request` at 230). VERIFIED. An IO adapter is needed; `hyper-util`'s `TokioIo` is the usual one. INFERRED, not read.
- **Use hyper for gate to engine as well as wallet to gate.** One helper covers a Unix socket and an Arti stream. We own the connection task, so aborting it closes the socket when the wallet disconnects, which is what triggers vLLM's abort. I found no doc sentence promising that for dropping a reqwest response. INFERRED.
- Simpler alternative: `reqwest::ClientBuilder::unix_socket` (`client.rs:1829`) plus `Response::bytes_stream` (`response.rs:351`, `stream` feature). VERIFIED. Use it if the helper proves awkward. It adds no TLS with `default-features = false`. INFERRED.
- `async-openai` 0.43.0: `ChatCompletionStreamOptions` (`chat_.rs:1158`) has only `include_usage` and `include_obfuscation`; `ChatChoiceStream` (1347) has no `token_ids`; no `flatten` in chat types, so vLLM extras vanish. `FinishReason` (1185) is closed, so `"abort"` or `"repetition"` should fail to parse. `CreateChatCompletionRequest` (889) carries OpenAI-only fields we would have to strip anyway. VERIFIED, INFERRED for the parse failure. AVOID.
- `openai-api-rs` 10.0.2: closed `FinishReason` (`mod.rs:197`), pulls `tokio-tungstenite` and reqwest. VERIFIED. AVOID.
- `eventsource-stream` 0.2.3: handles comments (`parser.rs:88`) but `buffer: String` has no limit I could see (`event_stream.rs:137, 276`). VERIFIED. AVOID; a bounded splitter is about 40 lines.

**Recommended approach.** A small `proto` crate with explicit structs and no `deny_unknown_fields`. Serde's default is exactly the policy: "by default unknown fields are ignored for self-describing formats like JSON". VERIFIED (https://serde.rs/container-attrs.html).

- Request in: `model`, `messages`, `max_completion_tokens`, `max_tokens`, `temperature`, `top_p`, `stop`, `seed`, `frequency_penalty`, `presence_penalty`. `stream` is accepted but ignored upstream. `content` is a string or `{type:"text"}` parts, flattened to a string; anything else gets a 400. Roles: `system`, `user`, `assistant`.
- Set by the gate: `stream:true`, `stream_options`, `return_token_ids:true`, `chat_template_kwargs`, a clamped `max_completion_tokens`.
- Dropped by omission, among others: `chat_template`, `cache_salt`, `echo`, `ignore_eos`, `min_tokens`, `n`, `tools`, `logit_bias`, `request_id`, `session_id`, `vllm_xargs`, `structured_outputs`, `priority`, `stop_token_ids`, `kv_transfer_params`, `mm_processor_kwargs`. Field list VERIFIED (`chat_completion/protocol.py`). The choice is mine.
- Response out: also a small struct re-serialised by the gate, so `prompt_token_ids`, `prompt_logprobs` and `kv_transfer_params` cannot leak. `finish_reason: Option<String>` survives vLLM's wider list; map `abort`, `error` and `repetition` deliberately.
- Rendering `/tokenize` and generation from the same rewritten body removes the spike's mismatch where `add_generation_prompt:false` gave 30 tokens against 34 (`spike_metering.md`). INFERRED.

## Client stack

- `reqwest` 0.13.5, last commit 2026-10-19. Default features are `default-tls` (rustls with aws-lc-rs and platform verifier), `charset`, `http2`, `system-proxy`. VERIFIED (`Cargo.toml`, CHANGELOG `# v0.13.0`). Default DNS is `getaddrinfo` on a thread pool, per the `hickory-dns` note. "**NOTE**: System proxies are enabled by default." VERIFIED (`lib.rs`). Call `.no_proxy()` (`client.rs:1436`) so `ALL_PROXY` cannot reroute purchases, and do not enable `hickory-dns`.
- `rusqlite` 0.41.1, last commit 2026-11-06, Rust 1.89. `bundled-sqlcipher` "searches for and links against a system-installed crypto library"; `bundled-sqlcipher-vendored-openssl` builds OpenSSL (`README.md:122-126`). `pragma_update` in `src/pragma.rs`. VERIFIED. libsqlite3-sys sets `-DSQLITE_TEMP_STORE=2` for SQLCipher builds (`build.rs:192`), which is the "important step" in SQLCipher's design doc. VERIFIED.
- **Durability.** "A transaction committed in WAL mode with synchronous=NORMAL might roll back following a power loss or system crash." "With synchronous=FULL in WAL mode, an additional sync operation of the WAL file happens after each transaction commit." VERIFIED (https://sqlite.org/pragma.html). A double-spend guard needs `journal_mode=WAL` with `synchronous=FULL`, as `purchase_wallet_abuse.md:609` says. FULL is what matters; WAL is an optimisation.
- **Encryption, simplest for the PoC: SQLCipher with a random key held in `keyring`.** "100% of data in the database file is encrypted" (SQLCipher README), and "page data stored in the WAL file is encrypted using the database key" (https://www.zetetic.net/sqlcipher/design/). A raw key uses `PRAGMA key = "x'...'"` and skips PBKDF2 (https://www.zetetic.net/sqlcipher/sqlcipher-api/). VERIFIED. This is one Cargo feature, one pragma and about 30 lines, with no crypto code of ours, and it also hides timestamps and row counts, which column encryption would not. This resolves `purchase_wallet_abuse.md:631`. Not run. Use the vendored-OpenSSL feature so all platforms behave alike. INFERRED.
- `keyring` 4.2.0, Open Source Cooperative, last commit 2026-11-06, Rust 1.88. `Entry::new(service, username)`, `set_secret`, and `Error::NoDefaultStore` when no store initialises. Default feature `v1` covers macOS Keychain, Windows Credential Manager and Secret Service. VERIFIED. On `NoDefaultStore`, prompt for a passphrase and use `PRAGMA key = 'passphrase'`, which runs PBKDF2. INFERRED.
- Simpler alternative: plain `bundled`, mode 0600, rely on full-disk encryption. Fine for a first local milestone with test money; swapping later is a feature flag plus an export. INFERRED.
- `secrecy` 0.10.3 (`SecretBox`, `SecretString`, `ExposeSecret`, `src/lib.rs:58, 214, 262`) on `zeroize` 1.9.1 (`Zeroize`, `ZeroizeOnDrop`, imported at `lib.rs:45`). VERIFIED. Last commits 2026-10-17 and 2026-11-02. `secrecy`'s release is 2 years old but it is small. INFERRED.
- `directories` 6.0.0: `ProjectDirs::from(qualifier, organization, application)` (`lib.rs:428`), `data_local_dir`, `state_dir`, `runtime_dir`. VERIFIED. Last commit 2025-01-12. Small and stable.
- `clap` 4.6.7 (`derive` feature present; last commit 2026-11-02), `ratatui` 0.30.2 (MIT, Rust 1.88; last commit 2026-11-03). VERIFIED for metadata. APIs were not read: nothing unusual depends on them. Simpler alternative: skip the TUI in the first milestone, since the page and CLI cover the demo.

## Web UI: BUILD a small page in walletd

We need a model selector and balance display, against a local endpoint, with no telemetry.

- **Open WebUI** v0.13.0, 2026-11-03, 157,944 stars, active. Licence clause 4 forbids altering "Open WebUI" branding unless users stay at or under fifty in any rolling 30 days, or with permission or an enterprise licence. VERIFIED (`LICENSE`). Local endpoint: `OPENAI_API_BASE_URL` (`config.py:313`). Telemetry: the Dockerfile sets `SCARF_NO_ANALYTICS=true DO_NOT_TRACK=true ANONYMIZED_TELEMETRY=false` (95-97), `OFFLINE_MODE` disables the update check and sets `HF_HUB_OFFLINE` (`env.py:1199-1203`), OTEL is off by default (`env.py:1255`). VERIFIED on main. I did not audit all outbound paths, so "no telemetry" is UNKNOWN in the strong sense. It brings `chromadb` and `sentence-transformers`. Balance display needs a patch. Users may point their own copy at walletd.
- **LibreChat** moved to `LibreChat-AI/LibreChat`, MIT, last commit 2026-10-31. VERIFIED. Telemetry and deployment needs not evaluated.
- **LobeHub** v2.3.2, 2026-11-06, "LobeHub Community License": Apache-2.0 plus a commercial licence for derivative works. VERIFIED. AVOID.
- **chatbot-ui**: MIT, last push 2024-08-03, no releases. VERIFIED. Abandoned.
- **AnythingLLM**: MIT, active. PostHog telemetry on unless `DISABLE_TELEMETRY === "true"` (`server/models/telemetry.js:50`). VERIFIED. AVOID as default.
- **NextChat**: MIT. Not evaluated further (GitHub API limit).
- **Recommendation.** One HTML file via `include_str!`: `fetch` on `/v1/models` for the selector, a walletd-only `GET /wallet/balance`, streaming with `fetch` and a stream reader, text inserted with `textContent` so no Markdown renderer or third-party JS is needed. Strict CSP, no CDN. About 300 lines. INFERRED. Any OpenAI-compatible client still works against walletd `/v1`, but without balance. If nobody will use the page, drop it and demo with the CLI.

## Repository

- **Layout** (INFERRED, my recommendation):
  ```
  Cargo.toml  Cargo.lock  rust-toolchain.toml  deny.toml  .cargo/config.toml
  crates/proto  crates/act  crates/storefront  crates/gate
  crates/wallet-core  crates/walletd  crates/wallet-cli  crates/fake-engine
  xtask/  image/  tests/e2e/
  ```
  Toolchain floor is Rust 1.89 because of `rusqlite` (`keyring` and `ratatui` need 1.88). VERIFIED (crates.io).
- **xtask**: alias `xtask = "run --package xtask --"` in `.cargo`, needing nothing beyond cargo and rustc. VERIFIED (https://github.com/matklad/cargo-xtask, last commit 2025-10-28).
- **cargo-deny** covers licences, bans, advisories ("looking in an advisory database") and sources. VERIFIED (README). `cargo-audit` adds nothing on top. INFERRED. `cargo-vet` is for later.
- **Reproducible builds.** `--locked` errors when the lock is missing or would change; `--frozen` is `--locked` plus `--offline`; `cargo vendor` copies sources; `--remap-path-prefix` strips paths. VERIFIED (doc.rust-lang.org). Vendor only for the gate image. Prove it by building twice and comparing hashes. INFERRED.
- **E2E.** xtask starts fake-engine, gate, storefront and walletd as plain processes with TCP in place of Tor; tests drive walletd over HTTP. `assert_cmd` for the CLI (`cargo_bin_cmd`, `src/cargo.rs:67`). `testcontainers` requires Docker images (README line 17). VERIFIED. Real Tor in tests belongs to the Tor track.

## Maintainers and last commits

VERIFIED (crates.io `repository` field and each repo's GitHub commits feed, default branch, read 2026-11-06).

| Project | Maintainer or org | Last commit |
|---|---|---|
| axum | tokio-rs | 2026-11-06 |
| hyper | hyperium | 2026-11-02 |
| reqwest | seanmonstar | 2026-10-19 |
| async-openai | 64bit | 2026-11-01 |
| openai-api-rs | dongri | 2026-10-03 |
| eventsource-stream | jpopesculian | 2022-02-17 |
| tokenizers | huggingface | 2026-11-06 |
| minijinja | mitsuhiko | 2026-11-06 |
| rusqlite | rusqlite | 2026-11-06 |
| ratatui | ratatui | 2026-11-03 |
| clap | clap-rs | 2026-11-02 |
| secrecy | iqlusioninc | 2026-10-17 |
| zeroize | RustCrypto | 2026-11-02 |
| directories | dirs-dev | 2025-01-12 |
| keyring | open-source-cooperative | 2026-11-06 |
| cargo-deny | EmbarkStudios | 2026-10-26 |
| cargo-vet | mozilla | 2026-10-22 |
| cargo-audit | rustsec | 2026-11-05 |
| assert_cmd | assert-rs | 2026-11-01 |
| testcontainers | testcontainers | 2026-10-25 |
| uv | astral-sh | 2026-11-06 |

## Still UNKNOWN or not run

- Any vLLM behaviour under CC mode on the Verda RTX PRO 6000, and CUDA 13.0 wheels on the 13.2 or 13.4 images.
- vLLM disconnect timing over a Unix socket with a hyper client, and whether vLLM back-pressures stalled readers.
- Throughput with FlashInfer disabled, and whether `PrivateNetwork=yes` disturbs the NVIDIA stack.
- Whether minijinja reproduces Python's rendering byte for byte on the Qwen3 template, and whether `tokenizers` loads its `tokenizer.json`.
- `bundled-sqlcipher-vendored-openssl` build time and portability on our targets.

## Design impact

1. **Disable the prefix cache explicitly.** Add `--disable-prefix-caching` to the engine command, because the v0.33.0 default is on. Gate drops `cache_salt`. Dev engine gets `--no-cache-prompt --cache-ram 0 --no-slots --no-webui --offline`. If caching is wanted later, use a per-conversation base64url salt of at most 128 characters.
2. **Put the engine on a Unix socket with no network.** Replace any TCP binding with `vllm serve --uds <path>`, in a systemd unit with `PrivateNetwork=yes`. Socket permissions allow only the gate. The gate calls only `/v1/chat/completions`, `/tokenize` and `/health`. Drop `--api-key`.
3. **Fix the metering contract.** Start the engine with `--enable-force-include-usage` and `--override-generation-config '{"max_new_tokens": N}'`. The gate always streams upstream and sets `stream_options {include_usage, continuous_usage_stats}` and `return_token_ids`. Do not pass a reasoning parser: it can hide token IDs. Handle `abort`, `error` and `repetition` finish reasons.
4. **Change the demo model** to Qwen3-0.6B with `enable_thinking:false` set by the gate, replacing SmolLM3-3B. Use SmolLM2-135M for laptop dev.
5. **Change the gate's dependencies.** Holds use engine `/tokenize`. `tokenizers` and `minijinja` leave the gate and live only in the wallet, where recount is a tolerance check rather than an equality check.
6. **Change the HTTP stack on the use plane.** Gate to engine and wallet to gate both use `hyper` over a stream, and `reqwest` is purchase-plane only, built with `.no_proxy()`. This keeps reqwest, TLS and DNS code out of the gate, and enforces the two-stack rule by dependency.
7. **Own the wire types.** Add a `proto` crate with allowlist structs and a bounded SSE splitter, and no OpenAI type crate in production.
8. **Change the image build.** Hash-locked uv environment on Python 3.12, CUDA 13.0 from wheels, no system toolkit, `cuda13.2.cc` on Ubuntu 24.04. No compiler in the image, FlashInfer kernels disabled at first, or baked from a hashed archive. tmpfs for caches. Set `VLLM_NO_USAGE_STATS=1` and `DO_NOT_TRACK=1` (the value must be exactly `1`), `HF_HUB_OFFLINE=1`, `--disable-uvicorn-access-log`, `--disable-fastapi-docs`.
9. **Change the wallet store.** SQLCipher with a keychain-held random key, passphrase fallback on headless Linux, `journal_mode=WAL`, `synchronous=FULL`.
10. **Use an embedded page as the Web UI,** and add `GET /wallet/balance` to walletd. Because browsers can reach localhost, walletd checks the `Host` header and requires a random local bearer token. INFERRED, my reasoning rather than a source.
