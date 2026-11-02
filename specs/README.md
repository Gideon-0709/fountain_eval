# Configuration, process and evidence contracts

## User configuration

The public input is one strict TOML document. Required keys are `engine`, `operator`, `scheme`, and `test`. Scheme is a name string or a table with `name`, optional `source`, and optional `mode` (default `systematic`). `test` is an exact content version, currently `0.1.0`; it does not contain a suite name. Optional `[run]` accepts only `output` and `cache`.

References support local packages, fixed Git commits and exact registry versions. Every relative path is resolved against the config directory. Unknown keys, duplicate keys, per-case selectors and arbitrary parameter overrides are rejected. Config text, path, digest, normalized request and resolved storage paths are retained. Prepared execution consumes this stored input without rereading the original file.

Config syntax is framework-owned. T owns workloads, severity, timing and metrics. I owns bindings, source defaults and capabilities. Source and invocation forms are independent. Product ABI rejects an external scheme source; native integrations bind and verify the selected scheme package.

## Four-tuple and content catalog

Resolved C records engine/operator/scheme package identities, source digests, algorithm selection and dependency provenance. I/T identities contain `id` and `digest`. E contains common conditions and their digest, including configured scheme name/mode, plus separate runtime observations. Scheme implementation versions belong to C rather than E.

`specs/catalog-lock.json` records compact I/T identity seals for the whole internal candidate. Package, I and T remain on the planned 0.1.0 baseline; no release is implied. A whole-content T identity is independent of per-run applicability and interface family. Registered content is expanded for the configured scheme and capabilities, preserving every registered case. Different draft content digests are not treated as equivalent.

## Applicability and coverage

Each check declares severity, rule, work counts and whether it is measured. Expanded checks have optional `skip` and `measurement_skip` objects with `code` and `reason`. Current codes are `scheme_not_selected`, `capability_unavailable` and `outside_api_domain`. Missing or unsupported scheme/interface selection is a configuration/integration error, not a successful empty plan.

`test-plan.json` retains the full expansion. `coverage.json` and `result.json.coverage` list every case with separate validation and measurement states. Validation states include passed, failed, skipped and not_executed. Measurements include recorded, incomplete, skipped, not_applicable and not_executed. Recorded samples are accepted only for a succeeded run. Prepared and early-failed results do not claim validation or measurement completion.

A successful result requires every applicable validation event and every applicable sample. The validator reconstructs the canonical plan, so an edited skip cannot hide missing evidence. Failures and interruptions leave unfinished cases unexecuted; they are never converted into applicability skips. The acceptance harness runs both schemes and writes reachability accounting; cases blocked by upstream fatal checks remain documented coverage gaps.

## Process protocol 1

Each invocation starts with exactly one hello containing protocol, build, interface, whole test version, selected scheme, operator, family, worker and instrumentation mode. These must match the generated build and request.

- `describe`: hello followed by done with mode describe.
- `validate`: one validation event per applicable check, then done.
- `measure WORKER`: one warmup and five samples per ordinary applicable case, then done.
- `first-use REPETITION CASE`: one internally scheduled fresh-process sample, then done.
- `diagnose WORKER`: one diagnostic event per applicable chain/workflow case except fresh-process cases, using the diagnostic binary.
- A caught panic records a fatal execution diagnostic and an error, then exits nonzero. Completed prefixes remain available.

The driver does not decide which failed checks to skip. Applicability comes from the registered config/capability expansion and actual protocol coverage must match it. Stdout is NDJSON; stderr is diagnostic output. Malformed output, unexpected events, missing completion, timeout and abnormal exit remain failures.

Invocation deadlines and process-group cleanup remain enforced. SIGINT produces an interrupted record when a run is active. Resolution/build have separate bounded deadlines.

## Correctness and samples

Validation includes case, rule, fatal/warning severity, passed/failed status and witness. Boundary events carry operation arguments, width, dispatch, scope, expected/actual values and explanation. A failed warning-level check is non-blocking; its witness identifies observed data rather than proving equality with the reference.

Samples include case, worker, repetition, elapsed nanoseconds, payload bytes, operation count, outcome, details and witness. Boundary measurements contain grouped observations accounting for the T-derived number of calls; rejected boundary calls are labelled rather than counted as successful byte operations. Warmup results are not samples. Initialization, output inspection and serialization are outside operator timing. Codec output-ready timing includes construction and loading through owned output. Lifecycle cases include destruction; the details retain both boundaries.

Fatal sample witnesses must match their validated results. Warning observations can differ and remain visible. `correctness.json` and the result record report executed check counts, fatal failures and warnings. A succeeded run can have both `passed_with_warnings` correctness and `completed_with_skips` coverage.

## Evidence and comparison

Sealed results preserve config, request, available source/build/configuration provenance, complete test plan and coverage, correctness, raw samples, separate diagnostic observations, HTML/Markdown/CSV views and logs. `seal.json` maps artifact paths to SHA-256; escaping paths are rejected. Seals detect modification, not establish a third-party signature. Early failures do not fabricate a four-tuple.

Comparison first validates both whole plans and all applicable evidence, then compares common measured cases. T and E must match, I must be registered and common fatal witnesses must agree. Both coverage tables are returned and the comparison scope is explicit. There are no fabricated rows for skipped cases. Warning differences do not reject comparison. Results with no common measurements are rejected.

Source references, config comments, run IDs, timestamps and temporary output/cache locations do not replace actual C/I/T/E identities. Product dependency graphs and loader evidence remain separate from callback-side provenance. Report and comparison do not require the build cache; prepared execution does.
