#!/usr/bin/env bash
# Prepares a fresh Ubuntu 24.04 instance once: the packages, Rust, the
# services, vLLM and the model's weights. Safe to run again.
#
# What it downloads: Ubuntu packages (tor, a C toolchain, cmake, Python's
# venv), rustup and Rust 1.94.0, the workspace's crates, vLLM and its
# dependencies from PyPI (VLLM_SPEC overrides "vllm"), and MODEL_WEIGHTS
# from Hugging Face.

. "$(dirname "$0")/common.sh"

say "Ubuntu packages"
export DEBIAN_FRONTEND=noninteractive
# A fresh instance may still be running unattended-upgrades.
apt_get() {
  $SUDO apt-get -o DPkg::Lock::Timeout=600 -q "$@"
}
apt_get update
# perl and make build the vendored OpenSSL under SQLCipher; cmake builds
# aws-lc for rustls.
apt_get install -y --no-install-recommends \
  tor build-essential pkg-config perl cmake python3-venv python3-dev curl ca-certificates
# xtask runs its own C tor with its own torrc; the system's isn't needed.
$SUDO systemctl disable --now tor.service tor@default.service >/dev/null 2>&1 || true

if ! command -v cargo >/dev/null 2>&1; then
  say "rustup and Rust 1.94.0"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y -q --profile minimal --default-toolchain 1.94.0
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

say "building xtask and the services (release)"
manifest="$SRC/Cargo.toml"
cargo build -q --release --manifest-path "$manifest" -p xtask
# The same two builds `xtask host` makes, so that one finds them done.
cargo build -q --release --manifest-path "$manifest" \
  -p gate -p storefront -p fake-engine -p fake-stripe -p wallet-cli
cargo build -q --release --manifest-path "$manifest" -p walletd

say "vLLM (${VLLM_SPEC:-vllm}) in $VENV"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
fi
"$VENV/bin/pip" install -q --upgrade pip
"$VENV/bin/pip" install -q "${VLLM_SPEC:-vllm}"
"$VENV/bin/vllm" --version

say "the weights of $MODEL_WEIGHTS"
"$VENV/bin/python" -c '
import sys
from huggingface_hub import snapshot_download
print(snapshot_download(sys.argv[1]))
' "$MODEL_WEIGHTS"
say "done"
