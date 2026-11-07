#!/usr/bin/env bash
# Records what the instance reports about its TEE in tree/attest/: an AMD
# SEV-SNP report from the guest's configfs-tsm, the GPU's confidential
# computing state, and NVIDIA's attestation if `nvattest` is installed.
# Then it sets the GPU ready, which CUDA needs in CC mode.
#
# Nothing here is verified by the wallet yet: the summary is what the
# instance says about itself. With REQUIRE_TEE=1 it fails unless the CPU
# is an SNP guest and the GPU is in CC mode; a rehearsal sets it to 0.

. "$(dirname "$0")/common.sh"

OUT="$TREE/attest"
mkdir -p "$OUT"
SUMMARY="$OUT/summary.txt"
: >"$SUMMARY.part"

# A line for the summary, which the wallet's console shows.
note() {
  printf '%s\n' "$*" >>"$SUMMARY.part"
  say "$*"
}

cpu_tee=0
gpu_cc=0

# The CPU: an SEV-SNP report through configfs-tsm (Linux 6.7 and later).
tsm=/sys/kernel/config/tsm/report
if [ ! -d "$tsm" ] && [ -d /sys/kernel/config ] && ! mountpoint -q /sys/kernel/config 2>/dev/null; then
  $SUDO mount -t configfs none /sys/kernel/config 2>/dev/null || true
fi
if [ -d "$tsm" ] && command -v python3 >/dev/null 2>&1; then
  entry="$tsm/counteractive-$$"
  $SUDO mkdir "$entry"
  # report_data: SHA-512 of release.json, so the report names this release.
  # Stage 2 binds the gate's TLS key here instead.
  python3 -c '
import hashlib, sys
sys.stdout.buffer.write(hashlib.sha512(open(sys.argv[1], "rb").read()).digest())
' "$TREE/release.json" | $SUDO tee "$entry/inblob" >/dev/null
  $SUDO cat "$entry/outblob" >"$OUT/snp-report.bin"
  provider="$($SUDO cat "$entry/provider" 2>/dev/null || echo unknown)"
  $SUDO cat "$entry/auxblob" >"$OUT/snp-certs.bin" 2>/dev/null || true
  $SUDO rmdir "$entry"
  if [ "$provider" = "tdx_guest" ] && [ -s "$OUT/snp-report.bin" ]; then
    cpu_tee=1
    mv "$OUT/snp-report.bin" "$OUT/tdx-quote.bin"
    # The TD quote: a 48-byte header, then the TD report body (Intel TDX
    # DCAP quote format v4/v5): TDATTRIBUTES, MRTD, the RTMRs, REPORTDATA.
    python3 -c '
import sys
quote = open(sys.argv[1], "rb").read()
version = int.from_bytes(quote[0:2], "little")
tee = int.from_bytes(quote[4:8], "little")
body = 48
attributes = int.from_bytes(quote[body + 120:body + 128], "little")
debug = "debug allowed" if attributes & 1 else "debug off"
print(f"CPU TEE: Intel TDX guest, quote v{version} (TEE type {tee:#x}), TD attributes {attributes:#x} ({debug})")
print(f"launch measurement (MRTD): {quote[body + 136:body + 184].hex()}")
' "$OUT/tdx-quote.bin" >>"$SUMMARY.part"
    note "report data: SHA-512 of release.json (the quote is tree/attest/tdx-quote.bin)"
  elif [ "$provider" = "sev_guest" ] && [ -s "$OUT/snp-report.bin" ]; then
    cpu_tee=1
    # The ATTESTATION_REPORT layout from AMD's SEV-SNP firmware ABI.
    python3 -c '
import sys
report = open(sys.argv[1], "rb").read()
version = int.from_bytes(report[0:4], "little")
policy = int.from_bytes(report[0x08:0x10], "little")
vmpl = int.from_bytes(report[0x30:0x34], "little")
debug = "debug allowed" if policy >> 19 & 1 else "debug off"
print(f"CPU TEE: AMD SEV-SNP guest, report v{version} from VMPL {vmpl}, policy {policy:#x} ({debug})")
print(f"launch measurement: {report[0x90:0xC0].hex()}")
' "$OUT/snp-report.bin" >>"$SUMMARY.part"
    note "report data: SHA-512 of release.json (the report and AMD's certificates are in tree/attest)"
  else
    note "CPU TEE: configfs-tsm answered with provider ${provider}, not an SEV-SNP report"
  fi
elif [ -e /dev/sev-guest ]; then
  note "CPU TEE: /dev/sev-guest exists but configfs-tsm doesn't; no report taken"
else
  note "CPU TEE: none found (no configfs-tsm report, no /dev/sev-guest)"
fi

# The GPU: its confidential computing state, attestation and ready state.
# The value of the first "name: value" line `nvidia-smi conf-compute $1`
# prints, which surrounds it with blank lines.
cc_value() {
  nvidia-smi conf-compute "$1" 2>/dev/null | grep ':' | head -n 1 | sed 's/.*: *//' || true
}

if command -v nvidia-smi >/dev/null 2>&1; then
  gpu="$(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader | head -n 1)"
  nvidia-smi conf-compute -q >"$OUT/gpu-cc.txt" 2>&1 || true
  cc_state="$(grep -iE 'CC (state|status)' "$OUT/gpu-cc.txt" | head -n 1 | sed 's/.*: *//' || true)"
  environment="$(cc_value -e)"
  case "$cc_state" in
    [Oo][Nn]) gpu_cc=1 ;;
  esac
  note "GPU: ${gpu}; CC ${cc_state:-unknown}, environment ${environment:-unknown}"

  if command -v nvattest >/dev/null 2>&1; then
    if nvattest attest --device gpu --verifier remote >"$OUT/nvattest.json" 2>"$OUT/nvattest.err"; then
      note "GPU attestation: nvattest with NVIDIA's remote verifier passed (tree/attest/nvattest.json)"
    else
      note "GPU attestation: nvattest failed: $(tail -n 1 "$OUT/nvattest.err")"
    fi
  else
    note "GPU attestation: not run (nvattest is not installed)"
  fi

  if [ "$gpu_cc" = 1 ]; then
    ready="$(cc_value -grs)"
    case "$ready" in
      [Rr]eady) note "GPU ready state: ready" ;;
      *)
        # CUDA refuses to run on a CC GPU until it is set ready. Stage 1
        # sets it without checking NVIDIA's attestation first.
        $SUDO nvidia-smi conf-compute -srs 1 >/dev/null
        note "GPU ready state: $(cc_value -grs) (set by this script, after ${ready:-an unknown state})"
        ;;
    esac
  fi
else
  note "GPU: no nvidia-smi"
fi

mv "$SUMMARY.part" "$SUMMARY"
if [ "${REQUIRE_TEE:-1}" = 1 ] && { [ "$cpu_tee" = 0 ] || [ "$gpu_cc" = 0 ]; }; then
  say "REQUIRE_TEE=1, but this isn't an SEV-SNP or TDX guest with a GPU in CC mode"
  exit 1
fi
