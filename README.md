# fountain_eval

`feval` freezes Fountain components, generates isolated native builds, executes every applicable check, records measurements and compares results. The `fountain-eval` Cargo package provides the `fountain_eval` library and the `feval` CLI. Component source remains in its own repository.

The project is an unreleased internal **0.1.0** candidate. Its formal content covers validation tests (V), operation tests (O), link tests (L) and workflow tests (W). Encoding-plan construction and operation-list cloning and recorded execution belong to link testing; plan reuse across blocks belongs to workflow testing. See the [test guide](docs/TEST.md) for contracts and interpretation and the automatically generated reports for executed coverage and limitations. No release or performance ranking is implied.

## Install and initialize a workspace

Requirements: Linux or macOS, Git, `tar`, a native linker/C toolchain and Rust managed by rustup. The generated drivers use Rust 1.89.0, release optimization and the host CPU. Positive native construction coverage uses authorized local Arena and product checkouts on ARM64. Public Slab/Simd currently fail the formal GF check; the pinned operator also has an AVX2 compile error on x86_64. CI verifies these rejections and records the remaining coverage gaps.

```bash
cargo install --path . --locked
cd /path/to/experiments
feval init
feval workspace
feval inspect
feval run
feval report <result-id>
```

Run the installation command from the repository. The subsequent commands work from any directory after activation. Edit `feval_workspace/fountain-eval.toml` to select the components, scheme and test version. The starter uses the public engine's standalone encoding-plan tests. Add a compatible operator to evaluate a pair. The repository's [configuration](fountain-eval.toml) is a pair comparison you can copy into a workspace. Adjust its Arena path for the new config location.

```text
feval_workspace/
  fountain-eval.toml
  results/
    <result-id>/
  reports/
    <result-id>/
      report.md
      index.json.lzma
  .fountain-eval/
    cache/
    prepared/
```

| Command | Effect |
|---|---|
| `feval init` | Create and activate a new `feval_workspace` in the current directory. If it already exists, use `feval activate ./feval_workspace` to reopen it. |
| `feval init DIR` | **Empty the existing directory in place**, including all hidden files, then recreate and activate the workspace. This is also how to reset an existing workspace. The filesystem root is rejected. |
| `feval activate DIR` | Select an initialized workspace for future commands across shells. |
| `feval workspace` | Print the active workspace's absolute path. |
| `feval clean` | Empty raw results, pending preparations and build cache in the active workspace. Keep its configuration and reports. |

The activation record is stored at `$XDG_STATE_HOME/feval/active.json` or `~/.local/state/feval/active.json`. Use `feval --help` for a command list and `feval run --help` or `feval help init` for command-specific options.

`test` selects the **entire content version**. Every registered case is considered. Inapplicable cases appear with a machine-readable reason and explanation. Validation runs in order from V01 to V11, completing all independent applicable V cases even after a fatal. All fatal findings are reported. If V has any fatal, that C does not execute O/L/W prechecks or measurements. Warnings do not block performance testing. A failing case retains its first failing assertion; crashes, timeouts and interruption can leave V incomplete. Independent Cs continue after a failed C; an interrupt stops the batch and leaves subsequent Cs unexecuted.

`inspect`, `prepare` and `run` use the active workspace's `fountain-eval.toml`. Use `--config` for another file inside that workspace. A relative `--config` is resolved from the workspace root. Local component paths are always relative to the selected config's directory.

```bash
feval inspect --config alternatives/r10.toml
feval prepare --config alternatives/r10.toml --json > prepared.json
feval run --config alternatives/r10.toml
```

Unknown fields, duplicate keys and malformed references are configuration errors. When moving an older config into a workspace, remove its `[run]` section. The workspace determines all output paths.

## Terminal progress

`run` and `prepare` print compact stage progress to stderr. Each C has a separate heading with its position in the batch and concise component names. Four stages organize the work: **Preparation**, **Validation**, **Performance** and **Diagnostics**. Preparation groups input resolution, native builds and capability checks. `prepare` stops after that stage.

A component heading such as `C1 [2/2]` identifies the active selection. Numbered headings show preparation, validation, performance and diagnostics. Preparation names the source, build and probe steps. During driver execution, subheadings identify the latest observed test group, such as encoding plans, operation tests, link tests or workflow tests. Measurement and diagnostic groups also identify the worker.

A terminal refreshes one progress line with the observed count, percentage and stage elapsed time. CI and redirected stderr use plain lines at group changes and at 30-second intervals during long stages. A fatal error appears immediately. The final summary gives each component's outcome, the sealed result path and the command to generate a report. `NO_COLOR` disables color.

Live counts and stage completion describe work performed. The whole C passes only after final protocol, coverage, correctness and evidence validation. Incomplete recovery outcomes are counted separately; the terminal does not aggregate throughput across different workloads. Read the report for operation, link and workflow test results, detailed warning observations and performance dispersion.

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
operator0 = { source = "registry:fountain_operators@2.0.0#SlabDataOperator", features = ["simd"] }
# C1: engine alone.
engine1 = "registry:fountain_engine@2.1.0"
# C2: operator alone (requires your local checkout).
operator2 = "path:../fountain-arena-operator#MemoryDataOperator"
# C3: another complete combination.
engine3 = "path:../erasure_coding_lib/fountain_engine"
operator3 = { source = "path:../erasure_coding_lib/fountain_operators#SimdDataOperator", features = ["simd"] }
scheme = "rq"
test = "0.1.0"
```

Indices are nonnegative integers, may be sparse, and execute in numeric order. There is no implicit pairing across indices and no cross-product. Every C receives the complete T; unsupported cases stay visible with a reason. Engine-only runs test standalone plan generation/export. Operator-only runs test the pure-operator surface. The missing production component is never added to enable codec tests. Supporting trait dependencies and the engine-only scalar validation helper are recorded separately from selected components. A product engine alone currently has no exposed standalone planning API and is reported unsupported with explicit skipped cases.

One C is also written with indexed keys (`engine0` and/or `operator0`). Unindexed `engine`/`operator` fields are rejected. All Cs share the scheme and test version. To test another scheme, use another config.

## Component references

Engine, operator and explicit scheme sources use the same grammar:

| Form | Example |
|---|---|
| Local package directory or manifest | `path:../engine` |
| Exact registry version | `registry:fountain_engine@2.1.0` |
| Fixed Git commit and package | `git+https://github.com/shhyang/fountain_operators.git?package=fountain_operators&rev=ce232222603876f3c033109c548cf1091e874c10#SlabDataOperator` |

Git revisions must be full commit IDs. Credentials do not belong in reference URLs. Every operator reference ends in `#` followed by a public, crate-relative Rust type path, such as `#SlabDataOperator` or `#ops::CustomOperator`. The generated driver uses that type directly. The type must provide `new(width: usize)` and implement the `DataOperator` trait used in the selected build. Build errors identify missing exports, constructors or incompatible traits. Engine references select a crate. Scheme implementation selection uses `scheme.name` and the optional `scheme.source`.

Use an inline table when an engine or operator needs Cargo features:

```toml
engine0 = "registry:fountain_engine@2.1.0"
operator0 = { source = "registry:fountain_operators@2.0.0#SimdDataOperator", features = ["simd"], default-features = false }
scheme = "rq"
test = "0.1.0"
```

`source` accepts the same reference syntax as the string form. `features` defaults to an empty list, and `default-features` defaults to `false`. Set `default-features = true` to request the crate's default feature set. Cargo may also enable features through other dependencies. The report index records the requested options and Cargo's resolved features for the measurement and diagnostic builds. Diagnostic-only features requested by an interface are applied to its diagnostic build.

The Slab examples request `simd` to preserve the feature set used by earlier builds. To migrate an existing configuration, use `#SlabDataOperator`, `#SimdDataOperator`, or `#MemoryDataOperator` in the matching operator reference and specify its desired features explicitly.

Local inputs capture tracked and nonignored untracked modifications, deletions, workspace inheritance and required path dependencies. `prepare` freezes their actual bytes; later worktree changes do not alter prepared execution. Upstream files and version constraints are never rewritten. Required path bindings occur only in the generated build view, with original and effective manifests retained.

A local development configuration can be:

```toml
engine0 = "path:../erasure_coding_lib/fountain_engine"
operator0 = { source = "path:../erasure_coding_lib/fountain_operators#SimdDataOperator", features = ["simd"] }
scheme = "r10"
test = "0.1.0"
```

Local, Git and registry inputs are source choices. They do not create different user-facing test modes. Rust API and product ABI are the invocation boundaries; I selects the registered integration and validates it by compilation and probing.

## Scheme selection and provenance

The current integrations support `rq` and `r10` in `systematic` mode. `scheme = "rq"` is shorthand for the default source and that mode. Unknown names/modes are rejected explicitly, rather than replaced with another scheme.

Default sources are deterministic: public engine 2.1 uses registered `fountain_scheme` and `fountain_utility` 2.1.0; development engine 3 uses its monorepo packages. An explicit scheme source replaces only the selected scheme package:

```toml
engine0 = "registry:fountain_engine@2.1.0"
operator0 = { source = "registry:fountain_operators@2.0.0#SlabDataOperator", features = ["simd"] }
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
operator0 = { source = "path:../erasure_coding_lib/fountain_operators#SlabDataOperator", features = ["simd"] }
scheme = "rq"
test = "0.1.0"
```

The product uses its bundled scheme; an explicit external `scheme.source` is rejected. Binary and callback dependencies, hashes and loader evidence are recorded separately. Arena can be used as a product callback operator, while its current exact engine 2.1 dependency conflicts with source engine 3.

Product runs execute codec, source-replacement and lifecycle checks through the real shared library and the selected SymbolOps callbacks. Full operator cases and standalone plan/replay cases require surfaces the product ABI does not provide and are explicitly skipped. Product resource observations cover the driver/callback allocator, not allocations inside the dynamic library. The registered R10 scheme accepts K=4..8192 on every integration; K=1 success cases are explicitly outside-domain skips. Product V11 checks below-domain rejection through its recoverable API.

## Effective settings and prepared execution

Test content derives K, symbol width, repair budget, seeds, ordering, warmup/repetition counts, severity and timing. The config selects component build features alongside component references. I derives the native bindings and diagnostic build requirements. E captures actual common build and runtime conditions.

Storage paths are fixed by the workspace. The original config and its resolved settings are retained in the result and report index. Their location and comments do not affect experiment equality.

`prepare` leaves a pending workspace and its frozen cache, without publishing a final report. Its JSON response contains `prepared` and `batch`. Prepared execution consumes those inputs without rereading the original config:

```bash
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["prepared"])')
feval run --prepared "$PREPARED"
```

A pending preparation can be consumed once. `run --prepared` writes a normal result under `results/`. Generating its report removes both the run and consumed preparation data by default. Prepared binaries require the original CPU and OS family. `--prepared` and `--config` are mutually exclusive.

## What the formal content runs

`feval list` expands the complete 2,511-case candidate. The selected scheme and verified interface determine applicability; users cannot suppress cases.

| Category | Coverage | Measurement |
|---|---|---|
| Validation tests (V01–V11) | Scalar GF/byte oracle, mixed prefixes, storage/dispatch, boundary observations, scheme padding, plan reuse, small independent linear solve, receive ordering, participation and release | Validation outside ordinary timers |
| Operation tests (O01–O09) | Public storage, XOR, GF, copy/move and supported alias calls; direct/execute paths; small/large sets | One prepared call per independent instance |
| Link tests (L01–L12) | Symbolic plan construction and operation-list cloning, actual encode/decode stage partitions, recorded real-session replay, operation mix and allocation diagnostics | Ordinary totals and separate instrumented observations |
| Workflow tests (W01–W08) | First use, source/repair output, source replacement, all-source and missing-source decode, round trip, 32-block continuity and lifecycle | Directly measured output-ready/lifecycle durations |

The main matrix uses K=64/512/5008 and symbol widths 128/1016. Ordinary cases run in three sequential worker processes with one excluded warmup and five samples each (15 recorded samples). First-use cases use 15 fresh processes. Diagnostic builds have separate binaries and samples. Timers exclude checking and serialization; output-ready and full-lifecycle scopes are explicitly distinguished. No performance threshold determines correctness.

The pinned public Slab/Simd implementations have a `mul_add(src, 0, dst)` branch that clears `dst`. The independent mathematical reference expects `dst` unchanged. A run that detects this ordinary GF error fails before measurement; it is not converted into a self-XOR warning or silently skipped. Other independent V cases still execute; O/L/W remain unexecuted after V completes. This is a component result, not a reason to weaken the formal contract.

Checks emit explicit contract IDs, violation kinds, structured context and exact differences. Report grouping uses these identities; diagnostic wording is only supplementary. Older evidence without these fields is shown as unclassified and kept separate. A warning retains expected/actual data, operation, width, dispatch and measured observations. Warnings do not fail correctness, suppress performance or veto comparison. A faster implementation returning immediately is still timed and its deviation is reported. Developers decide the tradeoff. `correctness.status: passed_with_warnings` can accompany `status: succeeded` and exit 0.

Crashes, timeouts, build conflicts, malformed evidence and missing applicable results remain failures. They are never converted to capability skips. Unknown I/T or scheme selections are explicit unsupported errors.

## Reading results and comparing

`feval run` seals the entire batch under `results/<result-id>/` and prints the ID. `feval report <result-id>` publishes **one directory for the entire batch** at `reports/<result-id>/` in the active workspace. It contains `report.md` for reading and `index.json.lzma` for comparison. After verified publication, it removes the raw result and its private build cache. Use `--keep` to retain that data, including any consumed preparation. While the raw result is available, repeating the command verifies and reuses an existing report from the same sealed run.

Result IDs use the UTC start time in `YYYY-MM-DD_HH-MM-SSZ` format. A collision adds a numeric suffix starting at `-2`. The report uses the same ID.

```bash
feval run --json > run.json
RESULT=$(python3 -c 'import json; print(json.load(open("run.json"))["result"])')
feval report "$RESULT" --keep --json > report-path.json
REPORT=$(python3 -c 'import json; print(json.load(open("report-path.json"))["report"])')
feval report "$REPORT" --json
feval compare "$REPORT#C0" /path/to/candidate-report#C0 > comparison.md
```

The `run --json` response contains `result` and `batch`. Use the printed ID even when a run exits with a component failure. `report <result-id> --json` returns the report directory as `report`, the Markdown path as `markdown`, and `result_id`. Pass a published report directory to display the report or read its stored index with `--json`. The original `report.md` path also works with the index beside it. Relative report and comparison paths are resolved from the active workspace root. Multi-C comparisons use the `#C<N>` selector. Single-C reports can be passed directly.

The report starts with tables for the test configuration, component and interface versions, recorded environment, and results. Validation (V), Operation (O), Link (L) and Workflow (W) follow in order. V starts with its correctness summary, then Fatal and Warning findings grouped by the check-emitted `contract_id` and `failure_kind`, and ends with the complete coverage and V01–V11 family tables. Each problem summarizes the affected components, families and case counts, with one representative fatal diagnostic; individual cases remain in the index. Empty finding types show None detected. The O/L/W categories have their own family coverage and performance details. Comparable workloads show component results side by side. A dagger marks benchmark timing CV above 10%.

Detailed coverage counts and exclusion reasons appear in V and come from the per-case records. The report separates accepted measurements, partial sample sets, complete sets rejected with their run, unexecuted cases and skips. A case with a complete sample set can contain incomplete recovery attempts. Its row shows the recovery count and omits successful throughput. Family coverage and skip reasons remain visible for single-component and capability-limited selections.

`report.md` contains plain Markdown. Share it by itself for reading, or copy the entire report directory to retain `compare` and `report --json` access. The adjacent index records the config, C/I/T/E, test plan, build provenance, complete per-case coverage, correctness findings, warning observations, aggregate timings and diagnostics. The loader checks the Markdown and index as a pair. Earlier reports with an embedded index remain readable.

`compare` verifies T/E/I and common fatal-check witnesses before comparing common measured cases. Failed runs are ineligible; semantic warnings remain visible and eligible. Its headline rows show the baseline, candidate, median time change and CV, with observed changes in input, work and warnings flagged per case. Comparison uses the aggregate statistics validated before cleanup; it cannot recalculate statistics from disposed samples. Existing per-C raw evidence directories are still readable. A finalized report is a result, not a replayable preparation.

## Evidence, failures and retention

Each run retains its config, plan, provenance, validation, samples, diagnostics and subprocess logs. This includes completed prefixes from failed, unsupported and interrupted batches. Their report preserves outcome, errors, warnings and exact coverage. Large byte-array assertions retain lengths, the first differing offset and values, the total number of differences, short prefixes and SHA-256 hashes.

`report` verifies the sealed result and reads back both published files before cleaning raw data. A publication or verification error retains the raw result. A cleanup error identifies the affected paths. Cleanup uses result ownership and locks so other runs and preparations can continue. `clean` and explicit directory reset require exclusive access to the workspace. Keep the result with `--keep` when you need its individual samples or full logs.

| Exit | Meaning |
|---|---|
| 0 | Inspection, preparation or execution completed; review warnings and skips |
| 2 | Invalid configuration/CLI, unreadable evidence or rejected comparison |
| 3 | At least one unsupported C, with no failed/interrupted C |
| 4 | At least one failed or unexecuted C; inspect every batch entry |
| 130 | Interrupted run |

Start failures with the component outcome, fatal findings and diagnostic excerpts in the report index. A preparation with a corrupt cache fails explicitly. Execution and publication are local operations. The verified report directory survives cleanup.

## Maintainer references

See the [0.1.0 test guide](docs/TEST.md), [DESIGN and implementation map](docs/DESIGN.md), [CONTRIBUTING](CONTRIBUTING.md) and [artifact contracts](specs/README.md). Reports are produced by the CLI; historical construction summaries are not the source of component results.
