# fountain_eval

`fountain-eval` resolves and freezes Fountain components, builds an isolated native driver, checks correctness, records acceptance measurements, and compares compatible results. It is **one Cargo package** with a library and CLI. Component source code remains in its own repository; local snapshots and generated drivers live in the ignored cache.

A normal request has three inputs: **engine**, **operator**, and **test version**. Arena, Slab and Simd are operator implementations. The framework chooses the interface integration (I), resolves supporting schemes/utilities, and generates the effective experiment configuration (E).

The registered tests are **acceptance tests for this framework**, not a formal performance standard. They establish functioning integration and measurement boundaries. Three short samples do not establish optimization gains, sustained throughput or production stability.

## Install

Requirements: Linux or macOS, Git, `tar`, a native C linker/toolchain, and Rust managed by rustup. The acceptance recipes use Rust **1.89.0** and release native drivers. On macOS install the Xcode command-line tools; on Linux install the platform's standard build tools.

```bash
git clone https://github.com/Gideon-0709/fountain_eval.git
cd fountain_eval
rustup toolchain install 1.89.0 --profile minimal --component rustfmt --component clippy --no-self-update
cargo install --path . --locked
fountain-eval --help
fountain-eval list
```

Private repositories require your existing Git credentials. Do not put tokens in component references. Installation embeds all driver templates and the process protocol; the installed CLI can run from a separate working directory without the eval checkout or a user-written Cargo manifest. Registry publication remains disabled pending a separate release decision.

## First real run

From an empty working directory:

```bash
fountain-eval run \
  --engine registry:fountain_engine@2.1.0 \
  --operator registry:fountain_operators@2.0.0#slab \
  --test codec-smoke@0.1.0 > run.json
```

The command resolves sources, freezes them, builds the driver, checks its identity, runs correctness validation, performs one warmup and three recorded repetitions, and writes a unique directory under `runs/`. Its JSON output includes `run_dir`, status, the four-tuple, validation, raw samples and summaries. The default cache is `.fountain-eval/`.

Read and compare results using the returned directories:

```bash
RUN_A=$(python3 -c 'import json; print(json.load(open("run.json"))["run_dir"])')
fountain-eval report "$RUN_A"

fountain-eval run \
  --engine registry:fountain_engine@2.1.0 \
  --operator registry:fountain_operators@2.0.0#simd \
  --test codec-smoke@0.1.0 > simd.json
RUN_B=$(python3 -c 'import json; print(json.load(open("simd.json"))["run_dir"])')
fountain-eval compare "$RUN_A" "$RUN_B"
```

These are acceptance timings. `source_payload_gbps` is original source bytes × 8 / elapsed nanoseconds: decimal Gbit/s, not output wire rate. Both encoder and decoder use K × symbol_bytes as the numerator. Encoder work materializes all source symbols plus 128 repair symbols. Operator results report operations/s for their fixed trace; they do not invent a payload throughput numerator.

## Component references

| Form | Example | Resolution |
|---|---|---|
| Local package directory or manifest | `path:../fountain-arena-operator` | Includes current tracked and nonignored untracked files, including uncommitted edits |
| Exact registry version | `registry:fountain_engine@2.1.0` | Verifies the downloaded crate archive against Cargo's locked checksum |
| Fixed Git commit and package name | `git+https://github.com/shhyang/fountain_operators.git?package=fountain_operators&rev=ce232222603876f3c033109c548cf1091e874c10#slab` | Fetches the complete commit ID and finds one matching package |

Git revisions must be complete 40- or 64-digit hexadecimal commit IDs. Branch names, tags and abbreviated hashes are rejected. HTTPS and SSH URLs are supported; HTTPS credentials and embedded passwords are rejected. Quote Git references because they contain `&`. Use `#slab` or `#simd` for `fountain_operators`; Arena's package has an unambiguous implementation and accepts an omitted selector or `#arena`. Engine implementation selectors are not registered.

Different forms can be mixed. Package version, source content and dependency provenance all contribute to C. A branch checkout's dirty working bytes are captured for a path reference; a Git reference always selects its specified commit.

## Interface families

| Automatically selected I | Engine selection | Operators | Support dependencies |
|---|---|---|---|
| `native-2@0.1.0` | `fountain_engine` 2.1.x | Arena, Slab, Simd when compilation/validation succeeds | Registered scheme 2.1.0 and utility 2.1.0 |
| `native-3@0.1.0` | Development `fountain_engine` 3.x | Current monorepo Slab/Simd | Matching monorepo scheme/utility |
| `product-abi@0.1.0` | Monorepo `fountain_bin_ffi` package | Arena, Slab, Simd through SymbolOps | Product facades, sys bindings and binary source from the selected monorepo |

The current Arena pins engine 2.1.0 and is rejected against source engine 3. The product path is different: the dynamic library uses its own engine graph, while Arena callbacks can use engine 2.1 in the separate caller graph. Both graphs, lockfiles, artifacts and the loaded library location are recorded.

The product driver builds and loads the real shared library; it does not substitute a source-engine benchmark. Full operator operations are not exposed through the product API, so `operator-smoke` is explicitly unsupported there. A new precompiled-binary reference format is outside this release.

Examples with already available private checkouts:

```bash
fountain-eval run \
  --engine path:../erasure_coding_lib/fountain_engine \
  --operator 'path:../erasure_coding_lib/fountain_operators#slab' \
  --test operator-smoke@0.1.0

fountain-eval run \
  --engine path:../erasure_coding_lib/fountain_bin_ffi \
  --operator path:../fountain-arena-operator \
  --test codec-smoke@0.1.0
```

The registered development/product mappings use the existing monorepo layout. Layout, package or trait changes may require an I update; users do not maintain a parallel build manifest.

## Inspect, prepare, and replay frozen input

`inspect` is offline declaration inspection. It never downloads, compiles or claims native correctness. Its `execution_available: false` means execution was not established by inspection. `list` displays the registered I/T versions and content digests.

```bash
fountain-eval inspect \
  --engine registry:fountain_engine@2.1.0 \
  --operator path:../fountain-arena-operator \
  --test operator-smoke@0.1.0

fountain-eval prepare \
  --engine registry:fountain_engine@2.1.0 \
  --operator registry:fountain_operators@2.0.0#slab \
  --test codec-smoke@0.1.0 > prepared.json
PREPARED=$(python3 -c 'import json; print(json.load(open("prepared.json"))["run_dir"])')
fountain-eval run --prepared "$PREPARED"
```

`prepare` ends after source resolution, snapshots, build and capability/loader checks. It does not claim validation or measurements. `run --prepared` checks the original build's integrity and executes it without reading the original component worktrees. Editing those worktrees cannot change that prepared run. Each replay creates a new result directory. Native binaries require the original CPU and OS family; current runtime conditions are observed again.

`--cache-dir PATH` and `--output PATH` are optional storage settings for new requests. `--prepared` selects its original cache, so it cannot be combined with new component/test inputs or `--cache-dir`. Use a separate evaluation working directory rather than placing custom result directories inside component sources.

## What is fixed by T

| Acceptance test | Workload | Correctness | Measured boundary |
|---|---|---|---|
| `operator-smoke@0.1.0` | Widths 1, 31 and 128; a 14-operation trace through execute and direct calls | Scalar GF(256) oracle, every trace prefix, copy isolation, move, zeroing, XOR and multiply-add | Preloaded operation execution only |
| `codec-smoke@0.1.0` | RQ/R10; K=64, 128-byte symbols; no loss or first 8 source symbols missing; fixed order, up to 128 repairs | Systematic bytes, recovered payload bytes, incomplete state, selected operator participation and release | Construction and input loading through owned output materialization |

Both use one warmup and three recorded repetitions. Input generation, validation, serialization and final resource release are outside measured intervals. Callback counters are enabled only in correctness runs; measured codec runs use the uninstrumented operator. No performance threshold determines success.

**Deferred contract:** same-ID arithmetic, including self-XOR, is excluded while the team decides its semantics. Current Arena and Slab/Simd differ on self-XOR. This is a documented coverage boundary, not a passed equivalence claim. Distinct-ID cancellation and copy/write isolation remain checked.

## Reading evidence and comparability

Each run stores:

- `request.json`, `sources.json`, and `configuration.json` for the requested and resolved inputs;
- `result.json` with optional `(C,I,T,E)`, status, validation, raw samples and derived summaries;
- `stages.json`, distinguishing succeeded, running, failed, unsupported, interrupted and not-executed stages;
- `build.json`, `bindings.json`, `dependency-locks.json`, `dependency-graphs.json` and copied build logs;
- live child stdout/stderr, `validation.json`, `samples.json`, and product loader evidence when applicable;
- `evaluator.json` for framework/binary provenance and `seal.json` for evidence checksums.

Failures retain completed stages and logs. Unfinished measurements can retain a raw sample prefix, but never a successful aggregate report. Early failures have no complete four-tuple. Report and comparison need the result directory, not its build cache.

`compare` requires successful runs, equal T version/content, equal E conditions, valid samples, registered I support for that T, and matching independently checked correctness witnesses. Time stamps, run IDs, cache paths and observations such as uptime are excluded from E equality. Component-specific dependencies, features and binary hashes remain provenance rather than common environment fields.

Different I versions can be comparable under the same T/E when their registered semantic/timing contract and runtime witnesses agree. If C and I both differ, attribution explicitly includes both. A comparison is rejected with a reason when content identity, environment, correctness or completeness differs. Equal E does not remove ordinary runtime noise; acceptance measurements are not a statistical performance study.

## Failures and cache handling

| Exit | Meaning |
|---|---|
| 0 | Successful command, prepared input, or successful run; inspect still remains inspection-only |
| 2 | Invalid CLI/reference, unreadable evidence, or rejected comparison |
| 3 | Unsupported test, interface selection or capability |
| 4 | Resolution, dependency, build, correctness, timeout, cache or protocol failure during a run |
| 130 | Interrupted run |

Start with `result.json.error`, `stages.json`, then the referenced log. A source-engine version conflict is diagnosed before measurement; a shape mismatch can fail compilation; wrong results fail validation. Unknown/ambiguous registrations are not guessed.

Completed cached builds are verified before reuse. A corrupt or incomplete cache is rejected; the framework does not silently rebuild or retry it into a pass. Preserve the failed result, then explicitly choose a new cache directory or remove the identified failed cache entry if you want another attempt. Directory source symlinks, escaping file symlinks, and unmapped workspace `[patch]`/`[replace]` declarations are rejected explicitly.

Normal results and caches are local and ignored by Git. The CLI never uploads, archives, overwrites previous runs or deletes results. You manage retention explicitly. Prepared replay requires its cache; report/compare remain usable after cache deletion.

## Maintainer checks

See [the implementation acceptance summary](docs/VALIDATION.md), [CONTRIBUTING.md](CONTRIBUTING.md), [DESIGN.md](DESIGN.md), [interface and artifact contract](specs/README.md), and [implementation map](docs/modules.md).

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo doc --locked --no-deps
python3 scripts/check_repository.py
cargo build --locked
python3 scripts/native_acceptance.py --work .fountain-eval/public-acceptance --faults
```

The acceptance script requires a fresh work directory, records every attempt, stops on unexpected failure, and does not clean or upload its output. Add `--monorepo PATH --arena PATH` to cover all private integrations on an authorized local machine. CI runs framework checks on Linux x86_64, Linux ARM64 and macOS. Public native positive cases run on Linux ARM64 and macOS; the pinned public AVX2 code currently fails compilation on x86_64, where CI verifies explicit rejection instead. This is an upstream component limitation, not a passing native evaluation. Private coverage is local and reported separately.
