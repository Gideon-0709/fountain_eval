# fountain_eval

Independent evaluation infrastructure for Fountain engines and operators.

The standard request has three inputs: **an engine reference, an operator reference, and a test content version**. Arena, Slab, and Simd are operator implementations. Supporting schemes, utility code, and transitive dependencies are handled by the framework.

## Current status

This repository is a **single Rust package**, `fountain-eval`, providing a library and a CLI through internal modules.

| Available now | Planned after test content is agreed |
|---|---|
| Reference parsing and an empty catalog | Released test versions and interface implementations |
| Read-only inspection of local Cargo declarations | Source fetching, snapshots, and full dependency resolution |
| Some explicit engine version conflict checks | Isolated native-driver builds and capability checks |
| Structured inspection diagnostics | Codec correctness, timing, and comparison reports |

**No codec test or benchmark is implemented.** `run` returns unavailable; it never fabricates measurements or a passing evaluation. Successful inspection does not establish build compatibility or codec correctness.

## Build and install

Requirements: Git and Rust managed by rustup. The repository pins Rust **1.89.0**, rustfmt and Clippy. Cargo obtains the pinned toolchain and dependencies when needed. Future native component toolchains will follow the selected test recipe.

```bash
git clone https://github.com/Gideon-0709/fountain_eval.git
cd fountain_eval
cargo build --locked
cargo run --locked -- --help
cargo run --locked -- list
```

Cloning the private repository requires access through your normal Git credentials. `list` currently returns empty `interfaces` and `tests` arrays.

For a local installation:

```bash
cargo install --path . --locked
fountain-eval --help
```

Registry publication is not enabled. Local installation does not publish the package.

## Inspect a request now

This command checks reference syntax without fetching components:

```bash
cargo run --locked -- inspect \
  --engine 'registry:fountain_engine@2.1.0' \
  --operator 'git+https://github.com/Gideon-0709/fountain-arena-operator.git?rev=d777cad6b512ee17663e129512bae10bca3397b1&package=fountain-arena-operator#arena' \
  --test 'example-unregistered@0.1.0'
```

`example-unregistered@0.1.0` is deliberately an unregistered example, not a test release. Inspection accepts syntactically valid test references without implying that they exist.

JSON output contains `status: "inspection_only"`, `execution_available: false`, the request, local declarations where available, and diagnostics explaining unresolved checks. Redirect stdout to retain an inspection record. It is not a resolved four-tuple or an evaluation report.

## Component references

Shell-quote references containing spaces, `&`, or `#`.

| Form | Meaning |
|---|---|
| `path:<crate-directory>` | Read a local crate's `Cargo.toml` |
| `path:<path-to-Cargo.toml>` | Read a specific local manifest |
| `registry:<package>@<exact-version>` | Select a crates.io version; syntax only in this scaffold |
| `git+<https-or-ssh-url>?rev=<full-commit>&package=<package>` | Select a package in a pinned repository; syntax only in this scaffold |

Append `#<implementation>` to choose an implementation within a package, such as `#arena` or `#slab`. The scaffold records this selector but does not validate it against a catalog. Future drivers must reject ambiguous or unsupported selections rather than silently choose another implementation.

Rules:

- Registry and test versions require exact SemVer, such as `2.1.0` or `3.0.0-dev`, not ranges or `latest`.
- Git references require a full 40- or 64-digit hexadecimal commit ID and a package name. Inspection does not check whether that commit exists.
- Git URLs accept only `rev` and `package` query keys, each once. Embedded HTTPS credentials and URL passwords are rejected; use normal Git credentials.
- Names/selectors begin with an ASCII letter and contain letters, digits, `_`, or `-`.
- Local paths are relative to the command's working directory. The CLI does not expand `~` within a quoted reference; use an absolute path or a shell-expanded variable.
- `#` is reserved for the implementation selector in this draft format.

### Local development

With an adjacent Arena checkout:

```bash
cargo run --locked -- inspect \
  --engine 'registry:fountain_engine@2.1.0' \
  --operator 'path:../fountain-arena-operator#arena' \
  --test 'example-unregistered@0.1.0'
```

Inspection reads the operator manifest without editing it, invoking Cargo on it, or copying source. It does not freeze source contents, Git state, or the dependency graph.

### Monorepo components

Select component crate directories, not a virtual workspace root:

```bash
cargo run --locked -- inspect \
  --engine 'path:../erasure_coding_lib/fountain_engine' \
  --operator 'path:../erasure_coding_lib/fountain_operators#slab' \
  --test 'example-unregistered@0.1.0'
```

The future resolver will preserve complete monorepo layouts and relative dependencies. For Git sources, the `package` query key selects the component package.

### Explicit conflicts

If a local operator declares an unconditional, non-optional `fountain_engine = "=2.1.0"` dependency and the selected engine declares `3.0.0-dev`, inspection emits `engine_version_conflict` and exits with code 2.

This is a preliminary declaration check. Workspace-inherited versions, optional/target-specific dependencies, features, Git/path identity, patches, and transitive constraints require the future resolver and driver build. Inherited package versions are reported as unresolved with a warning. A clean inspection does not mean those checks passed.

## Formal test entry point

The intended public command is already accepted:

```bash
cargo run --locked -- run \
  --engine 'registry:fountain_engine@2.1.0' \
  --operator 'path:../fountain-arena-operator#arena' \
  --test 'example-unregistered@0.1.0'
```

Today it performs inspection and exits with code **3** because no test/interface implementations are registered. Inspection errors take precedence and return **2**. It does not fetch sources, build a driver, time code, or create a run directory.

Once real tests and drivers are registered, the same three-input command will:

1. Resolve the component pair and support dependencies.
2. Select a verified compatible interface implementation **I** automatically.
3. Expand test version **T** into its fixed workload and execution recipe.
4. Observe the host and derive effective experiment configuration **E**.
5. Freeze inputs, build an isolated driver, and reject explicit conflicts.
6. Validate correctness, then perform the T-defined measurements.
7. Save the resolved `(C, I, T, E)`, raw evidence, and report under a unique run ID.

Users will not need to author target files, Cargo manifests, or experiment JSON. T owns schemes, K, symbol widths, seeds, repair schedules, cache modes, repetitions, and build requirements. Host facts are observed. Output/cache locations can become optional operational settings; arbitrary test-parameter overrides are not part of standard execution.

## Exit codes

| Code | Current meaning |
|---|---|
| 0 | Help/catalog succeeded, or inspection found no errors; no evaluation occurred |
| 1 | Output could not be written |
| 2 | CLI/reference error, local manifest error, or detected declaration conflict |
| 3 | Evaluation unavailable: no test/interface implementations are registered |

Commands emit JSON except help/version output. Inspection diagnostics are included in JSON; syntax errors go to stderr. `run` also explains its unavailable status on stderr.

## Reproducibility and comparison

| Dimension | Ownership |
|---|---|
| **C** | User-selected engine/operator pair; framework records dependency/source provenance |
| **I** | Automatically selected immutable native API adapter |
| **T** | User-selected immutable test content and workflow |
| **E** | Derived recipe plus actual build/runtime conditions |

Different I versions remain comparable when they implement equivalent semantics for the same T and E. If C and I both change, the direct conclusion covers their combined path. The same T on different machines does not guarantee the same E.

Crate versions, output formats, and run IDs are metadata, not additional tuple dimensions. No I or T release is published by this scaffold.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo doc --locked --no-deps
python3 scripts/check_repository.py
```

CI checks the scaffold on Linux and macOS. These are framework checks, not native Fountain correctness/performance results.

See [DESIGN.md](DESIGN.md) for architecture and version boundaries, [CONTRIBUTING.md](CONTRIBUTING.md) for development rules, and [specs/README.md](specs/README.md) for the draft protocol boundary.
