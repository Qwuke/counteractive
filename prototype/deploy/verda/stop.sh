#!/usr/bin/env bash
# Stops the services `xtask host` started, then vLLM. The tree, the
# weights and the venv stay, so start.sh brings everything back.

. "$(dirname "$0")/common.sh"

if [ -f "$TREE/up.json" ]; then
  "$XTASK" --dev-dir "$TREE" down
else
  say "the services aren't up"
fi

if vllm_running; then
  pid="$(cat "$VLLM_PID")"
  # vLLM leads its own process group (start.sh used setsid); its engine
  # core runs in a child.
  kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid"
  waited=0
  while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -KILL -- "-$pid" 2>/dev/null || kill -KILL "$pid"
  fi
  say "vLLM stopped"
fi
rm -f "$VLLM_PID" "$ENGINE_SOCK"
