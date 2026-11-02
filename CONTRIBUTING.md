# Contributing

Develop and publish one `fountain-eval` package. Native Fountain dependencies belong to generated independent drivers, not the root package. Standard users provide one config with engine, operator, scheme and a complete test content version. Individual test selection and silent skips are prohibited.

## Quality checks

```bash
cargo fmt --all -- --check
rustfmt --edition 2024 --config skip_children=true --check drivers/common.rs drivers/operator.rs drivers/native.rs drivers/product.rs drivers/reference.rs drivers/workloads.rs drivers/telemetry.rs
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
python3 scripts/check_repository.py
cargo build --locked
python3 scripts/native_acceptance.py --work .fountain-eval/acceptance-new --faults
```

The public native acceptance checks Arena as a positive combination and verifies the pinned Slab/Simd fatal `mul_add(..., 0, ...)` result without treating its unexecuted cases as passes. It writes an explicit reachability gap report. The normal public native suite currently requires ARM64 because the pinned upstream AVX2 source has unresolved `portable` references on x86_64. Linux x86_64 CI uses `--expect-upstream-x86-failure` to verify that rejection; it does not claim positive native coverage.

Use a fresh acceptance work directory. Keep first failures and interruptions while debugging; a corrected candidate is a new attempt, not an automatic retry of the old result. The process interruption helper is an ignored test entry point invoked by its parent test in a separate process, so SIGINT cannot contaminate the normal test harness.

For full authorized local coverage, add `--reproduction --monorepo /path/to/erasure_coding_lib --arena /path/to/fountain-arena-operator`. These inputs are read-only. The acceptance script creates its own negative fixtures; never change the tested upstream components to obtain a pass. Private coverage on one platform does not establish coverage on another.

## Change ownership

- C owns component implementations and their resolved dependencies.
- I owns native calls, conversions, feature/source binding and capabilities.
- T owns workloads, timing, correctness severity, reference expectations and metric semantics.
- E is derived from the recipe and actual common conditions.

The repository is an unreleased internal 0.1.0 candidate. Update this baseline and its seal deliberately (`python3 scripts/update_catalog_lock.py` refreshes `specs/catalog-lock.json`) during the current round; do not invent additional versions or create tags/releases. The project owner will release 0.1.0 after acceptance. Once released, I/T content identities are immutable. A reporting-only fix does not require changing the methodology. Keep API changes separate from test-content changes when their semantics permit it.

## Documentation and retention

Write docs and diagnostics in English. README examples must execute the implemented workflow; [DESIGN](docs/DESIGN.md) explains the version boundaries and comparability model. Do not describe smoke measurements as formal performance conclusions.

Do not commit caches, component checkouts, credentials, personal bindings or raw acceptance outputs. After writing an implementation acceptance summary, remove only the explicitly identified temporary validation directories. The CLI never cleans normal user results automatically.

Repository work targets main as directed by the project owner. Upstream access grants no write permission. Keep `publish = false`; crates.io publication and licensing policy are separate decisions.

Fatal failures must remain blocking. Boundary warnings must be shown with expected/actual observations and timing rows, and must not block comparison solely because outputs differ. Severity and coverage are T-owned content. During the current unreleased round they remain part of the same 0.1.0 candidate; content digests identify each revision.

Every registered case must be reached in native acceptance or explicitly accounted for as inapplicable. Register applicability with a reason, keep failed/not-executed cases distinct, and test that missing observations cannot be hidden by changing coverage. Config parsing belongs to the framework; workload/severity/measurement policy belongs to T.
