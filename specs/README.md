# Interface and evidence contracts

## Request and identity

The request contains `components.engine`, `components.operator`, and `test`. Both component references carry a source form and an optional operator implementation selector. I selection and E generation are automatic. The request is not itself a completed four-tuple.

Resolved C records package/version, origin revision or registry checksum, source digest, implementation, captured source closure and dependency provenance. I/T identities each contain `id` and `digest`. E contains `conditions`, their canonical digest, and separate observations. Format version 1 describes the current internal result schema; the framework package version is provenance.

`specs/catalog-lock.json` records the current internal candidate. Package, I and T use the planned `0.1.0` baseline. Maintainers update candidate content digests deliberately during this unreleased round; the owner will freeze the release after acceptance. Results with different digests remain distinct even when their draft version labels match.

## Process protocol 1

Every invocation starts with exactly one hello containing `protocol`, `build`, `interface`, `test`, `operator`, and `family`. These must match the selected generated build.

- `describe`: hello followed by `done` with mode `describe`.
- `validate`: hello, one `validation` event per registered fatal or warning check, and `done` with mode `validate`.
- `measure`: hello, one `sample` event per case/repetition, and `done` with mode `measure`.
- A caught driver panic records a fatal execution diagnostic and an `error` event, then exits nonzero. Completed validation prefixes remain available. An abnormal exit or missing completion is never success.

A sample includes case, zero-based recorded repetition, elapsed nanoseconds, original payload bytes, operation count and output witness. New warning-case samples also include `observation` with `calls: 256` and grouped actual outputs plus occurrence counts. Every timed call is accounted for. Warmup results are not samples. Case sets, exact counts, units and oracle semantics belong to T. Stdout is NDJSON; stderr is diagnostic output. The CLI's outer output is a JSON object, not this internal protocol.

The current invocation deadline is 60 seconds. Resolution/build subprocesses have separate bounded deadlines. The framework sends termination to a complete owned child process group and reaps its direct child. SIGINT produces an interrupted result when a run is active.

## Correctness records

For the current T each validation event includes `case`, registered `rule`, `severity` (`fatal` or `warning`), `status` (`passed` or `failed`) and `witness`. Boundary events additionally carry the operation, symbol width, dispatch, scope, expected/actual output and message. A failed warning-level check is non-blocking. Its output witness identifies observed data rather than claiming equality with the scalar reference.

The framework verifies coverage and the registered severity; a driver cannot downgrade a fatal check. It validates observation hashes independently and reports measured deviations as well as validation deviations. Fatal sample witnesses must match their correctness check; warning sample witnesses may differ and their observations remain visible.

`result.json.correctness` and `correctness.json` provide status, counts of executed fatal/warning checks, fatal failures and warnings. `passed_with_warnings` remains run status `succeeded` and CLI exit 0. A successful run must contain all registered observations, including passing warning-level checks.

## Artifact validity

A sealed run contains the original request, stage state, resolved/configured inputs, available build provenance, validation, raw samples and result record. `seal.json` maps relative artifact names to SHA-256. Paths escaping the evidence directory are rejected. Seals are integrity checks, not signatures or third-party attestations.

Result statuses are `prepared`, `succeeded`, `failed`, `unsupported`, and `interrupted`; checkpoints can be `running`. `stages.json` also explicitly represents stages that were `not_executed`. A prepared record has no measurements. A failed measurement can retain its valid prefix but has no completed aggregate performance summary.

The result's optional four-tuple is created after build and capability/loader checks. A run that fails before that retains partial inputs without fabricating complete C/I/T/E. Correctness can subsequently reject an otherwise identified build.

`compare` verifies complete samples and their workload counts, registered test expansion, current I/T identities, equal T/E conditions and matching fatal-check witnesses. Warning differences are reported on both sides and each affected performance row; they do not reject a comparison. It recomputes descriptive summaries from raw samples. The current cross-I relation is limited to the registered families supporting the selected acceptance T: native 2 and 3 for operator tests, all three families for codec tests. It does not imply general API or arbitrary-workload equivalence.

## Product provenance

The product integration records the binary-side and callback-side dependency graphs separately. Cargo.lock contents, normalized manifest bindings, shared-library hash, driver hash, absolute load location and dynamic-loader output are retained. Validation confirms callback reads/writes and two operator destructions per encoder/decoder pair. Measurement disables those counters. The library is released by normal process exit after facade objects have been dropped.
