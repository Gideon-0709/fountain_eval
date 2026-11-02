# Acceptance test content

Current recipes are `operator-smoke@0.1.0` and `codec-smoke@0.1.0`, both with purpose `acceptance`. They are development regression fixtures for the framework. The team's formal correctness and performance test content has not yet been designed. `src/catalog.rs` expands inputs and check severity; `drivers/common.rs` and `src/measurement.rs` participate in T identity.

Fatal checks cover required operator behavior and codec correctness. Warning checks record self-XOR and repeated-source deviations without failing a run or preventing performance comparison. Operator boundary calls have their own timing rows. Codec boundary diagnostics inspect the selected operator outside codec timing; product diagnostics identify the callback implementation as their scope.

The repository is an unreleased internal 0.1.0 candidate. This round updates the same baseline and its content digests. The project owner controls the first team release; there is no historical compatibility layer.

Each measured case has one warmup and three recorded repetitions. These recipes verify framework mechanisms; formal performance matrices and thresholds remain a separate design task. See the [usage guide](../README.md) for timing, severity and units.
