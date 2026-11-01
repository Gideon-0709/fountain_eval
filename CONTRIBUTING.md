# Contributing

## Package boundary

Develop one `fountain-eval` package with library and CLI targets. Use internal modules. Native Fountain dependencies stay outside the framework; future native drivers use isolated build roots where needed.

The standard request contains engine, operator and test references. Interface selection and configuration derivation belong to the framework. Schemes and utility libraries are dependencies, not additional mandatory component inputs.

## Checks

Use the pinned Rust toolchain and commit `Cargo.lock` updates:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo doc --locked --no-deps
python3 scripts/check_repository.py
```

Cover meaningful input, error and lifecycle boundaries. Framework checks are not native codec or performance evidence. No I/T versions are registered yet.

## Change ownership

- Component implementation belongs to C and component repositories.
- Native API conversions and capabilities belong to I.
- Workload, timing, validation and statistical semantics belong to T.
- Actual recipe/environment conditions form E automatically.
- Crate, serialization and run identifiers do not add tuple dimensions.

Published I/T identities are immutable. Preserve failed evidence when fixing an affected implementation. Never hide dependency conflicts through silent component edits or move work outside measurement without changing its semantics.

## Documentation and hygiene

Write repository documentation and user-facing diagnostics in English. Keep README examples and DESIGN aligned with implemented behavior, the two-component model and automatic configuration. Mark future capabilities explicitly.

Do not commit component checkouts, caches, credentials, personal path bindings or temporary results. Upstream access does not authorize writes. Registry publication and repository licensing remain separate decisions; keep `publish = false` until publication is authorized.
