# Configuration, process and evidence contracts

## User configuration

The public input is one strict TOML document. Required shared keys are `scheme` and `test`, plus at least one indexed `engineN` or `operatorN` reference. References with the same canonical nonnegative integer N form one C; indices may be sparse and execute numerically. A C may contain either or both roles. Unindexed keys and leading-zero indices are rejected. Scheme is a name string or a table with `name`, optional `source`, and optional `mode` (default `systematic`). `test` is an exact content version, currently `0.1.0`; it does not contain a suite name. Optional `[run]` accepts `report` (Markdown root, default `reports`), `output` (sealed evidence root, default `runs`) and `cache` (default `.fountain-eval`). Paths resolve relative to the config. The optional report field is omitted from normalized serialization when absent, so pre-existing sealed configurations retain their identity. It is a storage setting, not an E condition.

References support local packages, fixed Git commits and exact registry versions. Every relative path is resolved against the config directory. Unknown keys, duplicate keys, per-case selectors and arbitrary parameter overrides are rejected. Config text, path, digest, normalized per-C requests and resolved storage paths are retained. Prepared execution consumes this stored input without rereading the original file.

Config syntax is framework-owned. T owns workloads, severity, timing and metrics. I owns bindings, source defaults and capabilities. Source and invocation forms are independent. Product ABI rejects an external scheme source; native integrations bind and verify the selected scheme package.

## Four-tuple and content catalog

Resolved C records engine/operator/scheme package identities, source digests, algorithm selection and dependency provenance. I/T identities contain `id` and `digest`. E contains common conditions and their digest, including configured scheme name/mode, plus separate runtime observations. Scheme implementation versions belong to C rather than E.

`specs/catalog-lock.json` records compact I/T identity seals for the whole internal candidate. Package, I and T remain on the planned 0.1.0 baseline; no release is implied. A whole-content T identity is independent of per-run applicability and interface family. Registered content is expanded for the configured scheme and capabilities, preserving every registered case. Different draft content digests are not treated as equivalent.

## Applicability and coverage

Each check declares severity, rule, work counts and whether it is measured. Expanded checks have optional `skip` and `measurement_skip` objects with `code` and `reason`. Current codes are `scheme_not_selected`, `component_not_selected`, `capability_unavailable` and `outside_api_domain`. Missing or unsupported scheme/interface selection is a configuration/integration error, not a successful empty plan.

`test-plan.json` retains the full expansion. `coverage.json` and `result.json.coverage` list every case with separate validation and measurement states. Validation states include passed, failed, skipped and not_executed. Measurements include recorded, incomplete, skipped, not_applicable and not_executed. Recorded samples are accepted only for a succeeded run. Prepared and early-failed results do not claim validation or measurement completion.

A successful result requires every applicable validation event and every applicable sample. The validator reconstructs the canonical plan, so an edited skip cannot hide missing evidence. Failures and interruptions leave unfinished cases unexecuted; they are never converted into applicability skips. The acceptance harness runs both schemes and writes reachability accounting; cases blocked by upstream fatal checks remain documented coverage gaps.

## CLI output

`run` and `prepare` emit plain-text live progress on stderr and a final human-readable summary on stdout. `--json` replaces only the final stdout response with `{run_dir, report, batch}`. `inspect` and `list` retain JSON stdout. `report` and `compare` retain Markdown output. Progress never appears in driver NDJSON or evidence sample fields.

Validation rows distinguish passed checks, non-blocking warnings and fatal failures. Measurement rows show per-worker collection; `RECORDED` means the expected observations have arrived, not that the whole C has passed final evidence checks. Diagnostic counts are separate from ordinary samples. Unsupported and unexecuted cases remain explicit in phase/final summaries and full report coverage. A 15-second heartbeat reports active subprocesses or lock waits, not a completion estimate. The final paths are printed only after report publication.

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

## Batch evidence and wall time

Every `prepare` or `run` writes a unique batch root. `batch.json` contains `configuration.selections`, ordered `entries`, action, status, interruption flag, start timestamp and elapsed nanoseconds. Every entry records its index/request, outcome, optional relative child directory, optional elapsed nanoseconds, host phase times, check/warning/sample/diagnostic/skip counts and error. Unexecuted entries have no duration or fabricated child result. The separate report root contains `<run-id>/report.md` only, embedding all child results. No HTML or CSV views are generated. CLI `--json` returns both `run_dir` and `report`; `report`/`compare` render Markdown, while raw structured results remain in evidence JSON.

Per-C evidence lives in `C<index>/<run-id>/`. `result.json.configuration` retains the full original config, digest, derived request and component index. `timing` records elapsed nanoseconds through the last execution phase, excluding final child evidence/seal generation. Its host `phases_ns` includes setup, resolution, build, probe, validation, measurement and diagnostics when reached. Batch entry elapsed time additionally includes child evidence/seal work. Batch total includes sequential children and checkpoints, excluding its final seal/report publication. These operational durations are not benchmark samples or E equality fields.

Batch statuses are `succeeded`, `prepared`, `completed_with_failures`, `completed_with_unsupported`, or `interrupted`. A fatal/unsupported C leaves independent Cs runnable. User interruption leaves later entries `not_executed`. A prepared replay copies prior per-C outcomes and the origin seal to `prepared-from.json`; failed preparation entries are not retried. Child seals are verified along with the root seal; entry identity/status/counts must match child evidence.

## Evidence and comparison

Sealed results preserve config, request, available source/build/configuration provenance, complete test plan and coverage, correctness, raw samples, separate diagnostic observations and logs. `seal.json` maps artifact paths to SHA-256; escaping paths are rejected. Seals detect modification, not establish a third-party signature. Early failures do not fabricate a four-tuple.

`report` accepts a batch root or a per-C result, verifies the evidence and writes Markdown to stdout. Optional `--output DIR` reserves `DIR/<run-id>/report.md`; it never overwrites a previous report or changes the source seal. `compare` accepts two per-C results. Comparison first validates both whole plans and all applicable evidence, then compares common measured cases. T and E must match, I must be registered and common fatal witnesses must agree. Both coverage tables are returned and the comparison scope is explicit. There are no fabricated rows for skipped cases. Warning differences do not reject comparison. Results with no common measurements are rejected.

Source references, config comments, run IDs, timestamps and temporary output/cache locations do not replace actual C/I/T/E identities. Product dependency graphs and loader evidence remain separate from callback-side provenance. Report and comparison do not require the build cache; prepared execution does.
