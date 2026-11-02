# fountain_eval

`fountain-eval` freezes Fountain components, generates isolated native builds, executes every applicable check, records measurements and compares results. It is one Cargo package with a library and CLI. Component source remains in its own repository.

The project is an unreleased internal **0.1.0** candidate. The current test content consists of **framework acceptance fixtures**, not the team's formal correctness or performance standard. Formal tests will cover correctness, individual operations, execution chains and integrated workflows. Encoding-plan generation belongs to chain testing; complete encoding with plan reuse belongs to integrated testing. Those formal workloads are not implemented by this configuration change.

## Install and run

Requirements: Linux or macOS, Git, `tar`, a native linker/C toolchain and Rust managed by rustup. The generated acceptance drivers use Rust 1.89.0, release optimization and the host CPU. Public native positive coverage currently uses ARM64: the pinned upstream operator has an AVX2 compile error on x86_64, which is reported as a build failure.

```bash
cargo install --path . --locked
cp fountain-eval.example.toml fountain-eval.toml
fountain-eval inspect
fountain-eval run > run.json
```

An installed CLI works without the eval checkout. Create a `fountain-eval.toml` containing four entries:

```toml
engine = "registry:fountain_engine@2.1.0"
operator = "registry:fountain_operators@2.0.0#slab"
scheme = "rq"
test = "0.1.0"
```

`test` selects the **entire content version**. Users cannot select individual cases or disable checks. Every registered case is considered. Inapplicable cases remain in the report with a machine-readable reason and explanation. Fatal failures stop execution; later cases remain `not_executed`, not `skipped`.

Commands default to `fountain-eval.toml` in the current directory. An explicit location works from any directory:

```bash
fountain-eval inspect --config /path/to/experiment.toml
fountain-eval prepare --config /path/to/experiment.toml > prepared.json
fountain-eval run --config /path/to/experiment.toml > run.json
```

All relative paths are relative to the config file. There are no parent-directory searches, includes, environment interpolation, hidden global configuration or CLI overrides for component/test settings. Unknown fields, misspellings, duplicate keys and malformed references are errors. `--engine`, `--operator` and per-test `--test` flags have been removed.

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
engine = "path:../erasure_coding_lib/fountain_engine"
operator = "path:../erasure_coding_lib/fountain_operators#simd"
scheme = "r10"
test = "0.1.0"
```

Local, Git and registry inputs are source choices. They do not create different user-facing test modes. Rust API and product ABI are the invocation boundaries; I selects the registered integration and validates it by compilation and probing.

## Scheme selection and provenance

The current integrations support `rq` and `r10` in `systematic` mode. `scheme = "rq"` is shorthand for the default source and that mode. Unknown names/modes are rejected explicitly, rather than replaced with another scheme.

Default sources are deterministic: public engine 2.1 uses registered `fountain_scheme` and `fountain_utility` 2.1.0; development engine 3 uses its monorepo packages. An explicit scheme source replaces only the selected scheme package:

```toml
engine = "registry:fountain_engine@2.1.0"
operator = "registry:fountain_operators@2.0.0#slab"
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
engine = "path:../erasure_coding_lib/fountain_bin_ffi"
operator = "path:../erasure_coding_lib/fountain_operators#slab"
scheme = "rq"
test = "0.1.0"
```

The product uses its bundled scheme; an explicit external `scheme.source` is rejected. Binary and callback dependencies, hashes and loader evidence are recorded separately. Arena can be used as a product callback operator, while its current exact engine 2.1 dependency conflicts with source engine 3.

Product runs execute applicable codec checks and callback boundary diagnostics. Pure operator core cases and boundary timings require an operation surface the product ABI does not provide: these are explicit capability skips. A skip is not a passing test. Callback diagnostics identify their scope as `callback-operator`.

## Effective settings and prepared execution

Test content derives K, symbol width, repair budget, seeds, ordering, warmup/repetition counts, severity and timing. This draft has no arbitrary parameter, severity, feature or per-case skip overrides. I derives native bindings/features. E captures actual common build and runtime conditions.

Optional storage settings belong in the same file:

```toml
[run]
output = "runs"
cache = ".fountain-eval"
```

The defaults are the same paths relative to the config. Every invocation creates a unique result directory. The original config and its resolved request/settings are saved as evidence. Their location and comments do not affect experiment equality.

Prepared execution consumes the stored config and verified source/build inputs, without rereading the original config:

```bash
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["run_dir"])')
fountain-eval run --prepared "$PREPARED" > run.json
```

Its output directory comes from the frozen configuration. Current runtime conditions are observed again; the prepared native binary requires its original CPU and OS family. `--prepared` cannot be combined with `--config`.

## What the current acceptance content runs

`fountain-eval list` lists the full catalog. Internal content version `0.1.0` contains 26 cases:

| Registered cases | Validation | Measurement |
|---|---|---|
| 6 operator core cases: widths 1/31/128, execute/direct | Fatal scalar-reference checks for the 14-operation trace | Preloaded operation execution |
| 12 boundary cases: self-XOR and repeated-source XOR at each width/path | Warning-level reference observations | 256 independent preloaded calls per sample on native APIs |
| 8 codec cases: RQ/R10, 0/8 missing sources, encode/decode | Fatal payload, incomplete-state, participation and release checks | Construction and input loading through owned output |

The selected scheme enables four codec cases; the four cases for the other scheme are recorded as `scheme_not_selected`. Native runs validate 22 cases and record 66 samples. Product runs validate 16 cases and record 12 samples, with 10 validation skips and 22 measurement skips. Every measured case has one warmup and three recorded repetitions. Validation, serialization and final destruction are outside timers. These brief timings do not establish a performance ranking.

A warning retains expected/actual data, operation, width, dispatch and measured observations. Warnings do not fail correctness, suppress performance or veto comparison. A faster implementation returning immediately is still timed and its deviation is reported. Developers decide the tradeoff. `correctness.status: passed_with_warnings` can accompany `status: succeeded` and exit 0.

Crashes, timeouts, build conflicts, malformed evidence and missing applicable results remain failures. They are never converted to capability skips. Unknown I/T or scheme selections are explicit unsupported errors.

## Reading results and comparing

```bash
RUN=$(python3 -c 'import json; print(json.load(open("run.json"))["run_dir"])')
fountain-eval report "$RUN"
fountain-eval compare /path/to/baseline-run /path/to/candidate-run > comparison.json
```

`report` returns the complete JSON record. Read these fields first:

- `status`, `stage`, `error`: execution outcome and failure boundary.
- `correctness`: fatal failures, warning observations and executed check counts.
- `coverage`: every registered case's validation/measurement status and skip reason.
- `summary`: measured nanoseconds, operations/s and source-payload Gbit/s.
- `quadruple`: fixed C/I/T/E identities; absent for early failures.

A successful run can have `coverage.status: completed_with_skips`. Review the exact skipped cases; this does not assert that the whole catalog was executed. Failed runs retain `not_executed` cases and any raw sample prefix without a successful aggregate.

```bash
python3 - <<'PYREPORT'
import json
r = json.load(open("run.json"))["result"]
print(r["status"], r["correctness"]["status"], r["coverage"]["status"])
for case in r["coverage"]["cases"]:
    for phase in ("validation", "measurement"):
        if case[phase]["status"] == "skipped":
            print(case["case"], phase, case[phase]["reason"])
for warning in r["correctness"]["warnings"]:
    print("warning", warning["case"], warning["expected"], warning["actual"])
PYREPORT
```

Comparison verifies every applicable observation against the registered plan before comparing common measured cases. It requires matching T/content and common E conditions, valid I integrations and matching common fatal witnesses. It returns both coverage tables and explicitly labels its scope; product/native comparisons contain codec timing rows, not fabricated operator rows. Warning differences remain visible. A change to C and I together is attributed to both.

## Evidence, failures and retention

Each result includes `config.toml`, `config-resolved.json`, `request.json`, `test-plan.json`, `coverage.json`, `correctness.json`, `configuration.json`, source/build/dependency provenance, raw validation/samples, subprocess logs, `result.json` and a checksum seal. Product runs also retain library loader evidence. `report` and `compare` work after build-cache removal. Prepared execution still requires its verified cache.

| Exit | Meaning |
|---|---|
| 0 | Inspection, preparation or execution completed; review warnings and skips |
| 2 | Invalid configuration/CLI, unreadable evidence or rejected comparison |
| 3 | Unsupported content, scheme or interface |
| 4 | Resolution, dependency, build, fatal correctness, timeout, cache or protocol failure |
| 130 | Interrupted run |

Start failures with `error`, `stages.json` and the referenced log. Incomplete/corrupt build entries fail without rebuilding or retrying silently. Use a new cache directory or explicitly remove a failed entry after inspection. The CLI never uploads, archives, overwrites or deletes normal results. Default results/caches are ignored by Git; custom storage outside those directories is the user's responsibility.

## Maintainer references

See the [0.1.0 formal test plan and report guide (review draft)](docs/TEST_PLAN_0.1.0.md), [DESIGN](DESIGN.md), [CONTRIBUTING](CONTRIBUTING.md), [artifact contracts](specs/README.md), [implementation map](docs/modules.md) and [configuration acceptance](docs/VALIDATION_CONFIG.md). Earlier [initial acceptance](docs/VALIDATION.md) and [warning-policy acceptance](docs/VALIDATION_WARNINGS.md) describe historical construction candidates.
