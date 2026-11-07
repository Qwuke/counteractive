# Evidence retained for the independent review

Reviewed 6 November 2026. These files support the [report feedback](../../../reports/Anonymous%20metered%20private%20inference%20-%20review.md).

The original experiment files still existed in the temporary directory identified by the spike notes. Selected outputs have been copied here so this review does not rely only on the report's prose. `provenance.json` records source paths and SHA-256 hashes, including the original report before the review notice was added.

- `metering_projection.json` contains numeric fields from the original 436 request records, including 434 accepted requests. It omits prompts, generated text and token sequences. Requests are deduplicated by their original `i` field, as in the original analyzer.
- `metering_summary.json` and `metering_analysis.txt` are verbatim outputs of the original analysis. Recounted text totals were checked for consistency, but the tokenizer was not rerun.
- `tor_projection.json` contains token delivery counts from the original analysis of nine short streams and two sustained streams. It is not a packet capture or an independent Tor experiment.
- `attestation_measurements.txt`, `credit_negative_tests.txt` and `credit_races.txt` are verbatim original output. Checking these records does not independently verify signatures, execute cryptography, or reproduce the hardware measurements.
- `audit.py` recalculates selected totals, checks evidence hashes, and exposes the assumptions behind the arithmetic. `audit_results.json` records its output.

Run from any directory:

```sh
python3 '/Users/qwuke/Projects/counteractive/research_notes/Anonymous metered private inference/review_2026_11_06/audit.py'
```

Primary sources were also checked through the web and the GitHub connector. The feedback links each finding to its source. OpenPCC source was read at default-branch commit `911390a80afcbe6dfb48987a641ab345494b1eca`; `anonpay/processor.go` had blob SHA `7e60ee1678ad7204a524739f4a01d27ada868d8f`. Routstr PR 736 was confirmed merged on 17 September 2026, with merge commit `69a79a1e2c660f085b312aedf0dba39d3e1529a8`.

This is a targeted evidence audit, not an exhaustive security audit. No cloud resources were provisioned, live payments processed, external code built, or messages sent to maintainers. No new claim of real-world anonymity, production readiness, or legal clearance is made.
