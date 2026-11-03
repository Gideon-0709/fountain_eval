# fountain_eval

`fountain-eval` freezes Fountain components, generates isolated native builds, executes every applicable check, records measurements and compares results. It is one Cargo package with a library and CLI. Component source remains in its own repository.

The project is an unreleased internal **0.1.0** candidate. Its formal content covers correctness, individual operations, execution chains and integrated workflows. Encoding-plan construction/export and recorded execution belong to chain testing; plan reuse across blocks belongs to integrated testing. See the [test guide](docs/TEST_PLAN_0.1.0.md) for contracts and interpretation and the automatically generated reports for executed coverage and limitations. No release or performance ranking is implied.

## Install and run

Requirements: Linux or macOS, Git, `tar`, a native linker/C toolchain and Rust managed by rustup. The generated drivers use Rust 1.89.0, release optimization and the host CPU. Positive native construction coverage uses authorized local Arena and product checkouts on ARM64. Public Slab/Simd currently fail the formal GF check; the pinned operator also has an AVX2 compile error on x86_64. CI verifies these rejections and records the remaining coverage gaps.

```bash
cargo install --path . --locked
cp fountain-eval.example.toml fountain-eval.toml
fountain-eval inspect
fountain-eval run > run.json
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
fountain-eval inspect --config /path/to/experiment.toml
fountain-eval prepare --config /path/to/experiment.toml > prepared.json
fountain-eval run --config /path/to/experiment.toml > run.json
```

All relative paths are relative to the config file. There are no parent-directory searches, includes, environment interpolation, hidden global configuration or CLI overrides for component/test settings. Unknown fields, misspellings, duplicate keys and malformed references are errors. `--engine`, `--operator` and per-test `--test` flags have been removed.

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

Product runs execute codec, source-replacement and lifecycle checks through the real shared library and the selected SymbolOps callbacks. Full operator cases and standalone plan/replay cases require surfaces the product ABI does not provide and are explicitly skipped. Product resource observations cover the driver/callback allocator, not allocations inside the dynamic library. The registered R10 scheme accepts K=4..8192 on every integration; K=1 success cases are explicitly outside-domain skips. Product C11 checks below-domain rejection through its recoverable API.

## Effective settings and prepared execution

Test content derives K, symbol width, repair budget, seeds, ordering, warmup/repetition counts, severity and timing. This draft has no arbitrary parameter, severity, feature or per-case skip overrides. I derives native bindings/features. E captures actual common build and runtime conditions.

Optional storage settings belong in the same file:

```toml
[run]
report = "reports" # human-readable reports
output = "runs"    # sealed raw evidence
cache = ".fountain-eval"
```

The defaults are the paths above, relative to the config. Set `run.report` to the folder where you want to read reports. Every invocation reserves a unique `<report>/<run-id>/` directory and writes exactly one `report.md` containing every C. Raw evidence stays under `<output>/<run-id>/`; cache files stay under `cache`. These directories serve different purposes. Report paths do not affect E or comparability. An existing report is never overwritten. The original config and its resolved request/settings are saved as evidence. Their location and comments do not affect experiment equality.

Prepared execution consumes the stored config and verified source/build inputs, without rereading the original config:

```bash
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["run_dir"])')
fountain-eval run --prepared "$PREPARED" > run.json
```

Its output directory comes from the frozen configuration. Current runtime conditions are observed again; the prepared native binary requires its original CPU and OS family. `--prepared` cannot be combined with `--config`.

## What the formal content runs

`fountain-eval list` expands the complete 2,182-case candidate. The selected scheme and verified interface determine applicability; users cannot suppress cases.

| Category | Coverage | Measurement |
|---|---|---|
| Correctness (C01–C11) | Scalar GF/byte oracle, mixed prefixes, storage/dispatch, boundary observations, scheme padding, plan reuse, small independent linear solve, receive ordering, participation and release | Validation outside ordinary timers |
| Individual operations (O01–O09) | Public storage, XOR, GF, copy/move and supported alias calls; direct/execute paths; small/large sets | One prepared call per independent instance |
| Chains (L01–L12) | Symbolic plan construction/export, actual encode/decode stage partitions, recorded real-session replay, operation mix and allocation diagnostics | Ordinary totals and separate instrumented observations |
| Workflows (W01–W08) | First use, source/repair output, source replacement, all-source and missing-source decode, round trip, 32-block continuity and lifecycle | Directly measured output-ready/lifecycle durations |

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

The JSON command response contains `report` (the Markdown path) and `run_dir` (the sealed evidence path). No HTML/CSV reports or per-C report folders are generated. Raw machine-readable evidence is retained independently for verification and later comparisons.

```bash
python3 -c 'import json; print(json.load(open("run.json"))["report"])'
RUN=$(python3 -c 'import json; print(json.load(open("run.json"))["run_dir"])')
# Verify and render the complete report to stdout:
fountain-eval report "$RUN"
# Re-render historical evidence into a new report folder without rerunning measurements:
fountain-eval report "$RUN" --output ./review-reports
# The two arguments below are evidence directories, not Markdown files:
fountain-eval compare /path/to/batch/C0/run-id /path/to/batch/C1/run-id > comparison.md
```

`report` and `compare` produce Markdown only. `report --output DIR` creates `DIR/<run-id>/report.md` and refuses an existing destination. Report generation verifies the original evidence and does not modify or reseal it. Historical sealed results may still contain older report formats; those files remain part of their original checksum record, and the new renderer does not use them.

The report follows this reading order:

1. **Components and outcomes:** selected engine/operator, scope, outcome, elapsed wall time and the reason a C stopped.
2. **Correctness:** fatal failures explain the check, input, expected/observed behavior and impact; warnings show operation/dispatch, observed differences and repeated measurement deviations.
3. **Individual operations:** grouped by public operation, with separate short tables for latency/dispersion and work-normalized rates.
4. **Execution chains:** plan generation/export, recorded replay, stage partitions, operation mix and allocation observations.
5. **Integrated workflows:** full encode/decode, first use, reuse, continuity and lifecycle timings, with stage/resource details alongside the related workload.
6. **Coverage:** separate validation, measurement and diagnostic counts, plus skipped/unexecuted families and reasons. The linked `coverage.json` retains every exact case ID.
7. **Reproducibility:** component requests and source identities, I/T/E, host/build conditions, operational phase times and evidence locations.

All C details are embedded in the same file. Tables repeat headings every 12 rows and have at most six columns. Ordinary medians, ranges, CV and worker medians are distinct from instrumented observations. Diagnostic stage **means** use the same observation set: exclusive stage means plus the unattributed mean reproduce the mean diagnostic lifecycle, subject to rounding. They cannot be added to ordinary medians. Allocator scope and fused/unavailable internal stages are explicitly described.

A successful C can have warnings and skipped cases. Review its coverage before comparing it with a C that exercised a larger API surface. A fatal failure blocks a complete performance aggregate; raw sample prefixes remain evidence. Source Gbit/s is decimal and uses source payload bytes; repair Gbit/s uses actual repair output bytes. Incomplete recovery does not report successful throughput.

`compare` accepts two per-C evidence directories and validates T/E/I compatibility and common fatal witnesses before comparing common measured cases. It separates operations, chains and workflows, shows absolute/relative time changes and dispersion, and includes coverage differences and semantic warnings. A change to C and I together is attributed to both. No baseline or global ranking is silently selected.

## Evidence, failures and retention

Each result includes `config.toml`, `config-resolved.json`, `request.json`, `test-plan.json`, `coverage.json`, `correctness.json`, `configuration.json`, source/build/dependency provenance, raw validation/samples, subprocess logs, `result.json` and a checksum seal. Product runs also retain library loader evidence. `report` and `compare` work after build-cache removal. Prepared execution still requires its verified cache.

| Exit | Meaning |
|---|---|
| 0 | Inspection, preparation or execution completed; review warnings and skips |
| 2 | Invalid configuration/CLI, unreadable evidence or rejected comparison |
| 3 | At least one unsupported C, with no failed/interrupted C |
| 4 | At least one failed or unexecuted C; inspect every batch entry |
| 130 | Interrupted run |

Start failures with `error`, `stages.json` and the referenced log. Incomplete/corrupt build entries fail without rebuilding or retrying silently. Use a new cache directory or explicitly remove a failed entry after inspection. The CLI never uploads, archives, overwrites or deletes normal results. Default reports/results/caches are ignored by Git; custom storage outside those directories is the user's responsibility.

## Maintainer references

See the [0.1.0 test guide](docs/TEST_PLAN_0.1.0.md), [DESIGN and implementation map](docs/DESIGN.md), [CONTRIBUTING](CONTRIBUTING.md) and [artifact contracts](specs/README.md). Reports are produced by the CLI; historical construction summaries are not the source of component results.
