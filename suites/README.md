# Acceptance test content

The registered recipes are `operator-smoke@0.1.0` and `codec-smoke@0.1.0`, both with purpose `acceptance`. `src/catalog.rs` expands them; `drivers/common.rs` and `src/measurement.rs` participate in their content identities.

Operator coverage uses widths 1, 31 and 128, execute/direct dispatch, scalar GF arithmetic, and copy/write isolation. Same-ID arithmetic is deferred pending a team contract. Codec coverage uses RQ/R10, K=64, 128-byte symbols, missing-source counts 0/8 and a fixed 128-repair budget. Each case has one warmup and three measured repetitions.

These are framework acceptance recipes. They do not define a formal performance matrix, statistical performance standard or optimization threshold. See the [usage guide](../README.md) for precise timing and units.
