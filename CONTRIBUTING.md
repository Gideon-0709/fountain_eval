# Contributing

Develop and publish one `fountain-eval` package. Native Fountain dependencies belong to generated independent drivers, not the root package. Standard users provide only engine, operator and test references.

## Quality checks

```bash
cargo fmt --all -- --check
rustfmt --edition 2024 --config skip_children=true --check drivers/common.rs drivers/operator.rs drivers/native.rs drivers/product.rs
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
python3 scripts/check_repository.py
cargo build --locked
python3 scripts/native_acceptance.py --work .fountain-eval/acceptance-new --faults
```

The normal public native suite currently requires ARM64 because the pinned upstream AVX2 source has unresolved `portable` references on x86_64. Linux x86_64 CI uses `--expect-upstream-x86-failure` to verify that rejection; it does not claim positive native coverage.

Use a fresh acceptance work directory. Keep first failures and interruptions while debugging; a corrected candidate is a new attempt, not an automatic retry of the old result. The process interruption helper is an ignored test entry point invoked by its parent test in a separate process, so SIGINT cannot contaminate the normal test harness.

For full authorized local coverage, add `--monorepo /path/to/erasure_coding_lib --arena /path/to/fountain-arena-operator`. These inputs are read-only. The acceptance script creates its own negative fixtures; never change the tested upstream components to obtain a pass. Private coverage on one platform does not establish coverage on another.

## Change ownership

- C owns component implementations and their resolved dependencies.
- I owns native calls, conversions, feature/source binding and capabilities.
- T owns workloads, timing, correctness and metric semantics.
- E is derived from the recipe and actual common conditions.

Registered I/T content is frozen by `specs/catalog-lock.json`. Do not overwrite an existing released identity after changing its content; add a new version and migrate registrations deliberately. A reporting-only fix does not require changing the methodology. Keep API changes separate from test-content changes when their semantics permit it.

## Documentation and retention

Write docs and diagnostics in English. README examples must execute the implemented workflow; DESIGN explains the version boundaries and comparability model. Do not describe smoke measurements as formal performance conclusions.

Do not commit caches, component checkouts, credentials, personal bindings or raw acceptance outputs. After writing an implementation acceptance summary, remove only the explicitly identified temporary validation directories. The CLI never cleans normal user results automatically.

Repository work targets main as directed by the project owner. Upstream access grants no write permission. Keep `publish = false`; crates.io publication and licensing policy are separate decisions.
