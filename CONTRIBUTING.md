# Contributing

Develop and publish one `fountain-eval` package, providing the `feval` CLI and the `fountain_eval` library. Native Fountain dependencies belong to generated independent drivers, not the root package. Standard users provide one config with indexed engine/operator selections, scheme and a complete test content version. Individual test selection and silent skips are prohibited.

## Quality checks

```bash
cargo fmt --all -- --check
rustfmt --edition 2024 --config skip_children=true --check drivers/common.rs drivers/contracts.rs drivers/operator.rs drivers/native.rs drivers/product.rs drivers/reference.rs drivers/workloads.rs drivers/telemetry.rs drivers/scalar_operator.rs
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
python3 scripts/check_repository.py
cargo build --locked
python3 scripts/native_acceptance.py --work .fountain-eval/acceptance-new --faults
```

The native framework smoke uses the config emitted by `feval init` to exercise execution, report publication, cleanup and comparison. It requires Python 3.11 or later. The `--faults` option uses generated fixtures to check configuration, build, validation and process failures. Add `--reproduction` to exercise frozen execution with an edited config and reject a corrupted prepared binary. CI runs the same smoke and fault checks on each platform.

Use a fresh acceptance work directory. The acceptance script activates its own workspace and generates each report explicitly from a sealed run. Keep first failures and interruptions while debugging; a corrected candidate is a new attempt, not an automatic retry of the old result. The process interruption helper is an ignored test entry point invoked by its parent test in a separate process, so SIGINT cannot contaminate the normal test harness.

Evaluate real component combinations with `feval run --config CONFIG.toml` and publish the result with `feval report RESULT_ID`. The framework resolves and builds the configured selection, checks its behavior and records compatibility failures in the usual result. Release coverage comes from these runs and is specific to the selections, schemes and platforms exercised.

## Change ownership

- C owns component implementations, requested Cargo features and resolved dependencies.
- I owns native calls, conversions, dependency binding, diagnostic build requirements and capabilities.
- T owns workloads, timing, correctness severity, reference expectations and metric semantics.
- E is derived from the recipe and actual common conditions.

Release tags freeze the I/T identities in `specs/catalog-lock.json`. Changes to a published interface or test content require a new version of that identity. Reporting changes that preserve measurement semantics retain the same methodology. Keep API changes separate from test-content changes when their semantics permit it.

## Documentation and retention

Write docs and diagnostics in English. README examples must execute the implemented workflow; [DESIGN](docs/DESIGN.md) explains the version boundaries and comparability model. [TEST](docs/TEST.md) is the current test-content specification. Every change to test coverage, parameters, terminology, severity or timing must update this document in the same change as the implementation and regression checks. Keep the current filename; the content version belongs inside the document and catalog. Use **Validation tests (V)**, **Operation tests (O)**, **Link tests (L)** and **Workflow tests (W)** as the category names in documentation, terminal output and reports. Do not describe implementation acceptance as a component performance conclusion.

Do not commit caches, component checkouts, credentials, personal bindings or raw acceptance outputs. After writing an implementation acceptance summary, remove only the explicitly identified temporary validation directories. The report command keeps the report directory and, by default, removes that run's temporary evidence and build cache after verified publication.

Repository work targets main as directed by the project owner. Upstream access grants no write permission. Keep `publish = false`; crates.io publication and licensing policy are separate decisions.

Fatal failures must remain blocking. Boundary warnings must be shown with expected/actual observations and timing rows, and must not block comparison solely because outputs differ. Severity and coverage are T-owned content.

Use standard run reports to account for every registered case in release coverage. Register applicability with a reason, keep failed/not-executed cases distinct, and test that missing observations cannot be hidden by changing coverage. Config parsing belongs to the framework; workload/severity/measurement policy belongs to T.
