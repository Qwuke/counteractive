# Sourced by the other scripts here, which `xtask remote` runs on the
# instance with CA set to the instance's directory ($HOME/ca). In a
# rehearsal (`remote deploy --local`), CA_SRC and XTASK point at the
# laptop's workspace and xtask instead. Bash 3.2 compatible, for that
# rehearsal on macOS.

set -euo pipefail
# The services refuse directories others can read, such as run/.
umask 077

: "${CA:?CA must be the instance directory}"
# shellcheck source=/dev/null
. "$CA/ca.env"

SRC="${CA_SRC:-$CA/src}"
XTASK="${XTASK:-$SRC/target/release/xtask}"
TREE="$CA/tree"
VENV="$CA/venv"
VLLM_PID="$TREE/run/vllm.pid"
ENGINE_SOCK="$TREE/run/engine.sock"
export HF_HOME="$CA/hf"
export HF_HUB_DISABLE_TELEMETRY=1 VLLM_NO_USAGE_STATS=1 DO_NOT_TRACK=1

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

say() {
  printf '[%s] %s\n' "$(basename "$0" .sh)" "$*"
}

# Whether vLLM, started by start.sh, is running.
vllm_running() {
  [ -f "$VLLM_PID" ] && kill -0 "$(cat "$VLLM_PID")" 2>/dev/null
}

# Whether vLLM answers on its Unix socket.
vllm_healthy() {
  curl -fsS --max-time 5 --unix-socket "$ENGINE_SOCK" http://localhost/health >/dev/null 2>&1
}
