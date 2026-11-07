# Confidential LLM serving and trustworthy per-token metering inside the enclave

Basis: source and documentation reading on 2026-11-06. Nothing was run against a real engine, so every engine behaviour below is **untested in practice**. Source reads are of `main`/`master` on that date, which can differ from the tagged release (vLLM latest tag: v0.33.0). Most source was read through a fetch tool that summarises and quotes, so exact lines should be re-checked before anyone relies on them in code. The only code run was a toy simulation written for this track: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/token_recount_sim.py`.

**Answer to the objective:** token counts can be enforced by the enclave today with ordinary engine features plus a small proxy. They can be checked exactly by the client only if the server exposes token IDs. Recounting from received text is not exact. No surveyed system implements a ceiling hold with returned change, so that part is new work.

## Serving engine options for a small open model in a GPU CVM on Blackwell

### Takeaway
vLLM is the only engine with direct evidence on Blackwell under GPU confidential computing, and it has the best metering primitives. Pin the version, and treat its CC scheduling default as a tuning item. For this PoC's small model, either GPU has far more memory than needed.

### Cited Findings

| Engine | Status and version | Blackwell evidence | CC-mode evidence |
|---|---|---|---|
| vLLM | v0.33.0, 2 Nov (year not shown on the page; a 4 Nov 2026 comment confirms it), [releases](https://github.com/vllm-project/vllm/releases) | B200 listed; Blackwell needs CUDA 12.8 or later, [install docs](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/). Tested on 1x B200 with Qwen3-0.6B in [issue 61918](https://github.com/vllm-project/vllm/issues/61918) | Measured under TDX plus GPU-CC on RTX Pro 6000 and B300, [Serialized Bridge](https://arxiv.org/html/2606.23969) |
| SGLang | NVIDIA's container notes list 0.5.14, [26.07 notes](https://docs.nvidia.com/deeplearning/frameworks/sglang-release-notes/rel-26-07.html) | Some recipes on B200 compile kernels at runtime and need nvcc 13.4 or later, [cookbook](https://docs.sglang.io/cookbook/autoregressive/MiniMax/MiniMax-M3.1). The same NVIDIA page says the container targets Hopper and Ampere, so its Blackwell claim is unclear | None found |
| llama.cpp server | Source on `master`, [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md) | None found | None found |
| TGI | Maintenance mode: only minor bug fixes, docs and light maintenance. It recommends vLLM, SGLang, llama.cpp or MLX, [README](https://raw.githubusercontent.com/huggingface/text-generation-inference/main/README.md). The 11 Dec 2025 date comes only from [a third-party wiki](https://aiwiki.ai/wiki/huggingface_tgi) | Not relevant | Not relevant |
| Ollama | Usage only on the final stream event, [middleware](https://raw.githubusercontent.com/ollama/ollama/main/middleware/openai.go) | None found | None found |

- **CUDA and drivers.** v0.33.0 release wheels and images are CUDA 13.0 only and the `-cu129` images stopped; v0.32.0 was the last with CUDA 12.9 artifacts — [vLLM releases](https://github.com/vllm-project/vllm/releases). CUDA 13 needs an R580 or newer driver, and a compatibility mode covers R535 and R570 hosts on select datacentre GPUs — [vLLM install docs](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/).
- **Verda fit (cross-track).** The browser track saw CC images with CUDA 13.4 or 13.2 and a sample driver 580.126.09. That looks compatible but is unconfirmed on a running instance — `verda_browser_notes.md` in this folder.
- **NVIDIA CC support.** RTX PRO 6000 is CC single-GPU only; B200 supports CC single and multi-GPU; both on SEV-SNP or TDX, with Ubuntu 26.04 host and 24.04 guest — [NVIDIA supported platforms](https://docs.nvidia.com/confidential-computing/virtual-machines/supported-platforms.md).
- **Memory.** RTX Pro 6000 Server Edition: 96 GB GDDR7, PCIe Gen5, no NVLink — [Serialized Bridge](https://arxiv.org/html/2606.23969). B200: DGX B200 has 8 GPUs and 1,440 GB, so 180 GB each by my division — [NVIDIA DGX docs](https://docs.nvidia.com/dgx/dgxb200-user-guide/introduction-to-dgxb200.html). Memory bandwidth of 1.6 versus 7.7 TB/s comes only from [an aggregator](https://flopper.io/compare/nvidia-b200-sxm-180gb-vs-nvidia-rtx-pro-6000-blackwell-server-edition/spec-sheet).
- **CC overhead.** Serving lost 13-27% throughput under TDX plus GPU-CC, attributed to the CVM-GPU bridge rather than GPU compute. Pro 6000 dense decode lost about 10%. A rate-capped online test on B300 lost 1.1%. This is a vendor-authored preprint (v2, 9 Jul 2026), and no CUDA version or Pro 6000 vLLM version is stated — [Serialized Bridge](https://arxiv.org/html/2606.23969).
- **CC tuning.** `--no-async-scheduling` closed 57% of the dense-decode gap and about 100% of the KV-churn TTFT penalty on B300. The "up to 92%" worker-thread drain figure comes from one sweep — [Serialized Bridge](https://arxiv.org/html/2606.23969). The flag exists in the [vLLM CLI](https://docs.vllm.ai/en/latest/cli/serve/).
- **What others serve with.**
  - Tinfoil's container docs use vLLM as the worked example — [Tinfoil models](https://docs.tinfoil.sh/containers/models.md). A search excerpt showed the tag `vllm/vllm-openai:v0.14.1`, which I did not confirm on the page. Its models overview does not name an engine — [Tinfoil overview](https://docs.tinfoil.sh/models/overview.md).
  - NEAR's now-archived proxy is described as "A proxy for vLLM" — [nearai/vllm-proxy](https://github.com/nearai/vllm-proxy).
  - The academic OpenPcc prototype used vLLM 0.8.5 on TDX plus H100. It is an Ohio State project, and the paper does not mention Confident Security's similarly named one — [OpenPcc](https://arxiv.org/html/2606.11145v1).
- **Candidate models.**
  - [SmolLM3-3B](https://huggingface.co/HuggingFaceTB/SmolLM3-3B): Apache-2.0, 3B parameters, 64k context, documented for vLLM, SGLang and llama.cpp.
  - [Qwen3-0.6B](https://huggingface.co/Qwen/Qwen3-0.6B): Apache-2.0, 0.6B parameters on the card (Safetensors metadata says 0.8B), 32,768 context.
  - [Gemma 4 E2B](https://huggingface.co/google/gemma-4-E2B-it) is a possible third. It is tagged Apache-2.0 but the licence link is named `gemma_4_license`, so check the terms.

### Inferences
- **Engine recommendation.** Use vLLM as the first backend and llama.cpp for CPU-only validation and tests. Do not build on TGI. Ollama lacks per-chunk usage and token IDs in the paths I read, so it fits only local development.
- **Model recommendation.** Use SmolLM3-3B for the demo and Qwen3-0.6B for tests. At BF16, SmolLM3 weights are about 6 GB (3B x 2 bytes), so the RTX PRO 6000 is enough and cheaper to justify. B200 adds nothing for one small model unless Verda availability forces it.
- **Reasoning cost.** Both candidates default to thinking on, which spends billed tokens the user may not expect. The default should be chosen deliberately and shown in the UI.

### Gaps
- NVIDIA's Secure AI Compatibility Matrix rows did not render in a static fetch, so I could not confirm the driver and VBIOS row for either GPU in CC mode — [matrix](https://www.nvidia.com/en-us/data-center/solutions/confidential-computing/secure-ai-compatibility-matrix/). This needs a browser or a live instance.
- I found no statement from vLLM itself that it supports CC mode. The evidence is third-party measurement.
- Whether Tinfoil's production fleet runs vLLM is inferred from its docs and from router code using vLLM-style options, not stated.
- I found no primary NVIDIA figure for B200 per-GPU memory bandwidth.

## Counting: how engines expose usage, and what happens on abort

### Takeaway
A final usage event is not enough for metering over Tor, because a disconnect cancels it. The gate must force per-chunk usage and keep the last value seen. Tinfoil's router does exactly this.

### Cited Findings

| Engine | Usage in stream | On client disconnect |
|---|---|---|
| vLLM | `include_usage` sends one final chunk. `continuous_usage_stats` puts running usage on every chunk. Final usage sits after the loop — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py) | `AsyncLLM.generate` catches `CancelledError` and `GeneratorExit`, then aborts in the output processor and engine core — [async_llm.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/async_llm.py). No `CancelledError` handler exists in the streaming generator, so I infer the final usage never runs — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py) |
| SGLang | `continuous_usage_stats` per chunk, final chunk under `include_usage` — [serving_chat.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/entrypoints/openai/serving_chat.py) | A background task sleeps 2 seconds, then aborts if the request is still live — [tokenizer_manager.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/managers/tokenizer_manager.py) |
| llama.cpp | Partial chunks carry `n_decoded` and `n_prompt_tokens`; the final result honours `include_usage` — [server-context.cpp](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/server-context.cpp) | The SSE loop exits on `should_stop`. A cancel task releases the slot. No check was visible between `n_batch` decode chunks — [server-context.cpp](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/server-context.cpp) |
| Ollama | Final usage-only chunk when requested, none per chunk — [middleware](https://raw.githubusercontent.com/ollama/ollama/main/middleware/openai.go); [openai.go](https://raw.githubusercontent.com/ollama/ollama/main/openai/openai.go) | No explicit handling in the code shown — [middleware](https://raw.githubusercontent.com/ollama/ollama/main/middleware/openai.go) |

- **Engine metrics cannot reconcile aborts.** A draft vLLM PR opens with "Client disconnects are not counted anywhere today" — [PR 60146](https://github.com/vllm-project/vllm/issues/60146). Testers saw `finished_reason="abort"` stay at 0 on v0.33.0 — [issue 61918](https://github.com/vllm-project/vllm/issues/61918).
- **Tinfoil's approach.**
  - `ensureStreamingUsageOptions` forces `include_usage` and `continuous_usage_stats` on every streaming request "so billing can extract token counts" — [router main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go).
  - The extractor keeps the latest non-zero values and calls the handler after the scan loop exits for any reason, so a read error still reports the last-seen usage. If no usage arrived, nothing is reported — [extractor.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/tokencount/extractor.go).
  - Its tests cover upstream read errors but assert nothing about usage after a partial stream or client disconnect — [extractor tests](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/tokencount/extractor_streaming_test.go).
  - A request cancelled before any response emits no billing event, and a non-streaming response closed early yields a zero-token event — [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go).
- **What is counted (vLLM).**
  - Prompt tokens are `len(res.prompt_token_ids)` — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py).
  - EOS or a stop token is left out of decoded text but appended to `token_ids`, so it is counted — [detokenizer.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/detokenizer.py).
  - Tokens generated past a matched stop string are trimmed from the count — [output_processor.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/output_processor.py).
  - Reasoning tokens are tracked as a separate figure; `n` greater than 1 sums completions across choices and counts the prompt once — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py).
  - SGLang tracks reasoning tokens too, and reports cached tokens only with `--enable-cache-report` — [serving_chat.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/entrypoints/openai/serving_chat.py); [SGLang args](https://docs.sglang.io/advanced_features/server_arguments.html).
- **Coalescing.** With `stream_interval` above 1, vLLM buffers tokens between sends — [output_processor.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/output_processor.py). The default is 1 — [vLLM CLI](https://docs.vllm.ai/en/latest/cli/serve/).
- **Attribution conflict.** Commenters credit PR 58942 for fixing a stuck streaming case between v0.32.0 and v0.33.0 — [issue 61918](https://github.com/vllm-project/vllm/issues/61918). The PR's stated scope is streaming-input sessions, not normal SSE — [PR 58942](https://github.com/vllm-project/vllm/pull/58942). The real cause of the fix is unclear.

### Inferences
- **What the billed counts include.** Because prompt tokens are measured on `prompt_token_ids`, they should include everything added by rendering: template text, special tokens, tool schemas and any hidden system prompt. Output counts should include reasoning, tool-call tokens and EOS. Users will therefore see more tokens than their visible text implies, and the UI should say so. Tinfoil's event comment treats cached tokens as a subset of prompt tokens — [events.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/billing/events.go).
- **Meter definition.** Over Tor, "delivered" cannot be observed without an application acknowledgement. The honest meter is "tokens the gate sent". Generated-but-undelivered tokens in proxy, TCP and Tor buffers are billed, while the engine's in-flight tail after an abort is unbilled and absorbed by the operator.
- **Scale of the undelivered gap.** At 100 tok/s, a client that takes an illustrative 2 seconds to notice a dead circuit is billed for about 200 tokens it never saw. The real figure needs measuring.
- **Free-prefill abuse.** A client could send a long prompt and disconnect before the first token. Tinfoil's code would report no usage in that case, because it bills an identified account afterwards. An anonymous prepaid gate should charge input tokens at admission.
- **Force streaming upstream.** The gate should always stream from the engine, even for a non-streaming client, because partial JSON yields no count.
- **Gate placement.** Use a small proxy in the attested boundary, talking to the engine over localhost. It stays engine-independent, keeps the engine unpatched, and owns disconnect handling, padding and signing. It relies on the engine's count, which is acceptable because the engine is measured too. Engine patches would break modularity and make upgrades harder.

### Gaps
- How vLLM's reasoning parsers treat `</think>` markers in returned text was not confirmed. The split is delegated to a parser I did not read.
- The exact streaming behaviour of `return_token_ids` under `hide_stream_metadata` is unread.
- The Tinfoil docs page behind the prior conversation's "soft limits" claim was not located. Code reading shows post-hoc billing but no enforcement logic.

## Ceiling pre-authorization: computing, enforcing and settling a hold

### Takeaway
The computation and enforcement are feasible with existing engine behaviour, but **no surveyed system implements a token hold**. What happens to a hold when a stream dies has to be designed from scratch, and it is the riskiest part of the metering design.

### Cited Findings
- **No existing hold in Tinfoil.** `proxy.go` adds billing events after usage is known and has no reservation code — [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go). Pricing is computed after the request from reported usage — [pricing.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/pricing.go). "Reservations" in that codebase means dedicated host capacity, not tokens — [reservation tests](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/reservations_test.go).
- **Useful Tinfoil primitives.**
  - An optional `X-Tinfoil-Max-Output-Tokens` header overrides the body limit, so a proxy in front of the enclave can learn the limit while the body stays sealed — [max_output_tokens.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/max_output_tokens.go).
  - The input-token handler forwards messages, tools and template fields to the engine's `/tokenize` and returns the count — [input_tokens.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/input_tokens.go).
- **Enforcement (vLLM).** `check_stop` ends a request as `FINISHED_LENGTH_CAPPED` when output reaches `max_tokens` or total tokens reach `max_model_len`, and also handles EOS and stop token IDs — [sched/utils.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/core/sched/utils.py). Stop strings are matched later, in the frontend detokenizer, which then aborts the engine request and trims surplus tokens — [output_processor.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/output_processor.py).
- **Enforcement (llama.cpp).** `n_predict` "May exceed the set limit slightly if the last token is a partial multibyte character", and its default of -1 is unlimited — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md).
- **Stop speed after cancellation.**
  - vLLM: a tester in the issue says "A short tail after the cut is expected", covering "a few steps". Disconnect detection took 10-12 ms in CPU tests. On 1x B200 with Qwen3-0.6B, generation "stopped at ~1,300" tokens after a 2-second cut — [issue 61918](https://github.com/vllm-project/vllm/issues/61918).
  - vLLM v0.33.0: `num_requests_running` was back to 0 by +30 s in a test on 2x H100 NVL; on v0.32.0 it stayed at 26 for 5 minutes — [issue 61918](https://github.com/vllm-project/vllm/issues/61918).
  - SGLang: a deliberate 2-second delay precedes the abort — [tokenizer_manager.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/managers/tokenizer_manager.py).
- **Stream end is not authenticated.** EHBP uses no sentinel chunk, and the spec says "transport EOF alone does not prove that the application response is complete", so completion must be established at the application level. It delivers an authentic prefix and does not support resuming from an offset — [EHBP spec](https://raw.githubusercontent.com/tinfoilsh/encrypted-http-body-protocol/main/SPEC.md).
- **Key lifetime.** Attested keys persist across container restarts within one boot, and a reboot, deploy or update generates new ones — [Tinfoil attested keys](https://docs.tinfoil.sh/containers/attested-keys.md). Meta reboots all CVMs regularly — [Meta whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper).

### Inferences
- **Reading the 1,300 figure.** That run was 12,000 tokens in about 18 s, or about 667 tok/s, so 2 s gives about 1,330 tokens. "Stopped at ~1,300" is therefore the total at the cut and not a tail of 1,300 tokens, which agrees with "a few steps". At the 27-31 ms per-token times reported at c=128 on B300 in [Serialized Bridge](https://arxiv.org/html/2606.23969), a few steps is on the order of 100 ms. That is my estimate, not a measurement.
- **Hold formula.** `hold = P_in x input_tokens + P_out x n x effective_max_tokens`, with `effective_max_tokens = min(requested, context - input_tokens)`. Clamp `n` to 1 in the PoC. Always set `max_tokens` explicitly, because I could not confirm the default clamping logic. Settle as `charge = min(hold, computed)` so any engine overshoot falls on the operator.
- **Ordering problem (cross-track).** If the ACT spend amount is chosen by the client before the enclave sees the prompt, the client needs a tokenizer or a safe upper bound first. An enclave-side rejection ("need X") must cost nothing. Bucketed ceilings would also reduce what the amount reveals.
- **Dead-stream settlement.** A workable shape:
  1. The client includes a random request secret, encrypted to the enclave.
  2. The gate settles on stream end, cap or disconnect, and stores `{H(secret), receipt, change}` in RAM with a short TTL.
  3. The client retrieves it over a fresh Tor circuit using the secret.
- **What can still lose value.** A reboot or crash before retrieval loses the change. CVMs should drain and wait out the TTL before a planned reboot. Crash loss remains, and needs either a stated policy or an escrow design from the credentials track.
- **Holds and concurrency.** Unsettled holds lock the client's balance for that request's lifetime, including retrieval. This is another reason to keep ceilings near real use.

### Gaps
- Cancellation latency under GPU-CC has no published measurement. Cancellation during a long prefill is also unmeasured.
- What the v0.33.0 +30 s figure means is unclear, since it is a sample time and not a latency measurement.

## Reconciliation: can the client independently recompute the count?

### Takeaway
Not exactly, from text alone. With token IDs returned by the server, an exact check of delivered tokens is easy. Attestation is still what makes the count trustworthy; client recounting is a useful check on top, and it is only approximate when a stream dies.

### Cited Findings
- **Things that break recounting.**
  - Stop tokens are counted but not decoded, and streamed text withholds up to the stop string's length minus one until finish — [detokenizer.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/detokenizer.py).
  - `skip_special_tokens` changes the decoded text — [detokenizer.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/detokenizer.py).
  - The detokenizer has a recovery path for invalid UTF-8 output — [detokenizer.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/detokenizer.py).
  - llama.cpp documents limits interacting with partial multibyte characters — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md).
  - Reasoning and tool output pass through a parser that re-forms text — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py).
- **Chat templates are not pure.**
  - `strftime_now` lets a template read the clock. Any keyword argument can reach a template. Python methods in a template break in JS or Rust Jinja, and rendering a dict or list directly can differ across implementations without `tojson` — [HF chat template docs](https://huggingface.co/docs/transformers/chat_templating_writing).
  - Loading precedence across `chat_template.jinja` and legacy locations can change which template applies — [HF chat template docs](https://huggingface.co/docs/transformers/chat_templating_writing).
  - `enable_thinking` and `/think` change rendering on both candidates — [Qwen3-0.6B](https://huggingface.co/Qwen/Qwen3-0.6B); [SmolLM3-3B](https://huggingface.co/HuggingFaceTB/SmolLM3-3B).
- **Library versions can change tokenization.** Transformers warns that some Mistral `tokenizer.json` regexes lead to incorrect tokenization. The fix was debated because it changes behaviour, and false positives were reported — [PR 42131](https://github.com/huggingface/transformers/pull/42131); [PR 44736](https://github.com/huggingface/transformers/pull/44736). So pinned files alone may not be enough.
- **Engines can expose IDs.**
  - vLLM's `return_token_ids` is documented — [OpenAI server docs](https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/). `prompt_token_ids` comes on the first chunk and `token_ids` per chunk, gated also by `hide_stream_metadata` — [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py).
  - vLLM also has `/tokenize`, `/detokenize`, renderer endpoints and a token-in/token-out Generate API — [online serving](https://docs.vllm.ai/en/latest/serving/online_serving/).
  - llama.cpp has `return_tokens`, `/tokenize` and `/apply-template` — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md).
  - SGLang can return input and output IDs in `sglext`, and it omits them after an error abort — [serving_chat.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/entrypoints/openai/serving_chat.py).
- **Signed response precedent (NEAR AI Cloud).**
  - Payload is `<MODEL_ID>:<request_hash>:<response_hash>` with SHA-256 over exact bytes, signed with ECDSA or Ed25519 by a key checked against an attestation report.
  - Token counts are not separate signed fields, and the gateway removes per-chunk usage on streams by default.
  - It warns that "Instances of the same model share one signing key", so a matching signer does not prove the same instance served the request — [NEAR response signatures](https://docs.near.ai/cloud/verification/cloud-api/response-signatures.md).
- **Attested keys.** Tinfoil can generate Ed25519 or ECDSA P-256 keys inside the enclave and publish the public key in the verified attestation document. The page does not mention receipts or say how the entry is bound to the report — [Tinfoil attested keys](https://docs.tinfoil.sh/containers/attested-keys.md).
- **Toy simulation (mechanism only, not a real-world rate).** With a perfect tokenizer and canonical output, recount was 100% exact. Non-canonical token choices dropped exact recounts to 79.2% at 2% noise and 53.8% at 5% noise, mostly from lossy UTF-8 in my toy generator. A hidden EOS gave 41 against 40, and a stop-string trim gave 40 against 6. A token-ID hash chain matched exactly and detected a one-token truncation. SHA-256 ran at 4.36 M links/s here. Adding usage and IDs grew a chunk from 206 to 310 bytes in my example JSON — [simulation script](/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/simulations/token_recount_sim.py).

### Inferences
- **Three levels of check.**
  1. Text recount: an estimate only, useful for a UI sanity range.
  2. Token IDs: exact count of delivered tokens, plus confirmation they decode to the text shown.
  3. Signed receipt: makes the count, price and model binding non-repudiable for this boot's key.
- **Is a hash chain needed?** A running SHA-256 over IDs with one final signature is enough. Authenticated framing already gives an authentic prefix, so per-chunk signatures add cost for little gain. Hashing cost is negligible against token rates.
- **Input tokens.** With a template free of clock and environment use, a client holding the attested tokenizer and template can recount exactly. It should still compare against `prompt_token_ids` or a hash of them, because Rust or JS Jinja differences are likely in practice.
- **Receipt contents.** `{request nonce, model and catalogue digest, price version, input tokens, output tokens, H(prompt IDs), H(output IDs), finish reason, hold, charge}` signed with a per-boot attested key. Keep receipts on the client. Showing one to the issuer in a dispute would link that spend to the request.
- **Portability limit.** `continuous_usage_stats`, `return_token_ids` and `cache_salt` are engine extensions, not OpenAI API. Backends should therefore declare capabilities: per-chunk usage, token IDs, signed receipts. Tinfoil's hosted inference, as far as I could confirm, offers no client-verifiable signed token count. Running your own gate in Tinfoil Containers would.
- **What the check cannot prove.** On a dead stream, delivered IDs will be fewer than the receipt's count, and that gap cannot be told apart from an over-count by the client alone. Attestation has to cover that case.

### Gaps
- The real recount mismatch rate is unknown without running the validation below.
- Whether Tinfoil's router returns token IDs when asked, or filters them, was not checked. The `usageMetricsWriter` can put usage in a header or trailer, but its format lives in code I did not read — [usage_writer.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/usage_writer.go).

## Isolation and statelessness

### Takeaway
Prefix caching should be off for this design. Tinfoil's mitigation depends on an identity this system deliberately lacks, and a cross-request cache hit is itself linkage. The cost is real and it shows up in the user's bill.

### Cited Findings
- **Audited leakage.** Statistical audits found global cache sharing across users at seven API providers, including OpenAI. Authors worked with providers on fixes — [Gu et al., ICML 2025](https://proceedings.mlr.press/v267/gu25b.html); [poster](https://icml.cc/virtual/2025/poster/44473).
- **Prompt theft.** InputSnatch reports 87.13% accuracy determining cache-hit prefix lengths and a 62% success rate on exact medical inputs, with limits from memory, rate limits and TTL — [InputSnatch](https://arxiv.org/html/2411.18191v2).
- **Single-author 2026 preprint, partly simulated.**
  - Song et al. reached 100% token recovery at block size 1. vLLM 0.26.0 removed that option and defaults to 16, which the author says takes EarlyBird to 0% in practice.
  - Salted namespaces give 0% attack success in its simulated trials.
  - Measured TTFT was 149.6 ms cold and 32.8 ms cached on A100 with a 2,119-token prefix, and 1861 ms and 173 ms for llama.cpp via Ollama on Apple M4.
  - SGLang's radix cache leaks a finer matched-prefix length than block caches — [KVGov](https://arxiv.org/abs/2608.09225).
- **vLLM controls.** `cache_salt` goes into the first block's hash — [vLLM prefix caching](https://docs.vllm.ai/en/latest/design/prefix_caching/). v0.31.0 fixed key collisions between LoRA names and `cache_salt` — [vLLM releases](https://github.com/vllm-project/vllm/releases). `--no-enable-prefix-caching` exists — [APC docs](https://docs.vllm.ai/en/latest/features/automatic_prefix_caching/).
- **Conflict on the vLLM default.** The docs say to set `enable_prefix_caching=True` to enable it, which suggests off by default, while the CLI page states no default — [APC docs](https://docs.vllm.ai/en/latest/features/automatic_prefix_caching/); [vLLM CLI](https://docs.vllm.ai/en/latest/cli/serve/). Treat it as unknown: set the flag explicitly and verify.
- **llama.cpp.** `cache_prompt` and prompt caching default to on, and `/slots` and the web UI default to on — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md). Idle slots can be saved to a prompt cache — [server-context.cpp](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/server-context.cpp).
- **Tinfoil.**
  - Caching is on by default. The router combines an SDK secret with authenticated API identity to select a namespace, the cache stays in GPU memory and is never written to disk, and a prefix "stays cached until the inference server evicts it" — [Tinfoil prompt caching](https://docs.tinfoil.sh/sdk/prompt-caching.md).
  - The router strips any client `cache_salt`, because a client "must never choose the salt" — [cache_salt.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/cache_salt.go).
  - This confirms the prior conversation's statement about per-user scoping.
- **Meta.**
  - History is sent from the client with every request. The Predictor "turns off persistent KV Cache" so all computation and cache state "has the same lifecycle as the request". Components store no persistent data, and "We regularly reboot all CVMs".
  - It also lists rate limiting against side channels and privacy-preserving crash reporting. A "prod.pc.predictor.cache" artifact is a verified LLM runtime kernel cache, so that cache is not user state.
  - The whitepaper gives no cost or performance figure for these choices in the text I searched — [Meta whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper). Extraction was by my own script over the PDF, and no date was found in the text.
- **Streaming side channel.** Whisper Leak infers prompt topics from encrypted streaming packet sizes and timing. Padding, batching and packet injection each help and none is complete. Headline figures were read through summaries only — [Microsoft Research](https://www.microsoft.com/en-us/research/?p=1155905). EHBP frame lengths "reveal ciphertext chunk sizes and boundaries" — [EHBP spec](https://raw.githubusercontent.com/tinfoilsh/encrypted-http-body-protocol/main/SPEC.md).

| Item | vLLM | SGLang | llama.cpp |
|---|---|---|---|
| Request logging | `--enable-log-requests` and `--enable-log-outputs` default off; `--max-log-len` unlimited — [CLI](https://docs.vllm.ai/en/latest/cli/serve/) | `--log-requests` off; level 3 logs every input and output — [args](https://docs.sglang.io/advanced_features/server_arguments.html) | `--log-prompts-dir` off; `-v` logs everything — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md) |
| Access log and metrics | Uvicorn access log on, since disable defaults to False — [CLI](https://docs.vllm.ai/en/latest/cli/serve/) | `--enable-metrics` off — [args](https://docs.sglang.io/advanced_features/server_arguments.html) | `--metrics` off — [README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md) |
| Telemetry | **Anonymous usage data on by default.** Opt out with `VLLM_NO_USAGE_STATS=1`, `DO_NOT_TRACK=1` or a `do_not_track` file — [usage stats](https://docs.vllm.ai/en/latest/usage/usage_stats/) | None found in the part of the page I read | None found |
| Crash dumps | `dump_engine_exception` has no flag. Tensors are redacted and `anon_repr` is used where present, but plain strings and lists print in full — [dump_input.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/logging_utils/dump_input.py) | `--crash-dump-folder` keeps completed and in-flight requests when set, and `dump_requests_folder` pickles full request objects — [args](https://docs.sglang.io/advanced_features/server_arguments.html); [tokenizer_manager.py](https://raw.githubusercontent.com/sgl-project/sglang/main/python/sglang/srt/managers/tokenizer_manager.py) | Not established |

### Inferences
- **A cache hit is linkage.** Reuse across requests lets anyone timing the encrypted traffic see a long prompt answered quickly, which ties it to an earlier request. That undermines the unlinkable-spend goal even if contents stay hidden. Salting stops other users from probing, but it does not stop the operator from seeing this. Discounted cached-token pricing would additionally leak hits through the spend amount.
- **Salting has no identity to anchor to.** Tinfoil derives its salt from an authenticated identity. Without one, the only options are off, a per-request random salt (no cross-request benefit, with KV lingering until evicted), or a client-supplied conversation secret that gives up the linkage property above. Off is the clean choice.
- **The cost lands on the bill.** Re-sending history makes billed prefill grow quadratically. Twenty turns at 200 tokens each gives 200 x (1+...+20) = 42,000 input tokens, against 4,000 new tokens with reuse, which is 10.5 times more. Pricing must allow for this. Growing ciphertext sizes and hold amounts across turns are also a linkage signal between requests (cross-track).
- **The gate is also the right place for padding** and token batching, since it already owns framing. Padding adds to bandwidth that Tor must carry (cross-track).
- **Disk and logs.** Treat engine logs as sensitive in all cases. Send logs to memory or nowhere, disable swap and core dumps, mount scratch space as tmpfs and run engines offline against verified local weights. This list is my recommendation, and the engine flags alone will not achieve it.
- **Keep the TCB small.** vLLM has had recent advisories in this surface: chat template resource exhaustion, and per-request multimodal kwargs now gated behind a flag — [vLLM releases](https://github.com/vllm-project/vllm/releases). The gate should allowlist request parameters and strip `chat_template`, `cache_salt`, `n` above 1 and engine-specific extras.

### Gaps
- I found no statement on whether vLLM zeroes freed KV blocks between requests. Meta's "same lifecycle" wording does not specify zeroing either.
- SGLang's `--disable-radix-cache` was not confirmed in the part of the docs I read. Telemetry in SGLang and Ollama was not established.
- No primary source gives the production cost of Meta's stateless choice.

## Model selector: attested catalogue, pricing, routing and gateways

### Takeaway
Attested model pinning already exists in a reusable form at Tinfoil. Signed price tables did not turn up anywhere. Keep the PoC to one model in one CVM, but make the catalogue a signed, measured document from the start. The OpenAI API works as the outer interface but does not carry what metering needs.

### Cited Findings
- **Weights.** Modelwrap turns a pinned Hugging Face commit into a read-only package with a dm-verity root hash. The config commits to that hash, dm-verity checks each read, and anyone can recompute it with `modelwrap --schema 2 <repo>@<commit>` — [Tinfoil models](https://docs.tinfoil.sh/containers/models.md). The config's SHA256 hash is embedded in the kernel command line — [Tinfoil attestation architecture](https://docs.tinfoil.sh/verification/attestation-architecture.md).
- **Engine pinning.** vLLM has `--revision`, `--tokenizer-revision` and `--code-revision`, each defaulting to "the default version" if unset. `--trust-remote-code` defaults to False — [vLLM CLI](https://docs.vllm.ai/en/latest/cli/serve/).
- **Prices.** Tinfoil's `ServiceUsage` comment says pricing comes from "the service's published pricing from the model catalog". No signing or verification appears in that code — [pricing.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/pricing.go).
- **Router.** Tinfoil's router authenticates callers and checks rate limits with a control plane, resolves the model (or `auto`) to a healthy enclave, runs server-side tools and streams back — [router repo](https://github.com/tinfoilsh/confidential-model-router). Its billing `Event` holds `UserID`, `APIKey`, `RequestID`, `Enclave` and token counts, and goes to a control-plane endpoint — [events.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/billing/events.go).
- **Gateways.**
  - LiteLLM 1.82.7 and 1.82.8 were credential-harvesting releases in March 2026 — [GHSA via GitLab](https://advisories.gitlab.com/pypi/litellm/GHSA-5mg7-485q-xm76/). A 21 Oct 2026 compromise of 1.107.0 is reported by one source that I could not corroborate — [Konfirmity](https://www.konfirmity.com/advisories/litellm-1-107-0-supply-chain-compromise).
  - Envoy AI Gateway is now "Agent Router", at docs version 1.2, using Kubernetes-native resources, with a local `aigw run` quickstart — [Agent Router docs](https://theagentrouter.ai/docs/).

### Inferences
- **Catalogue entry.** `{model id, HF repo and commit, dm-verity root, tokenizer hash, template hash, engine image digest, context limit, max token limit, price version}`. Tokenizer and template hashes are what make client reconciliation possible. Measuring the price table into the image makes each price change a new measurement; signing it with a key in the transparency log avoids that at the cost of one more key to trust. Either is workable; I found no precedent to copy.
- **Per-model enclave versus router enclave.** A router gives one attested entry point and hides the model name from the host's first hop, but router-to-model traffic and timing can still reveal the choice, and clients trust the router's checks of model enclaves. Per-model enclaves give direct attestation and expose the model by destination. RTX PRO 6000 is single-GPU in CC, so each GPU-backed model likely needs its own CVM or a shared engine host. For the PoC, a gate and engine in one CVM avoids this entirely.
- **Model choice does reveal something.** Rarely used models split the anonymity set, and differing prices reveal the model through the spend amount. A flat unit price across similar models reduces that. Note also that Tinfoil's cleartext-header primitive would expose `max_tokens` to any fronting proxy.
- **The Tinfoil modular path is two different integrations.** Running your own gate in Tinfoil Containers keeps everything above. A pooled account on hosted Tinfoil Inference means your gateway learns counts from a header or trailer, not from a receipt your client can verify, and Tinfoil's `Event` still records the pooled key. That second mode offers weaker client-side guarantees and should be labelled so.
- **Gateway choice.** Do not place LiteLLM or a Kubernetes gateway inside the enclave boundary for the PoC. Their size and supply-chain exposure are out of proportion to one route. A small custom gate, informed by Tinfoil's `tokencount` approach, is simpler. Check the router's licence before copying code, as I could not read it from the page.

### Gaps
- I found no signed price table implementation in any surveyed system.
- Client traffic analysis of router versus per-model designs has no measurement here.

## Proposed validation

Nothing here has been run. Times are rough estimates for one person on a laptop or one instance.

**V1. Recount mismatch (about half a day).** Local OpenAI-compatible server (vLLM CPU or llama.cpp) with a tiny CPU model such as a small Qwen3-0.6B quantisation.
- **Corpus:** at least 200 prompt and completion pairs across Unicode and combining marks, emoji and ZWJ sequences, CJK, code with odd whitespace, long digit strings, stop sequences, `max_tokens` values forced to cut mid-character, thinking on and off, and tool definitions.
- **Each run compares:** server usage; independent recount using `tokenizer.json` and `chat_template.jinja` at the pinned commit; `return_token_ids` output where supported.
- **Report:** mismatch rate and signed difference per category, split into input and output, with a root cause assigned to every mismatch.
- **Confirms the claim if:** IDs match usage 100% of the time, text recount mismatches cluster in the predicted causes, and input recount with a pinned template matches exactly.
- **Refutes it if:** IDs differ from usage, or input recounts differ with no identifiable cause.

**V2. Mid-stream abort (about half a day).** Run a small script with a raw socket close at chosen token offsets, including during prefill, on v0.32.0 and v0.33.0 of vLLM and on llama.cpp.
- **Record:** last-seen `completion_tokens` at the proxy, engine generation-token counter delta, and time until the running count returns to zero.
- **Confirms if:** last-seen usage stays within a few steps of the engine's delta and generation stops within about a second.
- **Refutes if:** generation continues to `max_tokens`, as in the v0.32.0 reports.

**V3. Cancellation under CC (about 1 day, needs a Verda CC instance).** Repeat V2 on RTX PRO 6000 CC with async scheduling on and off. Record time to GPU idle and throughput against a matched non-CC run if available. This also tests the Serialized Bridge figures on small models, where the paper offers little guidance.

**V4. Template and tokenizer portability (about half a day).** Render and tokenize the V1 corpus with the client's actual stack (Rust or JS Jinja and tokenizer) and compare it to the engine's `prompt_token_ids`. Any difference decides whether the client can compute a hold on its own.

**V5. Hold and settlement fault injection (about 2 days, once a gate exists).** Kill the client, the circuit, the gate and the CVM at each state: hold accepted, prefill, mid-stream, after cap, before retrieval. Check there is no double settlement, charge never exceeds hold, and each lost-value case matches the stated policy.

**V6. Cache off verification (about 2 hours).** Send repeated long prefixes and confirm `cached_tokens` is zero or absent and TTFT shows no bimodal cold and cached split, on the exact flags used in production.

## Claims table

Every row rests on documentation or source reading and is **untested in practice** unless noted.

| Claim | Status | Evidence |
|---|---|---|
| The enclave can count tokens using the engine's own usage | solid (untested in practice) | [vLLM serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py), [Tinfoil extractor](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/tokencount/extractor.go) |
| vLLM runs on Blackwell under GPU-CC | solid for RTX Pro 6000 and B300 per one vendor preprint; conditional for B200 CC (no direct CC test found) | [Serialized Bridge](https://arxiv.org/html/2606.23969), [issue 61918](https://github.com/vllm-project/vllm/issues/61918), [NVIDIA platforms](https://docs.nvidia.com/confidential-computing/virtual-machines/supported-platforms.md) |
| CUDA and driver are compatible on Verda CC images | conditional: v0.33.0 needs CUDA 13 and R580+; matrix rows unread | [vLLM releases](https://github.com/vllm-project/vllm/releases), [vLLM install](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/), `verda_browser_notes.md` |
| TGI is a sound choice | false: maintenance mode | [TGI README](https://raw.githubusercontent.com/huggingface/text-generation-inference/main/README.md) |
| A final `include_usage` event is enough for metering | false: disconnect skips it in vLLM (inferred from code) | [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py), [router main.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/main.go) |
| Per-chunk usage survives a stream cut as last-seen count | solid in code; no test asserts it | [extractor.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/tokencount/extractor.go), [extractor tests](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/tokencount/extractor_streaming_test.go) |
| Tokens generated but undelivered are billed exactly | false: delivery is unobservable without an acknowledgement, and the engine tail is unreported | [EHBP spec](https://raw.githubusercontent.com/tinfoilsh/encrypted-http-body-protocol/main/SPEC.md), [issue 61918](https://github.com/vllm-project/vllm/issues/61918) |
| `max_tokens` is a hard stop | solid for vLLM; weak for llama.cpp (documented small overshoot) | [sched/utils.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/core/sched/utils.py), [llama.cpp README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md) |
| Ceiling hold with change exists in current systems | false: none found; new work | [proxy.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/proxy.go), [pricing.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/pricing.go) |
| A hold can be settled safely when a stream dies | conditional: design proposed; crash loss unresolved | Inference, with [EHBP spec](https://raw.githubusercontent.com/tinfoilsh/encrypted-http-body-protocol/main/SPEC.md) |
| GPU work stops quickly after cancel on vLLM | conditional: "a few steps" on recent main; v0.32.0 reports contradict; attribution unclear; CC unmeasured | [issue 61918](https://github.com/vllm-project/vllm/issues/61918), [PR 58942](https://github.com/vllm-project/vllm/pull/58942) |
| Client can recount exactly from text alone | false | [detokenizer.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/detokenizer.py), [output_processor.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/v1/engine/output_processor.py), toy simulation |
| Client can check delivered tokens exactly given IDs | solid in principle; needs V1 | [vLLM docs](https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/), [serving.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/entrypoints/openai/chat_completion/serving.py) |
| Client can recompute input tokens exactly | conditional: pinned files, clock-free template, same Jinja behaviour; library versions can matter | [HF templates](https://huggingface.co/docs/transformers/chat_templating_writing), [PR 42131](https://github.com/huggingface/transformers/pull/42131) |
| Signed receipt binding model, tokens, price and nonce is feasible | solid in concept; only partial precedent, since NEAR does not sign counts separately and I found none signing price | [NEAR signatures](https://docs.near.ai/cloud/verification/cloud-api/response-signatures.md), [Tinfoil attested keys](https://docs.tinfoil.sh/containers/attested-keys.md) |
| EHBP proves a stream finished | false: spec says application-level completion is required | [EHBP spec](https://raw.githubusercontent.com/tinfoilsh/encrypted-http-body-protocol/main/SPEC.md) |
| Shared prefix caches leak across users | solid (measured on real providers) | [Gu et al.](https://proceedings.mlr.press/v267/gu25b.html), [InputSnatch](https://arxiv.org/html/2411.18191v2), [KVGov](https://arxiv.org/abs/2608.09225) |
| Salted caches are enough for this design | weak: they stop other tenants, need an identity anchor, and cross-request timing still links requests (my inference) | [Tinfoil caching](https://docs.tinfoil.sh/sdk/prompt-caching.md), [cache_salt.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/cache_salt.go) |
| Meta keeps no persistent KV cache and reboots regularly | solid (their statement; not independently checked) | [Meta whitepaper](https://ai.meta.com/static-resource/private-processing-technical-whitepaper) |
| Cost of Meta-style statelessness is known | unknown | No primary figure found. Quadratic prefill growth is my arithmetic |
| Engines are private by default on logs and telemetry | false for vLLM usage stats and access logs; other defaults fine; crash paths need care | [usage stats](https://docs.vllm.ai/en/latest/usage/usage_stats/), [vLLM CLI](https://docs.vllm.ai/en/latest/cli/serve/), [dump_input.py](https://raw.githubusercontent.com/vllm-project/vllm/main/vllm/logging_utils/dump_input.py) |
| Pinned weights can be attested through dm-verity | solid on Tinfoil; to be rebuilt or reused on Verda (cross-track) | [Tinfoil models](https://docs.tinfoil.sh/containers/models.md) |
| Signed price tables exist as prior art | unknown: none found | [pricing.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/pricing.go) |
| OpenAI-compatible API is a sufficient backend abstraction | weak: fine as the outer shape, but per-chunk usage, IDs and salts are engine extensions | [vLLM docs](https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/), [Ollama middleware](https://raw.githubusercontent.com/ollama/ollama/main/middleware/openai.go) |
| Tinfoil Inference can back this with client-verifiable metering | weak: identity-bound billing events and no signed counts found; Tinfoil Containers is more plausible | [events.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/billing/events.go), [usage_writer.go](https://raw.githubusercontent.com/tinfoilsh/confidential-model-router/main/manager/usage_writer.go), [attested keys](https://docs.tinfoil.sh/containers/attested-keys.md) |
