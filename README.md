# fountain_eval

`feval` freezes Fountain components, generates isolated native builds, executes every applicable check, records measurements and compares results. The `fountain-eval` Cargo package provides the `fountain_eval` library and the `feval` CLI. Component source remains in its own repository.

The project is an unreleased internal **0.1.0** candidate. Its formal content covers validation (V), individual operations (O), execution chains (L) and integrated workflows (W). Encoding-plan construction/export and recorded execution belong to chain testing; plan reuse across blocks belongs to integrated testing. See the [test guide](docs/TEST.md) for contracts and interpretation and the automatically generated reports for executed coverage and limitations. No release or performance ranking is implied.

## Install and run

Requirements: Linux or macOS, Git, `tar`, a native linker/C toolchain and Rust managed by rustup. The generated drivers use Rust 1.89.0, release optimization and the host CPU. Positive native construction coverage uses authorized local Arena and product checkouts on ARM64. Public Slab/Simd currently fail the formal GF check; the pinned operator also has an AVX2 compile error on x86_64. CI verifies these rejections and records the remaining coverage gaps.

```bash
cargo install --path . --locked
cp fountain-eval.example.toml fountain-eval.toml
feval inspect
feval run
```

An installed CLI works without the eval checkout. Create a `fountain-eval.toml` containing indexed component references plus a shared scheme and test version:

```toml
engine0 = "registry:fountain_engine@2.1.0"
operator0 = "registry:fountain_operators@2.0.0#slab"
scheme = "rq"
test = "0.1.0"
```

`test` selects the **entire content version**. Users cannot select individual cases or disable checks. Every registered case is considered. Inapplicable cases remain in the report with a machine-readable reason and explanation. A fatal failure stops that C; later cases in that C remain `not_executed`. Independent Cs continue. An interrupt stops the batch and leaves subsequent Cs unexecuted.

Commands default to `fountain-eval.toml` in the current directory. An explicit location works from any directory:

```bash
feval inspect --config /path/to/experiment.toml
feval prepare --config /path/to/experiment.toml --json > prepared.json
feval run --config /path/to/experiment.toml
```

All relative paths are relative to the config file. There are no parent-directory searches, includes, environment interpolation, hidden global configuration or CLI overrides for component/test settings. Unknown fields, misspellings, duplicate keys and malformed references are errors. `--engine`, `--operator` and per-test `--test` flags have been removed.

## Terminal progress

`run` and `prepare` print compact stage progress to stderr. Each C has a separate heading with its position in the batch and concise component names. Four stages organize the work: **Preparation**, **Validation**, **Performance** and **Diagnostics**. Preparation groups input resolution, native builds and capability checks. `prepare` stops after that stage.

A component heading such as `C1 [2/2]` identifies the active selection. Numbered headings show preparation, correctness, performance and diagnostics. Preparation names the source, build and probe steps. During driver execution, subheadings identify the latest observed test group, such as encoding plans, individual operations or integrated workflows. Measurement and diagnostic groups also identify the worker.

A terminal refreshes one progress line with the observed count, percentage and stage elapsed time. CI and redirected stderr use plain lines at group changes and at 30-second intervals during long stages. A fatal error appears immediately. The final summary gives each component's outcome and the report path. `NO_COLOR` disables color.

Live counts and stage completion describe work performed. The whole C passes only after final protocol, coverage, correctness and evidence validation. Incomplete recovery outcomes are counted separately; the terminal does not aggregate throughput across different workloads. Read the report for individual operations, chains, workflows, detailed warning observations and performance dispersion.

For automation, add `--json` to keep the structured final response on stdout while progress remains on stderr:

```bash
feval run --json > run.json
feval prepare --json > prepared.json
# Optionally retain terminal progress in a separate log:
feval run --json > run.json 2> progress.log
```

The parent reads the driver's existing event log as it grows. Driver code, measured regions, warmups, samples and I/T identities are unchanged by progress display. Host phase durations include orchestration work; they are separate from benchmark timings.

## Multiple component selections in one config

Each index defines a C. For example:

```toml
# C0: a complete engine/operator combination.
engine0 = "registry:fountain_engine@2.1.0"
operator0 = "registry:fountain_operators@2.0.0#slab"
# C1: engine alone.
engine1 = "registry:fountain_engine@2.1.0"
# C2: operator alone (requires your local checkout).
operator2 = "path:../fountain-arena-operator"
# C3: another complete combination.
engine3 = "path:../erasure_coding_lib/fountain_engine"
operator3 = "path:../erasure_coding_lib/fountain_operators#simd"
scheme = "rq"
test = "0.1.0"
```

Indices are nonnegative integers, may be sparse, and execute in numeric order. There is no implicit pairing across indices and no cross-product. Every C receives the complete T; unsupported cases stay visible with a reason. Engine-only runs test standalone plan generation/export. Operator-only runs test the pure-operator surface. The missing production component is never added to enable codec tests. Supporting trait dependencies and the engine-only scalar validation helper are recorded separately from selected components. A product engine alone currently has no exposed standalone planning API and is reported unsupported with explicit skipped cases.

One C is also written with indexed keys (`engine0` and/or `operator0`). Unindexed `engine`/`operator` fields are rejected. All Cs share scheme, test version and storage settings. To test another scheme, use another config.

## Component references

Engine, operator and explicit scheme sources use the same grammar:

| Form | Example |
|---|---|
| Local package directory or manifest | `path:../engine` |
| Exact registry version | `registry:fountain_engine@2.1.0` |
| Fixed Git commit and package | `git+https://github.com/shhyang/fountain_operators.git?package=fountain_operators&rev=ce232222603876f3c033109c548cf1091e874c10#slab` |

Git revisions must be full commit IDs. Credentials do not belong in reference URLs. `fountain_operators` requires `#slab` or `#simd`; `fountain-arena-operator` selects Arena automatically. Engine references do not accept an implementation selector. Scheme implementation selection uses `scheme.name`.

Local inputs capture tracked and nonignored untracked modifications, deletions, workspace inheritance and required path dependencies. `prepare` freezes their actual bytes; later worktree changes do not alter prepared execution. Upstream files and version constraints are never rewritten. Required path bindings occur only in the generated build view, with original and effective manifests retained.

A local development configuration can be:

```toml
engine0 = "path:../erasure_coding_lib/fountain_engine"
operator0 = "path:../erasure_coding_lib/fountain_operators#simd"
scheme = "r10"
test = "0.1.0"
```

Local, Git and registry inputs are source choices. They do not create different user-facing test modes. Rust API and product ABI are the invocation boundaries; I selects the registered integration and validates it by compilation and probing.

## Scheme selection and provenance

The current integrations support `rq` and `r10` in `systematic` mode. `scheme = "rq"` is shorthand for the default source and that mode. Unknown names/modes are rejected explicitly, rather than replaced with another scheme.

Default sources are deterministic: public engine 2.1 uses registered `fountain_scheme` and `fountain_utility` 2.1.0; development engine 3 uses its monorepo packages. An explicit scheme source replaces only the selected scheme package:

```toml
engine0 = "registry:fountain_engine@2.1.0"
operator0 = "registry:fountain_operators@2.0.0#slab"
test = "0.1.0"

[scheme]
name = "rq"
source = "path:../my-schemes/fountain_scheme"
mode = "systematic"
```

`scheme.source` also accepts fixed Git and exact registry references. The selected scheme is snapshotted and bound into the actual driver graph. Metadata confirms that the built scheme is the selected one. Incompatible versions fail before measurement; no constraint is relaxed. The currently registered source package name is `fountain_scheme`; adding another implementation shape requires an I integration.

Scheme implementation identity belongs to C. The requested algorithm/mode is also recorded explicitly in E, so a normal same-condition comparison rejects RQ versus R10. Changing only the scheme implementation can be compared under the same algorithm, mode and other conditions. Cross-scheme studies require separately designed test methodology.

## Product API

To build the actual product shared library from an authorized monorepo checkout:

```toml
engine0 = "path:../erasure_coding_lib/fountain_bin_ffi"
operator0 = "path:../erasure_coding_lib/fountain_operators#slab"
scheme = "rq"
test = "0.1.0"
```

The product uses its bundled scheme; an explicit external `scheme.source` is rejected. Binary and callback dependencies, hashes and loader evidence are recorded separately. Arena can be used as a product callback operator, while its current exact engine 2.1 dependency conflicts with source engine 3.

Product runs execute codec, source-replacement and lifecycle checks through the real shared library and the selected SymbolOps callbacks. Full operator cases and standalone plan/replay cases require surfaces the product ABI does not provide and are explicitly skipped. Product resource observations cover the driver/callback allocator, not allocations inside the dynamic library. The registered R10 scheme accepts K=4..8192 on every integration; K=1 success cases are explicitly outside-domain skips. Product V11 checks below-domain rejection through its recoverable API.

## Effective settings and prepared execution

Test content derives K, symbol width, repair budget, seeds, ordering, warmup/repetition counts, severity and timing. This draft has no arbitrary parameter, severity, feature or per-case skip overrides. I derives native bindings/features. E captures actual common build and runtime conditions.

Optional storage settings belong in the same file:

```toml
[run]
report = "reports" # human-readable reports
output = "runs"    # temporary execution evidence
cache = ".fountain-eval"
```

The defaults are the paths above, relative to the config. Set `run.report` to the folder where you want to read reports. Every completed test reserves a unique `<report>/<run-id>/` directory and writes exactly one `report.md` containing every C. Raw evidence is temporary under `<output>/<run-id>/` and is deleted after verified report publication; per-batch cache files under `<cache>/<run-id>/` are removed at the same time. These directories serve different purposes. Report paths do not affect E or comparability. Normal execution never overwrites an existing report. Explicit `report --replace` can update the layout of the same historical run. The original config and its resolved request/settings are retained in the report index. Their location and comments do not affect experiment equality.

`prepare` leaves a pending workspace and its frozen cache, without publishing a final report. Its JSON response contains `prepared` and `batch`. Prepared execution consumes those inputs without rereading the original config:

```bash
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["prepared"])')
feval run --prepared "$PREPARED"
```

The pending workspace is single-use: after the run publishes its report, both that workspace and its cache are removed. Its output directory comes from the frozen configuration. Current runtime conditions are observed again; the prepared native binary requires its original CPU and OS family. `--prepared` cannot be combined with `--config`.

## What the formal content runs

`feval list` expands the complete 2,467-case candidate. The selected scheme and verified interface determine applicability; users cannot suppress cases.

| Category | Coverage | Measurement |
|---|---|---|
| Validation (V01–V11) | Scalar GF/byte oracle, mixed prefixes, storage/dispatch, boundary observations, scheme padding, plan reuse, small independent linear solve, receive ordering, participation and release | Validation outside ordinary timers |
| Individual operations (O01–O09) | Public storage, XOR, GF, copy/move and supported alias calls; direct/execute paths; small/large sets | One prepared call per independent instance |
| Execution chains (L01–L12) | Symbolic plan construction/export, actual encode/decode stage partitions, recorded real-session replay, operation mix and allocation diagnostics | Ordinary totals and separate instrumented observations |
| Integrated workflows (W01–W08) | First use, source/repair output, source replacement, all-source and missing-source decode, round trip, 32-block continuity and lifecycle | Directly measured output-ready/lifecycle durations |

The main matrix uses K=64/512/5008 and symbol widths 128/1016. Ordinary cases run in three sequential worker processes with one excluded warmup and five samples each (15 recorded samples). First-use cases use 15 fresh processes. Diagnostic builds have separate binaries and samples. Timers exclude checking and serialization; output-ready and full-lifecycle scopes are explicitly distinguished. No performance threshold determines correctness.

The pinned public Slab/Simd implementations have a `mul_add(src, 0, dst)` branch that clears `dst`. The independent mathematical reference expects `dst` unchanged. A run that detects this ordinary GF error fails before measurement; it is not converted into a self-XOR warning or silently skipped. Later cases remain unexecuted. This is a component result, not a reason to weaken the formal contract.

A warning retains expected/actual data, operation, width, dispatch and measured observations. Warnings do not fail correctness, suppress performance or veto comparison. A faster implementation returning immediately is still timed and its deviation is reported. Developers decide the tradeoff. `correctness.status: passed_with_warnings` can accompany `status: succeeded` and exit 0.

Crashes, timeouts, build conflicts, malformed evidence and missing applicable results remain failures. They are never converted to capability skips. Unknown I/T or scheme selections are explicit unsupported errors.

## Reading results and comparing

A normal run automatically writes **one Markdown report for the entire batch**:

```text
reports/
  <run-id>/
    report.md
```

The terminal prints the report path and confirms cleanup. With `--json`, the response contains `report` and `batch`. No HTML/CSV files, per-C reports or permanent raw result directories are generated.

```bash
REPORT=$(python3 -c 'import json; print(json.load(open("run.json"))["report"])')
# Verify and read the saved Markdown file:
feval report "$REPORT"
# Read its compact structured result index for automation:
feval report "$REPORT" --json
# Select one C from each report; a single-C report needs no selector:
feval compare "$REPORT#C0" /path/to/candidate/report.md#C0 > comparison.md
# Migrate an existing raw batch without repeating measurements:
feval report /path/to/runs/run-id --output ./reports
# Update an existing report for that same run, then clean its raw batch:
feval report /path/to/runs/run-id --output ./reports --replace
```

The report opens with component outcomes, fatal errors and grouped semantic warnings. Its performance tables show source-equivalent codec throughput at K=512/5008 and B=128/1016, plan construction/export time, dominant diagnostic stages and seven core operations in ns/call. A dagger flags a displayed median with CV above 10%. Coverage counts explain skipped and unfinished work.

`feval report REPORT.md` prints the readable summary. To refresh an older report in place, run `feval report REPORT.md --output REPORT_ROOT --replace`. The renderer reads the verified result index, so refreshing the layout also works after cache cleanup. `feval report REPORT.md --json` returns every case and diagnostic, and `feval compare LEFT RIGHT --json` returns all validated comparison rows.

The file includes a checksummed, compressed result index in a Markdown comment. It contains the original config, C/I/T/E, resolved test plan, build/source provenance, complete per-case coverage, correctness findings, grouped warning observations, aggregate performance results and per-metric diagnostic summaries. It contains no individual timing samples or raw log files. This index keeps `report`, `compare` usable after cleanup. Visible text and the index are checked together; checksums detect changes, not authenticate a publisher. `report --json` exposes the index without creating another file. Removing it disables structured reading and comparison.

`compare` verifies T/E/I and common fatal-check witnesses before comparing common measured cases. Failed runs are ineligible; semantic warnings remain visible and eligible. Comparison uses the aggregate statistics validated before cleanup; it cannot recalculate statistics from disposed samples. Existing per-C raw evidence directories are still readable. A finalized report is a result, not a replayable preparation.

## Evidence, failures and retention

During execution the workspace contains configuration, plan, source/build provenance, validation, raw samples, diagnostics and subprocess logs. On completion the framework verifies these inputs, publishes the single Markdown file atomically, reads it back, then removes that invocation's raw batch, source snapshots, native build products and cache, including a consumed preparation workspace. C entries share cache only during that invocation; subsequent runs build afresh. The same rule applies to successful, failed, unsupported and interrupted batches. Their completed prefixes, errors, warning findings and exact coverage remain in the report; failed runs never become passes. Failure diagnostics retain labelled excerpts where needed. Large byte-array assertions retain lengths, the first differing offset/values, total differing offsets, short prefixes and SHA-256 hashes instead of full payload dumps.

A write, verification or cleanup failure is reported explicitly. Unpublished evidence is retained; it is never discarded to make an attempt appear successful. Cleanup rejects report/cache overlap and evidence changed after publication. Cleanup uses ownership records and exclusive workspace locks. User config/source trees, other active or pending invocations, prior reports and system-wide Cargo/Rust installations are untouched. Empty storage roots are removed. `report --output DIR` provides explicit recovery after a failed publication. Without durable output, `report` only reads and does not clean its input.

| Exit | Meaning |
|---|---|
| 0 | Inspection, preparation or execution completed; review warnings and skips |
| 2 | Invalid configuration/CLI, unreadable evidence or rejected comparison |
| 3 | At least one unsupported C, with no failed/interrupted C |
| 4 | At least one failed or unexecuted C; inspect every batch entry |
| 130 | Interrupted run |

Start failures with the report outcome, fatal findings and the diagnostic excerpts in the result index. Incomplete/corrupt build entries fail without rebuilding or retrying silently. A completed failed run cleans its cache after recording the failure; a pending preparation with a corrupt cache fails without silently rebuilding. The CLI never uploads or archives results. A verified Markdown report is the retained result; all of its temporary run records and cache are removed automatically. Default reports/results/caches are ignored by Git; custom storage outside those directories is the user's responsibility.

## Maintainer references

See the [0.1.0 test guide](docs/TEST.md), [DESIGN and implementation map](docs/DESIGN.md), [CONTRIBUTING](CONTRIBUTING.md) and [artifact contracts](specs/README.md). Reports are produced by the CLI; historical construction summaries are not the source of component results.
