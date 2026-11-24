# fountain_eval

`feval` freezes Fountain components, generates isolated native builds, executes every applicable check, records measurements and compares results. The `fountain-eval` Cargo package provides the `fountain_eval` library and the `feval` CLI. Component source remains in its own repository.

The project is an unreleased internal **0.1.0** candidate.

## Install and initialize a workspace

Install Git, `tar`, a native linker and C toolchain, and Rust via rustup on Linux or macOS. The generated drivers use Rust 1.89.0.

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

Set `test` to a test content version. Use `feval list` to inspect the catalog. See [TEST](docs/TEST.md) for the test plan. Each configured selection runs its applicable checks. If validation finds a fatal error for one selection, performance tests for that selection stop and the batch continues with the next selection.

`inspect`, `prepare` and `run` use the active workspace's `fountain-eval.toml`. Use `--config` for another file inside that workspace. A relative `--config` is resolved from the workspace root. Local component paths are always relative to the selected config's directory.

```bash
feval inspect --config alternatives/r10.toml
feval prepare --config alternatives/r10.toml --json > prepared.json
feval run --config alternatives/r10.toml
```

Unknown fields, duplicate keys and malformed references are configuration errors. When moving an older config into a workspace, remove its `[run]` section. The workspace determines all output paths.

## Terminal progress

`run` and `prepare` write progress to stderr. Each component selection has its own heading. A run shows preparation, validation, performance and diagnostics. `prepare` stops after preparation.

Interactive terminals refresh a status line. Redirected stderr includes milestones and periodic updates during long stages. The final summary gives each selection's outcome and identifies the result directory. Set `NO_COLOR` to disable color.

For automation, add `--json` to keep the structured final response on stdout while progress remains on stderr:

```bash
feval run --json > run.json
feval prepare --json > prepared.json
# Optionally retain terminal progress in a separate log:
feval run --json > run.json 2> progress.log
```

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

The product uses its bundled scheme; an explicit external `scheme.source` is rejected. Binary and callback dependencies, hashes and loader evidence are recorded separately. Select a callback operator compatible with the product's registered trait dependency.

Product runs execute codec, source-replacement and lifecycle checks through the real shared library and the selected SymbolOps callbacks. Full operator cases and standalone plan/replay cases require surfaces the product ABI does not provide and are explicitly skipped. Product resource observations cover the driver/callback allocator, not allocations inside the dynamic library. The registered R10 scheme accepts K=4..8192 on every integration; K=1 success cases are explicitly outside-domain skips. Product V11 checks below-domain rejection through its recoverable API.

## Prepared execution

`prepare` leaves a pending workspace and its frozen cache, without publishing a final report. Its JSON response contains `prepared` and `batch`. Prepared execution consumes those inputs without rereading the original config:

```bash
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["prepared"])')
feval run --prepared "$PREPARED"
```

A pending preparation can be consumed once. `run --prepared` writes a normal result under `results/`. Generating its report removes both the run and consumed preparation data by default. Prepared binaries require the original CPU and OS family. `--prepared` and `--config` are mutually exclusive.

## Generate and read a report

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

The `run --json` response contains `result` and `batch`. Use the printed ID even when a run exits with a component failure. `report <result-id> --json` returns the report directory as `report`, the Markdown path as `markdown`, and `result_id`. Pass a published report directory to display the report or read its stored index with `--json`. The original `report.md` path also works with the index beside it. Relative report and comparison paths are resolved from the active workspace root.

The Markdown report opens with the batch summary and contains separate V, O, L and W sections. Share `report.md` for reading, or copy the entire report directory to retain the index for `compare` and `report --json`.

### Comparing runs

Select one component per report with `#C<N>` when a report contains several selections. Compare two successful results that meet the [compatibility requirements](docs/DESIGN.md#comparability-and-attribution). The command outputs Markdown by default and supports `--json`.

## Exit codes and troubleshooting

| Exit | Meaning |
|---|---|
| 0 | Inspection, preparation or execution completed; review warnings and skips |
| 2 | Invalid configuration/CLI, unreadable evidence or rejected comparison |
| 3 | At least one unsupported C, with no failed/interrupted C |
| 4 | At least one failed or unexecuted C; inspect every batch entry |
| 130 | Interrupted run |

For a component failure, generate the report and inspect that selection's outcome and findings. Use `feval report <result-id> --keep` when you need individual samples or subprocess logs.

## Run from source

From the repository checkout, run a CLI command without installing it by passing the command after Cargo's `--`.

```bash
cargo run --locked --bin feval -- --help
cargo doc --locked --no-deps
```

The generated library documentation is at `target/doc/fountain_eval/index.html`. See [CONTRIBUTING](CONTRIBUTING.md) for development checks, [DESIGN](docs/DESIGN.md) for the architecture and implementation map, and [TEST](docs/TEST.md) for the test plan.
