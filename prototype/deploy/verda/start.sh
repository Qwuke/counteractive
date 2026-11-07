#!/usr/bin/env bash
# Starts vLLM on the engine socket (or nothing, with ENGINE=fake), waits
# for it to load the model, then starts the stack around it with `xtask
# host`. Whatever already runs is left alone.

. "$(dirname "$0")/common.sh"

mkdir -p "$TREE/run" "$TREE/logs"
chmod 700 "$TREE/run"

if [ "$ENGINE" = vllm ]; then
  if vllm_running; then
    say "vLLM is already running (pid $(cat "$VLLM_PID"))"
  else
    vllm="$VENV/bin/vllm"
    help="$("$vllm" serve --help=all 2>/dev/null || "$vllm" serve --help 2>/dev/null || true)"
    case "$help" in
      *--uds*) ;;
      *)
        say "this vLLM has no --uds; set VLLM_SPEC to a newer one and rerun setup.sh"
        exit 1
        ;;
    esac
    # Prompts must never reach a log. Say so explicitly where this vLLM
    # has the flag; the newest log no requests without
    # --enable-log-requests.
    flags=""
    case "$help" in *--disable-log-requests*) flags="$flags --disable-log-requests" ;; esac
    # On a B200 vLLM picks FlashInfer's attention, which compiles kernels
    # at startup with nvcc and ninja the image lacks; FlashAttention ships
    # built, and is what it picks on an RTX PRO 6000 anyway.
    case "$help" in *--attention-backend*) flags="$flags --attention-backend FLASH_ATTN" ;; esac
    # Access lines would time each request.
    case "$help" in *--disable-uvicorn-access-log*) flags="$flags --disable-uvicorn-access-log" ;; esac
    # Prefix caching would let one session's timing reveal another's
    # prompts. vLLM has spelt its off switch both ways.
    case "$help" in
      *--no-enable-prefix-caching*) flags="$flags --no-enable-prefix-caching" ;;
      *--disable-prefix-caching*) flags="$flags --disable-prefix-caching" ;;
      *)
        say "this vLLM has no switch to turn prefix caching off"
        exit 1
        ;;
    esac

    rm -f "$ENGINE_SOCK"
    say "starting vLLM: $MODEL_WEIGHTS as $MODEL_ID, context $MODEL_CONTEXT, on $ENGINE_SOCK"
    # Its own session, so it outlives this SSH command; HF_HUB_OFFLINE
    # because setup.sh downloaded the weights. FlashInfer's sampler would
    # compile a kernel at startup, which needs nvcc and ninja the image
    # lacks; PyTorch's sampler needs neither. vLLM's V2 model runner reads
    # host memory through UVA views that go stale on a GPU in CC mode, so
    # it samples garbage (vllm-project/vllm#57224, open in 0.33.0); the V1
    # runner copies explicitly.
    # shellcheck disable=SC2086
    HF_HUB_OFFLINE=1 VLLM_USE_FLASHINFER_SAMPLER=0 VLLM_USE_V2_MODEL_RUNNER=0 \
      setsid "$vllm" serve "$MODEL_WEIGHTS" \
      --served-model-name "$MODEL_ID" \
      --uds "$ENGINE_SOCK" \
      --max-model-len "$MODEL_CONTEXT" \
      $flags \
      >>"$TREE/logs/vllm.log" 2>&1 </dev/null &
    echo $! >"$VLLM_PID"
  fi

  say "waiting for vLLM to load the model (a few minutes the first time)"
  deadline=$((SECONDS + 900))
  until vllm_healthy; do
    if ! vllm_running; then
      say "vLLM exited; the end of its log:"
      tail -n 40 "$TREE/logs/vllm.log"
      exit 1
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      say "vLLM isn't healthy after 15 minutes; the end of its log:"
      tail -n 40 "$TREE/logs/vllm.log"
      exit 1
    fi
    sleep 5
  done
  say "vLLM is healthy"

  # Healthy isn't correct: on a CC GPU a broken data path can stream
  # well-formed tokens that spell nothing. Ask a question with one answer.
  answer="$(curl -fsS --max-time 60 --unix-socket "$ENGINE_SOCK" http://localhost/v1/chat/completions \
    -H 'content-type: application/json' \
    -d "{\"model\":\"$MODEL_ID\",\"messages\":[{\"role\":\"user\",\"content\":\"What is the capital of France? Answer in one word.\"}],\"max_tokens\":8,\"temperature\":0}" |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["choices"][0]["message"]["content"])' 2>&1 || true)"
  case "$answer" in
    *Paris*) say "vLLM answers sensibly (the capital of France: ${answer})" ;;
    *)
      say "vLLM is healthy but answers nonsense (the capital of France: ${answer@Q}); its GPU data path is broken"
      exit 1
      ;;
  esac
fi

if [ -f "$TREE/up.json" ]; then
  say "the stack is already up ($TREE/up.json)"
  exit 0
fi
# `deploy --skip-setup` pushes new source without setup.sh's builds;
# `xtask host` rebuilds the services, but not itself.
if [ -z "${CA_SRC:-}" ]; then
  cargo build -q --release --manifest-path "$SRC/Cargo.toml" -p xtask
fi
fake=""
if [ "$ENGINE" = fake ]; then
  fake="--fake-engine"
fi
# shellcheck disable=SC2086
"$XTASK" --dev-dir "$TREE" host \
  --storefront "$STOREFRONT_LISTEN" \
  --fake-stripe "$FAKE_STRIPE_LISTEN" \
  --success-url "$SUCCESS_URL" \
  $fake
