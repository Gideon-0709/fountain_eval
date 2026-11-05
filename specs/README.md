# Configuration, process and evidence contracts

## User configuration

The public input is one strict TOML document. Required shared keys are `scheme` and `test`, plus at least one indexed `engineN` or `operatorN` reference. References with the same canonical nonnegative integer N form one C; indices may be sparse and execute numerically. A C may contain either or both roles. Unindexed keys and leading-zero indices are rejected. Scheme is a name string or a table with `name`, optional `source`, and optional `mode` (default `systematic`). `test` is an exact content version, currently `0.1.0`; it does not contain a suite name. Optional `[run]` accepts `report` (Markdown root, default `reports`), `output` (temporary evidence root, default `runs`) and `cache` (default `.fountain-eval`). Paths resolve relative to the config. The optional report field is omitted from normalized serialization when absent, so pre-existing sealed configurations retain their identity. It is a storage setting, not an E condition.

References support local packages, fixed Git commits and exact registry versions. Every relative path is resolved against the config directory. Unknown keys, duplicate keys, per-case selectors and arbitrary parameter overrides are rejected. Config text, path, digest, normalized per-C requests and resolved storage paths are retained. Prepared execution consumes this stored input without rereading the original file.

Config syntax is framework-owned. T owns workloads, severity, timing and metrics. I owns bindings, source defaults and capabilities. Source and invocation forms are independent. Product ABI rejects an external scheme source; native integrations bind and verify the selected scheme package.

## Four-tuple and content catalog

Resolved C records engine/operator/scheme package identities, source digests, algorithm selection and dependency provenance. I/T identities contain `id` and `digest`. E contains common conditions and their digest, including configured scheme name/mode, plus separate runtime observations. Scheme implementation versions belong to C rather than E.

`specs/catalog-lock.json` records compact I/T identity seals for the whole internal candidate. Package, I and T remain on the planned 0.1.0 baseline; no release is implied. A whole-content T identity is independent of per-run applicability and interface family. Registered content is expanded for the configured scheme and capabilities, preserving every registered case. Different draft content digests are not treated as equivalent.

## Applicability and coverage

Each check declares severity, rule, work counts and whether it is measured. Expanded checks have optional `skip` and `measurement_skip` objects with `code` and `reason`. Current codes are `scheme_not_selected`, `component_not_selected`, `capability_unavailable` and `outside_api_domain`. Missing or unsupported scheme/interface selection is a configuration/integration error, not a successful empty plan.

During execution, `test-plan.json` records the full expansion and `coverage.json` records every case. Publication retains the plan and coverage in the Markdown result index, with separate validation and measurement states. Validation states include passed, failed, skipped and not_executed. Measurements include recorded, incomplete, skipped, not_applicable and not_executed. Recorded samples are accepted only for a succeeded run. Prepared and early-failed results do not claim validation or measurement completion.

A successful result requires every applicable validation event and every applicable sample. The validator reconstructs the canonical plan, so an edited skip cannot hide missing evidence. Failures and interruptions leave unfinished cases unexecuted; they are never converted into applicability skips. The acceptance harness runs both schemes and writes reachability accounting; cases blocked by upstream fatal checks remain documented coverage gaps.

## CLI output

`run` and `prepare` emit plain-text live progress on stderr and a final human-readable summary on stdout. `--json` replaces only the final stdout response with `{report, batch}` for execution and `{prepared, batch}` for a pending preparation. `inspect` and `list` retain JSON stdout. `report` and `compare` default to Markdown output; `report --json` reads the retained result index. Progress never appears in driver NDJSON or evidence sample fields.

Terminal output groups resolution/build/probe into Preparation, followed by Correctness, Performance and Diagnostics. Each stage has a start and aggregate completion row. Numbered headings and observed test groups locate progress within a component. Interactive stderr updates one line at one-second intervals. CI and redirected stderr emit plain milestones with 30-second heartbeats across invocations. No ordinary case ID, individual warning or per-case rate is printed. Fatal check errors and protocol errors have one concise immediate notice; crashes and interruptions remain visible in the final component status. Warning counts, incomplete outcomes, exclusions and unexecuted checks are retained in summaries and full report coverage. Observed counts do not imply that final evidence validation has succeeded. Final report paths appear only after publication.

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

## Result publication and retention

Execution uses a temporary batch root under `run.output`. Its batch/child records and seals describe the full config, resolved plan, source/build inputs, validation, samples and diagnostics. Per-C timing excludes final child sealing; batch entry wall time includes child sealing; batch total excludes final report publication and cleanup. These are operational times, not benchmark samples or E conditions.

The permanent result is `run.report/<run-id>/report.md`. It contains summary-first human-readable results plus a zlib/base64 result index in a Markdown comment. SHA-256 covers the visible body and encoded index together. The index preserves batch metadata, resolved config/plans, C/I/T/E/build identity, exact per-case coverage, fatal witnesses, warning variants, aggregate timings and diagnostic metric summaries. Raw sample arrays and raw log files are not embedded. `report --json` returns this index. Checksums detect modification; they are not publisher signatures.

Publication validates the full temporary evidence, writes and syncs a temporary report, verifies round-trip content, then atomically installs it. Only after the source seal and ownership are rechecked are that raw batch, its owned cache and any consumed preparation workspace removed. Failure to publish/verify leaves raw data intact. Cleanup rejects paths overlapping the report or cache and never removes unrelated runs or sources. A cleanup failure reports the retained locations. Failed/interrupted/unsupported results follow the same publication rule and retain completed prefixes, diagnostics and exact unfinished coverage. A private `<cache>/<run-id>` directory is shared by C entries within this batch only and removed after publication. Ownership records and workspace leases prevent deleting another invocation; empty immediate storage roots are removed. System-wide Cargo/Rust files and user sources are outside this ownership.

`report PATH` verifies and displays an existing report or reads historical raw evidence. `--output DIR` creates a durable report; `--replace` explicitly replaces only the same run. Publishing from a raw batch automatically cleans its owned temporary storage after verification. Without `--output`, no cleanup occurs. The normal batch API returns the report path and batch record; CLI JSON contains `report` and `batch`.

`compare LEFT RIGHT` accepts report paths with optional `#C<N>` selectors, required for multi-C reports, or historical per-C raw results. It checks successful status, T/E identity, registered I, canonical aggregate case coverage and common fatal witnesses. Warning differences remain eligible. Comparison consumes statistics validated at publication; disposed samples cannot be reanalyzed. The result includes scope, dispersion, warnings and coverage differences. No common measured cases is an error.

`prepare` seals a pending workspace and retains its frozen cache; it does not publish a final report. `run --prepared DIRECTORY` exclusively consumes that workspace and revalidates the cache and current CPU/OS; failed prepared entries remain unexecuted rather than retried. Origin identity and previous outcomes are retained in the final report. After verified publication, both the new run and its consumed preparation are removed, including their caches. Preparation is single-use and independent of later changes to the original source/config.
