#!/usr/bin/env bash
# What runs on the instance, and what it reported about its TEE.

. "$(dirname "$0")/common.sh"

if [ -f "$TREE/attest/summary.txt" ]; then
  say "attestation summary (as the instance reports it):"
  sed 's/^/  /' "$TREE/attest/summary.txt"
fi

case "$ENGINE" in
  vllm)
    if vllm_running && vllm_healthy; then
      say "vLLM: running (pid $(cat "$VLLM_PID")), healthy"
    elif vllm_running; then
      say "vLLM: running (pid $(cat "$VLLM_PID")), not healthy yet"
    else
      say "vLLM: not running"
    fi
    ;;
  *) say "engine: fake-engine (a rehearsal)" ;;
esac

if [ -f "$TREE/up.json" ]; then
  # up.json lists each service as {"name": ..., "pid": ...}.
  tr -d ' \n' <"$TREE/up.json" | grep -oE '"name":"[^"]+","pid":[0-9]+' | while IFS= read -r entry; do
    name="$(printf '%s' "$entry" | sed -E 's/"name":"([^"]+)".*/\1/')"
    pid="$(printf '%s' "$entry" | sed -E 's/.*"pid":([0-9]+)/\1/')"
    if kill -0 "$pid" 2>/dev/null; then
      say "$name: running (pid $pid)"
    else
      say "$name: NOT running (pid $pid)"
    fi
  done
else
  say "the services aren't up"
fi

if [ -f "$TREE/tor/onion/hostname" ]; then
  say "onion service: $(cat "$TREE/tor/onion/hostname")"
fi
if command -v nvidia-smi >/dev/null 2>&1; then
  say "GPU: $(nvidia-smi --query-gpu=name,memory.used,memory.total,utilization.gpu --format=csv,noheader | head -n 1)"
fi
