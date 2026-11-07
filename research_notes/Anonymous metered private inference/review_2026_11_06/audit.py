#!/usr/bin/env python3
"""Recalculate selected report numbers from preserved experiment evidence.

Standard library only. No network, model execution, or third-party code.
This checks recorded output; it does not reproduce the original experiments.
"""
import csv
import hashlib
import io
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parent
provenance = json.loads((root / "provenance.json").read_text())
for name, metadata in provenance["files"].items():
    actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
    if actual != metadata["sha256"]:
        raise SystemExit(f"Evidence checksum mismatch: {name}")

records = json.loads((root / "metering_projection.json").read_text())
accepted = [r for r in records if r["status"] == 200]
summary = json.loads((root / "metering_summary.json").read_text())
recount = summary["completion_text"]

# Recover the per-category recount table from the original analyzer's output.
analysis = (root / "metering_analysis.txt").read_text()
section = analysis.split("## T1 completion_tokens vs re-encoding delivered text\n")[1].split("\n##")[0]
categories = []
for line in section.splitlines():
    cells = [c.strip() for c in line.strip("|").split("|")]
    if len(cells) == 8 and cells[1].isdigit():
        reported, recounted = int(cells[5]), int(cells[6])
        categories.append({"category": cells[0], "requests": int(cells[1]),
                           "reported": reported, "recounted": recounted,
                           "undercount_percent": 100 * (reported - recounted) / reported})

tor = json.loads((root / "tor_projection.json").read_text())
streams = tor["streams"] + tor["sustained"]
measurement_text = (root / "attestation_measurements.txt").read_text()
positive_text, negative_text = measurement_text.split("\nnegative controls: each must NOT equal the real measurement\n")
positive = list(csv.DictReader(io.StringIO(positive_text.strip())))
negative = list(csv.DictReader(io.StringIO(negative_text.strip())))
credit_text = (root / "credit_negative_tests.txt").read_text()
bitflips = re.search(r"flips=(\d+) decode_err=(\d+) InvalidProof=(\d+).*accepted_same_semantics=(\d+) accepted_changed=(\d+)", credit_text)
if bitflips is None:
    raise SystemExit("Could not parse credit experiment output")

result = {
    "method": "Reanalysis of recorded evidence, not a rerun of the experiments",
    "evidence_files_checksum_verified": len(provenance["files"]),
    "metering": {
        "recorded_requests": len(records),
        "accepted_requests": len(accepted),
        "preflight_prompt_matches": sum(r["prompt_tokens"] == r["preflight_prompt_tokens"] for r in accepted),
        "output_within_requested_limit": sum(r["completion_tokens"] <= r["output_limit"] for r in accepted),
        "total_hold_tokens": sum(r["preflight_prompt_tokens"] + r["output_limit"] for r in accepted),
        "total_used_tokens": sum(r["prompt_tokens"] + r["completion_tokens"] for r in accepted),
        "reported_completion_tokens_from_records": sum(r["completion_tokens"] for r in accepted),
        "recounted_completion_tokens_from_original_analyzer": recount["enc"],
        "aggregate_undercount_percent": 100 * (recount["rep"] - recount["enc"]) / recount["rep"],
        "per_category_from_original_analyzer": categories,
        "limitation": "Tokenizer was not rerun; recount values come from the original analyzer output. No 5% detection guarantee follows from these aggregates.",
    },
    "tor_recorded_analysis": {
        "runs": len(streams), "complete_runs": sum(r["done"] for r in streams),
        "tokens_received": sum(r["n_tokens_rx"] for r in streams),
        "tokens_emitted": sum(r["n_emitted"] for r in streams),
    },
    "attestation_recorded_output": {
        "matching_measurements": sum(r["report_measurement"] == r["computed"] for r in positive),
        "measurements": len(positive), "negative_controls": len(negative),
        "negative_controls_matching_original": sum(r["equals_real"] == "True" for r in negative),
    },
    "credit_recorded_output": dict(zip(
        ["bitflips", "decode_errors", "invalid_proofs", "accepted_same", "accepted_changed"],
        map(int, bitflips.groups()),
    )),
    "illustrative_arithmetic": {
        "three_TC15_only_in_600": {"count": 3, "ratio_percent": 100 * 3 / 600},
        "three_each_in_TC15_and_TC40_in_600": {"count": 6, "ratio_percent": 100 * 6 / 600},
        "twenty_growing_prompts_at_200_new_tokens": 200 * sum(range(1, 21)),
        "new_tokens_only_in_same_simplified_example": 200 * 20,
        "timing_model_top1_N100_delay_1_day": 1 / (1 + (100 - 1) / 30),
        "timing_model_top1_N100_delay_7_days": 1 / (1 + (100 - 1) / 30 * 7),
    },
}
print(json.dumps(result, indent=2))
