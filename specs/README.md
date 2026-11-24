# Configuration, process and evidence contracts

## User configuration

The CLI uses an active workspace selected by `feval init` or `feval activate DIR`. The absolute path persists in the user state directory across invocations and working directories. `inspect`, `prepare` and `run` default to its `fountain-eval.toml`. An explicit `--config` must resolve to a file inside that workspace. `run --prepared` uses the sealed configuration.

The public input is one strict TOML document. Required shared keys are `scheme` and `test`, plus at least one indexed `engineN` or `operatorN` reference. References with the same canonical nonnegative index form one C. Indices may be sparse and execute numerically. Unindexed keys and leading-zero indices are rejected. A scheme is a name string or a table with `name`, optional `source` and optional `mode` (default `systematic`). `test` is an exact content version, currently `0.1.0`. Result, report, preparation and cache roots are derived from the workspace. Storage paths do not participate in E equality.

References support local packages, fixed Git commits and exact registry versions. Operator references append `#` and a crate-relative Rust type path. Engine and operator entries accept a string or a table containing `source`, `features` (default `[]`) and `default-features` (default `false`). Every relative path is resolved against the config directory. Unknown keys, duplicate keys, per-case selectors and arbitrary parameter overrides are rejected. Config text, path, digest, normalized per-C requests and resolved storage paths are retained. Prepared execution consumes this stored input without rereading the original file.

Config syntax is framework-owned. T owns workloads, severity, timing and metrics. I owns bindings, source defaults and capabilities. Source and invocation forms are independent. Product ABI rejects an external scheme source; native integrations bind and verify the selected scheme package.

## Four-tuple and content catalog

Resolved C records engine/operator/scheme package identities, the selected operator type, source digests, requested Cargo build options, algorithm selection and dependency provenance. Resolved features are retained in the normal driver, diagnostic driver and product library graphs as applicable. I/T identities contain `id` and `digest`. E contains common conditions and their digest, including configured scheme name/mode, plus separate runtime observations. Scheme implementation versions belong to C rather than E.

`specs/catalog-lock.json` records compact I/T identity seals for the whole internal candidate. Package, I and T remain on the planned 0.1.0 baseline; no release is implied. A whole-content T identity is independent of per-run applicability and interface family. Registered content is expanded for the configured scheme and capabilities, preserving every registered case. Different draft content digests are not treated as equivalent.

## Applicability and coverage

Each check declares severity, rule, work counts and whether it is measured. Expanded checks have optional `skip` and `measurement_skip` objects with `code` and `reason`. Current codes are `scheme_not_selected`, `component_not_selected`, `capability_unavailable` and `outside_api_domain`. Missing or unsupported scheme/interface selection is a configuration/integration error, not a successful empty plan.

During execution, `test-plan.json` records the full expansion and `coverage.json` records every case. Publication retains the plan and coverage in the report directory's index, with separate validation and measurement states. Validation states include passed, failed, skipped and not_executed. Measurements include recorded, incomplete, skipped, not_applicable and not_executed. Recorded samples are accepted only for a succeeded run. Prepared and early-failed results do not claim validation or measurement completion.

A successful result requires every applicable validation event and every applicable sample. The validator reconstructs the canonical plan, so an edited skip cannot hide missing evidence. Failures and interruptions leave unfinished cases unexecuted; they are never converted into applicability skips. Use normal run reports to verify coverage for each configured scheme and integration. Cases blocked by fatal checks remain coverage gaps.

## CLI output

`run` and `prepare` emit plain-text live progress on stderr and a final human-readable summary on stdout. `--json` replaces only the final stdout response with `{result, batch}` for execution and `{prepared, batch}` for a pending preparation. `inspect` and `list` retain JSON stdout. `report <result-id> --json` returns `{report, markdown, result_id}` after publication. `report REPORT_DIR --json` reads the retained result index. `compare` defaults to Markdown output. Progress never appears in driver NDJSON or evidence sample fields.

Terminal output groups resolution/build/probe into Preparation, followed by Validation, Performance and Diagnostics. Each stage has a start and aggregate completion row. Numbered headings and observed test groups locate progress within a component. Interactive stderr updates one line at one-second intervals. CI and redirected stderr emit plain milestones with 30-second heartbeats across invocations. No ordinary case ID, individual warning or per-case rate is printed. Fatal check errors and protocol errors have one concise immediate notice; crashes and interruptions remain visible in the final component status. Warning counts, incomplete outcomes, exclusions and unexecuted checks are retained in summaries and full report coverage. Observed counts do not imply that final evidence validation has succeeded. The final execution response identifies the sealed raw result. Final report paths appear only after publication.

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

## Validation and samples

Validation includes case, rule, fatal/warning severity, passed/failed status and witness. Boundary events carry operation arguments, width, dispatch, scope, expected/actual values and explanation. A failed warning-level check is non-blocking; its witness identifies observed data rather than proving equality with the reference.

Samples include case, worker, repetition, elapsed nanoseconds, payload bytes, operation count, outcome, details and witness. Boundary measurements contain grouped observations accounting for the T-derived number of calls; rejected boundary calls are labelled rather than counted as successful byte operations. Warmup results are not samples. Initialization, output inspection and serialization are outside operator timing. Codec output-ready timing includes construction and loading through owned output. Lifecycle cases include destruction; the details retain both boundaries.

Fatal sample witnesses must match their validated results. Warning observations can differ and remain visible. `correctness.json` and the result record report executed check counts, fatal failures and warnings. A succeeded run can have both `passed_with_warnings` correctness and `completed_with_skips` coverage.

## Result publication and retention

`run` returns a unique sealed batch under `results/<result-id>/`. Its batch and child records describe the config, plan, source/build inputs, validation, samples and diagnostics. Per-C timing excludes final child sealing. Batch entry wall time includes child sealing. Report generation and cleanup have their own lifetime. These operational times are separate from benchmark samples and E conditions.

`report <result-id>` publishes `reports/<result-id>/`. It contains a plain `report.md` and `index.json.lzma`. The latter is LZMA-compressed JSON with `markdown_sha256`, `snapshot_sha256` and `snapshot`. The loader checks the Markdown byte hash and the canonical JSON snapshot hash. The snapshot retains batch metadata, config and plans, C/I/T/E/build identity, coverage, fatal witnesses, warning variants, aggregate timings and diagnostic summaries. Individual sample arrays and raw logs remain in the raw result. The loader accepts older single-file reports with an embedded index.

Publication validates the raw evidence, writes and syncs both files in a staging directory, verifies the pair and renames the directory into place. It then rechecks the source seal and ownership before removing the raw batch, its private cache and any consumed preparation by default. `--keep` retains these inputs. A repeat request uses the existing report if it verifies and belongs to the same sealed run. Publication or verification failure leaves raw data available. `report REPORT_DIR` reads a verified report in place.

The active workspace has fixed storage roots. It uses a shared lock for evaluation and an exclusive lock for `clean` or reset. Per-result and preparation leases protect concurrent publication or consumption. `clean` empties `results/`, `.fountain-eval/cache/` and `.fountain-eval/prepared/`. It preserves config and reports. Explicit `init DIR` empties and recreates the supplied existing directory's contents.

`compare LEFT RIGHT` accepts report directories or their Markdown paths with optional `#C<N>` selectors, required for multi-C reports, or historical per-C raw results. It checks successful status, T/E identity, registered I, canonical aggregate case coverage and common fatal witnesses. Warning differences remain eligible. Comparison consumes statistics validated at publication; disposed samples cannot be reanalyzed. The result includes scope, dispersion, warnings and coverage differences. No common measured cases is an error.

`prepare` seals a pending workspace and retains its frozen cache; it does not publish a final report. `run --prepared DIRECTORY` exclusively consumes that workspace and revalidates the cache and current CPU/OS; failed prepared entries remain unexecuted rather than retried. Origin identity and previous outcomes are retained in the final report. Report publication removes both the new run and its consumed preparation, including their caches, by default. Preparation is single-use and independent of later changes to the original source/config.
