# Spike: can a client independently reconcile token counts?

Run on 2026-11-06, 13:37 to 14:14 local (about 37 minutes). This is a validation experiment and not the PoC. The code is throwaway.

**Answer in one paragraph.** A client can reproduce `prompt_tokens` for ordinary chat messages (421/421, identical IDs). It cannot reconcile `completion_tokens` from text: re-encoding the delivered text disagrees in 242/434 requests (55.8%), and in 88/434 (20.3%) after adding 1 for EOS. Exact reconciliation needs the full ID list, which this engine returns only for non-streamed responses (0/434 mismatches). The OpenAI-style logprobs list is not exact (36.2% mismatch non-streamed, 63.4% streamed), and streamed IDs are lossy too. An aborted stream delivers no usage at all, and a client that stops reading without closing lets generation run to `max_tokens` (38/39 runs). So a gate must meter on the engine side, charge generated tokens against a pre-debited hold, and rewrite requests, because `max_tokens` can be overridden or bypassed.

All engine results below are for llama.cpp `llama-server` b12110 only. **vLLM was not run** (see Provenance).

Raw files live in the session scratchpad, which is temporary: `/private/tmp/claude-501/-Users-qwuke-Projects-counteractive/85f7ca71-591a-49ab-8311-dcdd0f25bf9b/scratchpad/spikes/metering/raw/`. Scripts are in the sibling `scripts/` directory. Evidence names in this note are relative to that `raw/` directory.

## Provenance

| Item | Canonical source and how established | Publisher | Exact version | Licence | Integrity check | Registry, advisories, install scripts |
|---|---|---|---|---|---|---|
| `llama-server` binary | GitHub `ggml-org/llama.cpp`, an organisation repo with 133k stars, homepage llama.app. Stable release `v0.9.0` names its nightly build `b12110`. | ggml-org. Asset uploaded by `github-actions[bot]`. | `b12110`, commit `cd959bae99ff4bed22bef2604f45241c09bbb8c8` (GitHub commit verification: valid), macOS arm64 tarball | MIT | SHA-256 `4f5b8de12e0b9516658ca6a5764c6ffb3d7c659c3dd538556b1413c7b4050662` matches the GitHub API digest and the SLSA statement subject. **The Sigstore signature was not cryptographically verified**: `gh attestation verify` failed with `unsupported tlog public key type: PKIX_ED25519` because gh 2.49.1 is too old. I read the statement only: builder is `.github/workflows/release-publish.yml@refs/heads/master`, same commit. | 13 repo advisories reviewed. Those with a stated fix predate this build. The newest, CVE-2026-34159, is in the RPC backend, lists no patched version but an affected range of `<= b7991` (this build is b12110), and the RPC backend was not used. Tarball is binaries only, no install script. Binaries are ad hoc signed and not notarised. Bound to 127.0.0.1, `--offline`, `--device none`. |
| GGUF model | `unsloth/SmolLM2-135M-Instruct-GGUF`, card names base model `HuggingFaceTB/SmolLM2-135M-Instruct` | Unsloth (HF organisation) | commit `9e6855bc4be717fca1ef21360a1db4b29d5c559a`, file `SmolLM2-135M-Instruct-Q4_K_M.gguf`, 105 MB | Apache-2.0 | SHA-256 `ed5fa30c487b282ec156c29062f1222e5c20875a944ac98289dbd242e947f747` matches the HF LFS hash. Not reproduced from upstream weights. | Slightly above the "tens of MB" asked for. ggml-org publishes only the base SmolLM2 (no chat template), so I used this Instruct build. Q2_K at 88 MB was available and was not used, to keep a typical quantisation. |
| HF tokenizer files | `HuggingFaceTB/SmolLM2-135M-Instruct` | Hugging Face TB (model author) | commit `12fd25f77366fa6b3b4b768ec3050bf629380bac` | Apache-2.0 | `tokenizer.json` SHA-256 `9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c`. Git blob IDs of all 5 files match the HF API. | Data files only. GGUF-embedded template is byte-identical to the HF template (checked). |
| `tokenizers` (PyPI) | `huggingface/tokenizers` | Hugging Face | 0.23.3 | Apache-2.0 | `--require-hashes`, wheel `68649e97...a716`, matches PEP 740 attestation subject | Trusted Publisher: `huggingface/tokenizers`, `python-release.yml`. OSV: none. Wheel only, so no build script ran. `--no-deps`: its `huggingface-hub` dependency was deliberately not installed and is not needed for `from_file`. |
| `jinja2` (PyPI) | `pallets/jinja` | Pallets | 3.1.6 | BSD | `--require-hashes`, `85ece445...2f67`, matches attestation | Trusted Publisher: `pallets/jinja`, `publish.yaml`. OSV: none. Wheel only. |
| `markupsafe` (PyPI) | `pallets/markupsafe` | Pallets | 3.0.4 | BSD-3-Clause | `--require-hashes`, `73e77980...2a72`, matches attestation | Trusted Publisher: `pallets/markupsafe`, `publish.yaml`. OSV: none. Wheel only. |

PyPI attestations were read but their signatures were not verified. PyPI project pages returned a client challenge for three of the packages, so I used the Trusted Publisher identity as the owner match in place of the maintainer list.

**Not installed:**

- `transformers`: 29 packages when resolved, and not needed. I render the template with jinja2 using the same sandbox and `trim_blocks`/`lstrip_blocks` settings it uses.
- **vLLM 0.33.0, skipped.** PyPI has only `manylinux` wheels plus an sdist, and its docs say "there are no pre-built Apple silicon CPU wheels" and macOS "must build from source". That is the trouble the brief said to skip for. I read its source at tag `v0.33.0` as data only:
  - `return_token_ids` is documented as giving delta `token_ids` per streamed chunk.
  - `max_completion_tokens` takes precedence over `max_tokens`.
  - Chat routes use a `with_cancellation` decorator.

  **None of this is tested.**

**Isolation incidents, all mine:**

- `gh attestation verify` wrote a Sigstore TUF cache to `~/.cache/gh/.sigstore`, outside the spike directory, despite the redirected `GH_CONFIG_DIR`. I checked that every entry was created at 13:43 by that run, then removed only that subtree. The rest of `~/.cache/gh` is your older cache and was left alone, though the parent directory's modification time changed.
- Two shell typos created empty files at `/tmp/null` and `/tmp/nonexistent_ignore`. Both are gone.
- I ran `gh auth status` once early on. It only reported login state. All GitHub API calls were anonymous `curl`, and no credentials were used.

## Method

- Model server: `llama-server -ngl 0 --device none -c 8192 -np 1 --metrics --slots --offline`, on an Apple M5 Max CPU with 8 threads (about 480 tok/s) or 1 thread (about 245 tok/s).
- Independent count: `tokenizers.Tokenizer.from_file(tokenizer.json)` with `add_special_tokens=False`, over the HF `chat_template` rendered in a jinja2 sandbox.
- Corpus: 436 synthetic, deterministic requests in 15 categories (`scripts/corpus.py`). Temperature was one of 0, 0.3, 0.7, 1.0, 1.4, with fixed seeds. 434 were accepted. Two were rejected with HTTP 400 for lone surrogates in JSON, and nothing ran for them.
- Each request was run through several paths: chat non-streamed with logprobs (436), chat with `verbose`+`return_tokens` (434), chat streamed with `timings_per_token` (434), chat streamed with logprobs (146), native `/completion` non-streamed (434 + 144) and streamed (434).
- The root-cause split sums exactly per request (asserted in `scripts/analyze3.py`).

## Test 1: server count vs independent recount

### prompt_tokens

| category | n | count mismatch | IDs identical to `/tokenize` |
|---|---|---|---|
| ascii | 25 | 0 | 25 |
| latin_combining_misc | 25 | 0 | 25 |
| emoji | 36 | 0 | 36 |
| cjk | 30 | 0 | 30 |
| rtl | 24 | 0 | 24 |
| code | 30 | 0 | 30 |
| long_whitespace | 54 | 0 | 54 |
| special_token_like | 52 | 0 | 52 |
| stop_sequence | 40 | 0 | 40 |
| max_tokens_trunc | 30 | 0 | 30 |
| multi_turn | 30 | 0 | 30 |
| assistant_prefill | 10 | 10 | 0 |
| edge_cases | 18 | 3 | 15 |
| utf8_partial_forced | 20 | 0 | 20 |
| special_output_forced | 10 | 0 | 10 |
| **total** | **434** | **13 (3.0%)** | **421** |

All 13 are request-shape differences rather than tokenizer differences:

- 10 are assistant prefill. The server continues the final assistant message, while the HF template closes it (+6 tokens).
- 2 are typed content parts. The server flattens them, and this template cannot render them.
- 1 is the `developer` role. The server renames it to system, so the default system prompt is not injected (+22 tokens).

Special-token strings in user content, such as `<|im_end|>`, parse as real control tokens on both sides, so the counts agree. The side effect is that user content can forge role boundaries. That is outside this spike but worth a separate check.

### completion_tokens vs re-encoding the delivered text

| category | n | mismatch | delta min..max | reported | recounted |
|---|---|---|---|---|---|
| ascii | 25 | 11 (44.0%) | 0..1 | 928 | 917 |
| latin_combining_misc | 25 | 9 (36.0%) | 0..1 | 1136 | 1127 |
| emoji | 36 | 29 (80.6%) | 0..2 | 1787 | 1752 |
| cjk | 30 | 18 (60.0%) | 0..2 | 1799 | 1780 |
| rtl | 24 | 15 (62.5%) | 0..4 | 1261 | 1242 |
| code | 30 | 10 (33.3%) | 0..2 | 1360 | 1349 |
| long_whitespace | 54 | 24 (44.4%) | 0..1 | 2778 | 2754 |
| special_token_like | 52 | 32 (61.5%) | 0..1 | 1455 | 1423 |
| stop_sequence | 40 | 27 (67.5%) | 0..2 | 869 | 841 |
| max_tokens_trunc | 30 | 12 (40.0%) | 0..2 | 435 | 422 |
| multi_turn | 30 | 26 (86.7%) | 0..2 | 893 | 865 |
| assistant_prefill | 10 | 9 (90.0%) | -3..0 | 530 | 544 |
| edge_cases | 18 | 10 (55.6%) | 0..1 | 839 | 829 |
| utf8_partial_forced | 20 | 5 (25.0%) | 0..16 | 309 | 287 |
| special_output_forced | 10 | 5 (50.0%) | -2..1 | 217 | 215 |
| **total** | **434** | **242 (55.8%)** | | **16,596** | **16,347 (1.5% low)** |

Adding 1 when `finish_reason=stop` and no `stop` parameter was set leaves 88/434 (20.3%). Mismatch after that adjustment does not follow temperature in any clear way (15.8% to 23.7% across the 5 bands, n=66 to 131 each).

### Root causes (from a separate run with the true ID list, 434 requests)

| root cause | requests | sole cause | net tokens |
|---|---|---|---|
| EOS is counted but has no text | 164 | 159 | +164 |
| Partial UTF-8 tail at the limit is withheld from chat text | 44 | 29 | +44 |
| Non-canonical sequence: decode then encode changes the count | 26 | 9 | +35 |
| Stop string and tokens past it are trimmed from text | 16 | 15 | +16 |
| Prefill is echoed inside `content` | 10 | 5 | -21 |
| No difference | 196 | | 0 |

The same run gave these rates by category, so the failure pattern is visible per input type:

| category | n | mismatch | EOS | stop trim | partial UTF-8 | non-canonical | prefill echo |
|---|---|---|---|---|---|---|---|
| ascii | 25 | 11 | 11 | 0 | 0 | 0 | 0 |
| latin_combining_misc | 25 | 9 | 6 | 0 | 4 | 2 | 0 |
| emoji | 36 | 27 | 15 | 0 | 11 | 6 | 0 |
| cjk | 30 | 18 | 8 | 0 | 10 | 1 | 0 |
| rtl | 24 | 15 | 14 | 0 | 2 | 3 | 0 |
| code | 30 | 11 | 11 | 0 | 0 | 0 | 0 |
| long_whitespace | 54 | 20 | 19 | 0 | 0 | 1 | 0 |
| special_token_like | 52 | 32 | 31 | 0 | 0 | 1 | 0 |
| stop_sequence | 40 | 27 | 10 | 16 | 0 | 2 | 0 |
| max_tokens_trunc | 30 | 12 | 0 | 0 | 9 | 4 | 0 |
| multi_turn | 30 | 23 | 19 | 0 | 4 | 2 | 0 |
| assistant_prefill | 10 | 9 | 4 | 0 | 1 | 1 | 10 |
| edge_cases | 18 | 11 | 11 | 0 | 0 | 0 | 0 |
| utf8_partial_forced | 20 | 5 | 1 | 0 | 3 | 2 | 0 |
| special_output_forced | 10 | 5 | 4 | 0 | 0 | 1 | 0 |

A request can have more than one cause. Mismatch here is from the `verbose` run, so it differs slightly from the table above (different samples). It totals 235, while 238 requests have at least one cause. The 3-request gap is presumably causes that cancel out (prefill echo is negative, the others positive), but I did not check those 3 individually.

### Does decoding and re-encoding give the same count?

**No.**

- On real output, `encode(decode(ids))` gives different IDs in 61/434 requests (14.1%) and a different count in 26/434 (6.0%).
- When it differed, the model usually used more tokens than the canonical form (22 of 26), but sometimes fewer (4 of 26). So re-encoding is not a safe lower bound.
- Prefixes of real outputs, ignoring specials: count differs in 199/3,594 (5.5%).
- Random vocabulary IDs: count differs in 1,822/5,000 (36.4%).
- Forced special tokens (bias +100, 7 cases): the text was empty and re-encodes to 0, while 6 tokens were billed.

## Test 2: exact reconciliation with returned token IDs

| method | path | n | mismatch | max abs delta |
|---|---|---|---|---|
| A | Re-encode text, chat non-streamed | 434 | 242 (55.8%) | 16 |
| B | OpenAI `logprobs` ID list, chat non-streamed | 434 | 157 (36.2%) | 86 |
| C | OpenAI `logprobs` ID list, chat streamed | 145 | 92 (63.4%) | 72 |
| D | Count of content chunks, chat streamed | 434 | 284 (65.4%) | 86 |
| **E** | **`verbose:true` + `return_tokens:true`, `__verbose.tokens`, chat non-streamed** | **434** | **0 (0.0%)** | **0** |
| **F** | **`/completion` `return_tokens`, non-streamed** | **434** | **0 (0.0%)** | **0** |
| G | `/completion` `return_tokens`, streamed | 434 | 136 (31.3%) | 86 |

Mismatch by category (mismatches/n):

| category | A | B | C | D | E | F | G |
|---|---|---|---|---|---|---|---|
| ascii | 11/25 | 0/25 | 5/9 | 11/25 | 0/25 | 0/25 | 0/25 |
| latin_combining_misc | 9/25 | 13/25 | 4/8 | 16/25 | 0/25 | 0/25 | 13/25 |
| emoji | 29/36 | 30/36 | 10/12 | 32/36 | 0/36 | 0/36 | 30/36 |
| cjk | 18/30 | 28/30 | 10/10 | 29/30 | 0/30 | 0/30 | 27/30 |
| rtl | 15/24 | 13/24 | 6/8 | 22/24 | 0/24 | 0/24 | 13/24 |
| code | 10/30 | 0/30 | 2/10 | 11/30 | 0/30 | 0/30 | 0/30 |
| long_whitespace | 24/54 | 1/54 | 7/18 | 21/54 | 0/54 | 0/54 | 3/54 |
| special_token_like | 32/52 | 1/52 | 9/17 | 32/52 | 0/52 | 0/52 | 1/52 |
| stop_sequence | 27/40 | 24/40 | 12/14 | 28/40 | 0/40 | 0/40 | 0/40 |
| max_tokens_trunc | 12/30 | 20/30 | 10/10 | 20/30 | 0/30 | 0/30 | 20/30 |
| multi_turn | 26/30 | 12/30 | 8/10 | 27/30 | 0/30 | 0/30 | 14/30 |
| assistant_prefill | 9/10 | 4/10 | 3/3 | 8/10 | 0/10 | 0/10 | 4/10 |
| edge_cases | 10/18 | 1/18 | 3/6 | 12/18 | 0/18 | 0/18 | 1/18 |
| utf8_partial_forced | 5/20 | 10/20 | 1/6 | 11/20 | 0/20 | 0/20 | 10/20 |
| special_output_forced | 5/10 | 0/10 | 2/4 | 4/10 | 0/10 | 0/10 | 0/10 |

Why the others fail:

- **Merged multi-byte characters.** A character spanning several tokens appears as one logprobs entry carrying only the last ID. Example: 128 tokens of a repeated star emoji gave 42 logprobs entries, and 86 IDs were lost. The streamed native `tokens` field does the same: `[249]` while `tokens_predicted` jumps by 3. The ID list is therefore wrong for CJK, emoji and other non-ASCII text.
- **EOS.** It is in the non-streamed logprobs list but missing from the streamed one.
- **Stop strings.** The non-streamed logprobs list is cut back. `__verbose.tokens` keeps everything.

**Exact IDs while streaming: not available on this build.** `verbose` on a stream returns `tokens: []` in the final chunk. An exact **running count** is available: `timings_per_token:true` gives `predicted_n` on every chunk, which was monotonic and never above final usage in 434/434. The native path gives `tokens_predicted` the same way. It is a lower bound until the final chunk, because EOS and withheld tails come last (last content chunk vs usage differs in 224/434 chat, 49/434 native).

**What E and F do and do not show.** They show that the ID list agrees with the server's own counter, and `timings.predicted_n` agrees with usage (434/434). That is internal consistency. It is not independent proof of what the engine sampled. A client can partly cross-check `decode(ids)` against the delivered text: 357 equal, 43 partial UTF-8 tail, 24 stop-word extension, 2 whitespace only, 8 other (the sampled ones were prefill echoes). Non-streamed responses are also of limited use for a streaming product.

## Test 3: streaming abort

Prompt about 42 tokens, `ignore_eos:true`. The client closed once the running counter passed the target. "Generated" is the `/metrics` `tokens_predicted_total` delta for that run, on a single-slot server. On the debug server, the slot release log (`n_tokens - prompt + 1`) agreed in 68/68 runs. Bold rows are the requested 25/50/75% aborts.

| setup | client behaviour | N | stop at | n | read (median) | generated (median) | generated minus read | close to idle (median ms) | usage received |
|---|---|---|---|---|---|---|---|---|---|
| 8 threads | control, reads all | 600 | 100% | 5 | 600 | 600 | 0 | 1.5 | 5/5 |
| **8 threads** | **close (FIN)** | 600 | **25%** | 8 | 150 | 152 | **2** | 5.5 | 0/8 |
| **8 threads** | **close (FIN)** | 600 | **50%** | 8 | 300 | 302 | **2** | 5.3 | 0/8 |
| **8 threads** | **close (FIN)** | 600 | **75%** | 8 | 450 | 452 | **2** | 5.9 | 0/8 |
| 8 threads | reset (RST) | 600 | 25% | 5 | 150 | 152 | 2 | 5.0 | 0/5 |
| 8 threads | reset (RST) | 600 | 50% | 5 | 300 | 302 | 2 | 5.5 | 0/5 |
| 8 threads | reset (RST) | 600 | 75% | 5 | 450 | 452 | 2 | 5.6 | 0/5 |
| 8 threads | stops reading 3 s, then closes | 600 | 25% | 5 | 150 | 600 | 450 | 1.2 | 0/5 |
| 8 threads | stops reading 3 s, then closes | 600 | 50% | 5 | 300 | 600 | 133..300 | 0.9 | 0/5 |
| 8 threads | stops reading 3 s, then closes | 600 | 75% | 5 | 450 | 600 | 150 | 1.4 | 0/5 |
| 8 threads | stops reading 10 s, then closes | 3000 | 25% | 3 | 750 | 3000 | 2250 | 1.9 | 0/3 |
| 8 threads | stops reading 10 s, then closes | 3000 | 50% | 3 | 1500 | 3000 | 1500 | 2.1 | 0/3 |
| 8 threads | stops reading 10 s, then closes | 3000 | 75% | 3 | 2250 | 3000 | 750 | 2.7 | 0/3 |
| 1 thread | control | 300 | 100% | 3 | 300 | 300 | 0 | 1.5 | 3/3 |
| **1 thread** | **close (FIN)** | 300 | **25%** | 5 | 75 | 77 | **2** | 8.6 | 0/5 |
| **1 thread** | **close (FIN)** | 300 | **50%** | 5 | 150 | 152 | **2** | 9.6 | 0/5 |
| **1 thread** | **close (FIN)** | 300 | **75%** | 5 | 225 | 227 | **2** | 11.0 | 0/5 |
| 1 thread | stops reading 6 s, then closes | 300 | 25% | 5 | 75 | 300 | 225 | 2.0 | 0/5 |
| 1 thread | stops reading 6 s, then closes | 300 | 50% | 5 | 150 | 300 | 150 | 0.7 | 0/5 |
| 1 thread | stops reading 6 s, then closes | 300 | 75% | 5 | 225 | 300 | 75 | 2.0 | 0/5 |
| 1 thread + 300 ms RTT relay | close (FIN) | 800 | 25% | 5 | 200 | 272 | 71..73 | 162 | 0/5 |
| 1 thread + 300 ms RTT relay | close (FIN) | 800 | 50% | 5 | 400 | 463 | 62..64 | 161 | 0/5 |
| 1 thread + 300 ms RTT relay | close (FIN) | 800 | 75% | 5 | 600 | 655 | 54..56 | 164 | 0/5 |

**What the server reports or logs.**

- The client receives no usage on any abort (0 of 108 aborted runs in the table above; the 8 control runs all received it).
- The log shows `W srv stop: cancel task, id_task = N`, then `I slot release: ... stop processing: n_tokens = X`.
- Tasks cancelled mid-generation log no `print_timing` (0 of 86 cancels, across the two logs that have per-run attribution).
- Stalled runs that reached `max_tokens` logged `print_timing` and released normally. In 32 of those 38 no cancel was ever logged, and in the other 6 it came after generation had finished. So the log can look like a clean completion for a client that received only part of the output.
- `/metrics` does count the cancelled tokens, but only as a server-wide counter.
- `/slots` `n_decoded` was 0 on an idle slot, so it cannot be read after the fact. No per-request record of generated tokens survives except the log line.

**Does it keep generating after disconnect?**

- Fast reader that closes: yes, but briefly. It ran exactly 2 tokens past what was read in 54/54 loopback runs, and went idle in about 5 to 11 ms.
- Over the relay: 54 to 73 tokens and about 160 ms, which is about token rate × RTT. The relay is a local delay only. It is **not Tor** and models no Tor cell buffering.
- Reader that stops reading without closing: **generation is not back-pressured**. 38/39 runs reached `max_tokens`, including 3000 tokens while only 750 had been read. The one exception (`stall:3`, 50%, rep 2) reached 433/600. It was not back-pressure: the log shows a cancel with no timing, so it was still generating when the client closed at the end of the 3 s stall. Its `/metrics` generation time was 3.86 s, against about 1.3 s for the other four runs in that cell, so something slowed it by over 2 s. **I did not establish what.** My other jobs were running on the machine in that period, which may or may not be related. Results queue in the server, so a stalled connection also costs server memory (not measured).

**Abort before the first token** (n=10, 4,534-token prompt, close at 5 or 20 ms): the client got nothing. The server still evaluated 2048 prompt tokens in 9 runs and 4096 in 1, taking about 1.1 s (median) before going idle. Cancellation appears to take effect at a prompt batch boundary, though that is inferred from the numbers and not read from the code.

### What a metering gate would have to do

The first five rules follow from the results above. Rule 6 is an untested design suggestion.

1. **Admit by rewriting.** Allowlist endpoints and fields. Require `max_tokens`, strip the aliases, reject array prompts, and clamp to `min(requested, policy, n_ctx - prompt)` (Test 4).
2. **Debit the hold before inference.** Hold = pre-flight prompt count + clamped `max_tokens`. Charge the prompt at admission, because a pre-first-token abort still does the work and reports nothing.
3. **Meter on the engine side.** Read the engine's running counter, not chunks (method D is 65% wrong) and not text.
4. **On disconnect, close upstream at once and take the count from the engine.** Usable sources found here: a `/metrics` delta (single slot only) or the slot release log line. "Last `predicted_n` + 2" was exact 54/54 on loopback, but it depends on engine timing and must not be relied on without a margin.
5. **Charge generated tokens and refund the rest of the hold.** Billing only delivered tokens loses up to 75% of the run to a stalled reader (2250 of 3000). Charging the whole hold overcharges by 448 of 600 tokens on a clean 25% abort. Charging generated means the client pays for tokens it never saw, up to the hold, and it can only check them if the gate later reveals them (Test 5).
6. **Stop stalled readers at the gate.** TCP back-pressure will not stop the engine, so the gate needs an application-level ack or a send-queue watermark that triggers a cancel.

## Test 4: ceiling and hold

### Does generation exceed max_tokens?

For `max_tokens ≥ 1`, no. **0 over in 504 dedicated runs + 2,459 corpus completions.** The sweep covered `max_tokens` 1 to 512, 3 prompts, with and without `ignore_eos`, streamed and not, plus 336 runs forcing partial UTF-8 fragments (bias +100). The server README warns that `n_predict` "may exceed the set limit slightly if the last token is a partial multibyte character". I could not reproduce that in 336 attempts, which does not show it cannot happen.

### Where max_tokens does not do what a gate would assume

| request | result | consequence |
|---|---|---|
| `max_tokens=0` (n=2) | `completion_tokens: 1`, `finish_reason: length` | Over by 1 as reported. The README says no tokens are generated at 0. |
| `max_tokens=10, n_predict=50` | 50 tokens | `n_predict` wins |
| `max_tokens=10, max_completion_tokens=50` | 50 tokens | `max_completion_tokens` wins |
| `n_predict=50` alone | 50 tokens | Gate sees no limit field it knows |
| omitted, `null`, `-1`, or `1e9` (one run each) | 8154 tokens, about 40 s, until `n_ctx` is full | Unbounded up to the context size |
| `/v1/completions` with 3 prompts, `max_tokens=10` (n=2) | 30 completion tokens. **`prompt_tokens` reports only the first prompt: 4 vs true 12, and 1 vs true 3** | Ceiling multiplied, and usage under-reports |
| `/completion` with 3 prompts | 3 results of 10 | Same multiplication |
| `/v1/messages`, `/v1/responses` | Work, with their own field names and usage shapes | Extra endpoints to block |
| server `-n 16` | Client `max_tokens=100` still gets 100 (16 only when unset or `-1`) | `-n` is a default, not a ceiling |
| `n=3` or `n_cmpl=3` with `-np 1` | HTTP 400 | Not tested with more slots |
| `t_max_predict_ms=50` | Stops at 75 tokens | A usable time limit, also client-settable |
| `max_tokens="10"` / `10.7` | HTTP 400 / 10 tokens | Rejected / truncated |
| prompt larger than `n_ctx` | HTTP 400 with `n_prompt_tokens: 9031` | Clean failure with the count |
| prompt 7031 + `max_tokens=4000`, `n_ctx` 8192 | 1161 tokens, `finish_reason: length` | Hold over-reserves safely |

### Is the hold computable before inference?

**Yes.** `/apply-template` then `/tokenize` matched `usage.prompt_tokens` in 434/434, including the prefill, typed and `developer` cases that the client-side recount missed. The median lookup was 0.89 ms (p95 1.34 ms) on loopback with a short queue.

- Hold ≥ actual in 434/434. Sum of holds 54,079 tokens vs 43,589 used (19% unused). The median unused amount was 0 because 241/434 runs ended exactly at the limit.
- The whole body must be forwarded: `add_generation_prompt:false` gives 30 tokens where a gate passing messages alone would compute 34. `tools` and `response_format` did not change the prompt here, but this template has no tool support, so that says little about other models.
- Cached prompts: `prompt_tokens` stayed 191 while 190 were cached and 1 was evaluated (`prompt_tokens_details.cached_tokens` reports this). Whether to charge cached tokens is a policy choice.
- Same seed twice: 58/60 identical text, 59/60 identical count. Re-execution is not a reliable way to verify a count. Cause not investigated.

## Test 5: hash-chain receipt

Sketch in `scripts/t5_chain.py`, standard library only. `h0 = SHA-256(tag || nonce || SHA-256(prompt IDs))`, `h_i = SHA-256(h_{i-1} || u32 index || u32 token_id)`, receipt `{count, head}`. **The server side is simulated**, using real ID lists from method E. No engine here emits a chain.

| check | result |
|---|---|
| Honest verification, count equals usage | 434/434 (16,842 tokens) |
| Drop / duplicate / change a token, wrong nonce, inflate or deflate count | all detected, 1961/1961 each |
| Swap two tokens | 1850/1850 |
| Abort: client verifies delivered prefix against pinned per-chunk head | 450/450 |
| Abort: undelivered suffix revealed later and replayed from the pinned head | 450/450 |
| Forged suffix (one extra token) | detected 450/450 |

| overhead | value |
|---|---|
| SHA-256 per token, Python | 233 ns, against a 57 ns loop baseline (0.011% of the 2.04 ms/token on this CPU). An earlier run, since overwritten, gave 269 ns. |
| BLAKE2b per token, Python | 251 ns |
| SHA-256 + hex + JSON per token | 1.14 µs |
| Wire, final receipt only | 90 bytes per response |
| Wire, per-token | 21 bytes for ID and index, 92 bytes with a head, against 604 bytes per SSE event here (inflated by the long model path in each chunk) |

**What it does not do:**

- It proves consistency between what was sent and what was claimed. It does not prove the count is honest. A dishonest server can pick a longer ID sequence for the same text. Splitting 195 real outputs into per-character tokens took 9,619 tokens to 31,441 (3.27×), with decoded text identical in 195/195, and any chain over those IDs verifies (`t5b_inflation.json`). Catching that needs an attested engine binding, which this spike did not test.
- A head alone says nothing about how many steps produced it, so after an abort a claimed count is unverifiable until the undelivered IDs are revealed.
- Streaming a real chain on this engine needs a patch or sidecar, because exact IDs are not exposed while streaming (Test 2). vLLM's `return_token_ids` may avoid that, but this is untested.

## Limitations

**What this proves.** The behaviour of one llama.cpp build with one model on loopback.

**What it does not prove.** Anything about vLLM or about the GPU confidential VM.

**Engine-specific (llama.cpp b12110).** All field names and paths (`verbose`, `return_tokens`, `timings_per_token`, `/apply-template`, `/tokenize`, `/slots`, `/metrics`). Merged logprobs, parameter precedence, the array-prompt usage bug, `max_tokens=0` behaviour, the `-n` semantics, the 2-token overrun, the absence of back-pressure, and the batch-boundary cancel. All of this needs re-testing on vLLM before it informs a design.

**Model-specific (SmolLM2-135M, GPT-2-style BPE, ChatML).**

- Every rate by category and the non-canonical rate. This small model handles non-English text badly, so its garbled output may push the multi-byte rates up or down relative to a production model. I do not know which way.
- A SentencePiece or Tiktoken-style tokenizer may behave differently on decode and re-encode. Only this tokenizer was tested.

**Likely general by reasoning, not by test.** EOS and stop-string trimming break text recounts on any engine. Overrun scales with token rate × path latency. The chain's properties do not depend on the engine.

**Laptop vs target.**

| aspect | here | target |
|---|---|---|
| Hardware | M5 Max CPU, about 245 to 480 tok/s | RTX PRO 6000 or B200 in a confidential VM, not tested |
| Engine | llama.cpp, `-np 1`, one request at a time | Likely vLLM with continuous batching, which may change cancel timing and attribution |
| Network | Loopback, one simulated 300 ms relay | TLS and Tor streams, real buffering not modelled |
| Metering split | `/metrics` delta valid because of one slot | Many concurrent requests, so a per-request source is required |
| Context | 8192 | Larger |

**Measurement caveats.**

- The `/slots` poller ran every 2 ms or so and goes through the server's queue, so it adds some load.
- `-lv 4` gave no per-token lines, and I did not run `-lv 5`. Ground truth is the `/metrics` delta and release log, which agreed in 68/68.
- Other jobs of mine ran on the same machine during some abort trials. For example, the hash-chain script finished at 13:59:02 and the 8-thread abort run at 14:00:20. So the Test 5 timings are noisy, and the two runs may have disturbed each other.
- The first ceiling script crashed on a list response from `/completion`, and the remaining probes ran as part 2. `t4_output_part1.txt` holds the part 1 alias rows, truncated at 420 characters per line but with usage visible on the rows cited here.
- Sample sizes exceeded the 200 requests and the 5 aborts per fraction in the brief. The exceptions are the 10 s stall rows at n=3 each, but the same behaviour appears in the n=5 stall rows. The multi-prompt and `max_tokens=0` findings rest on 2 requests each, and the other alias and limit probes in Test 4 on 1 each.

## Claims

| claim | status | evidence file |
|---|---|---|
| HF tokenizer + template reproduce `prompt_tokens` for ordinary chat messages | confirmed by test (421/421, IDs identical) | `t12_analysis.txt`, `t12_records.jsonl` |
| That holds for every accepted request shape | refuted by test (13/434: prefill, typed parts, `developer`) | `t12_analysis.txt` |
| A client can reconcile `completion_tokens` by re-encoding delivered text | refuted by test (242/434; 88/434 after EOS +1) | `t12_analysis.txt`, `t12_results.json` |
| Decode then re-encode preserves the count | refuted by test (26/434 count, 61/434 IDs) | `t3_analysis.txt`, `t1_rootcause_chat.json` |
| OpenAI `logprobs` gives an exact ID list | refuted by test (157/434 non-streamed, 92/145 streamed) | `t12_analysis.txt` |
| A full ID list gives exact reconciliation, non-streamed, on this engine | confirmed by test (0/434 for E and F); internal consistency only | `t2b_records.jsonl`, `t12_analysis.txt` |
| Exact IDs are available while streaming on this engine | refuted by test (136/434; final chunk `tokens: []`) | `t12_analysis.txt`, `t4_output_part2.txt` |
| An exact running count is available while streaming | confirmed by test (monotonic and ≤ usage in 434/434; lower bound until final chunk) | `t3_analysis.txt`, `t2b_records.jsonl` |
| An aborted stream reports usage to the client | refuted by test (0 of 108 aborted runs) | `t3_8thread.jsonl`, `t3_1thread.jsonl`, `t3_delay300ms.jsonl` |
| A closed connection stops generation quickly on loopback | confirmed by test (+2 tokens in 54/54, about 5 to 11 ms) | `t3_analysis.txt`, `abort_log_excerpt.txt` |
| Post-abort overrun scales with path latency | confirmed by test, simulated relay only, not Tor (54 to 73 tokens at 300 ms RTT) | `t3_delay300ms.jsonl` |
| TCP back-pressure from a stalled reader limits generation | refuted by test (38/39 reached `max_tokens`; the other was stopped by the client's close after an unexplained slowdown) | `t3_8thread.jsonl`, `t3_1thread.jsonl` |
| Aborting before the first token avoids prompt cost | refuted by test (2048 or 4096 of 4534 processed, n=10) | `t3b_prefirst.jsonl` |
| Generation never exceeds `max_tokens` when it is ≥ 1 | confirmed by test (0 over, 504 + 2,459) | `t4_sweep_summary.json`, `t12_analysis.txt` |
| README warning of overrun on partial multibyte tokens | inconclusive (not reproduced in 336 forced runs) | `t4_sweep.jsonl` |
| `max_tokens=0` generates nothing | refuted by test as reported by usage (`completion_tokens: 1`, n=2) | `t4_output_part1.txt`, `t4_output_part2.txt` |
| `max_tokens` alone is a safe gate ceiling | refuted by test (aliases, omission, array prompts, `-n` not a cap) | `t4_output_part1.txt`, `t4_output_part2.txt`, `t4c_server_cap.json` |
| `usage` is trustworthy for array prompts | refuted by test (`prompt_tokens` 4 vs true 12, and 1 vs true 3; 2 requests) | `t4_output_part2.txt`, `final_checks.txt` |
| The hold is computable before inference via server endpoints | confirmed by test (434/434 exact, median 0.89 ms) | `t12_results.json`, `final_checks.txt` |
| A hash chain lets a client verify the count from what it received | confirmed by test, server side simulated (434/434; all tampering detected) | `t5_results.json` |
| A hash chain proves the count is honest | refuted by test (3.27× inflation with identical text passes) | `t5b_inflation.json` |
| Chain overhead is negligible next to generation | confirmed by test on this CPU only (0.011%; 90-byte final receipt) | `t5_results.json` |
| Same-seed re-execution can verify a count | refuted by test (59/60 same count, 58/60 same text; cause unknown) | `t4_determinism.json` |
| Any of the engine results apply to vLLM | inconclusive (not run; no Apple Silicon wheel; source read only) | `pypi_vllm.json`, `vllm_protocol_v0.33.0.py`, `vllm_api_utils_v0.33.0.py` |
