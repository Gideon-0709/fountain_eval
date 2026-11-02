# Formal-content construction validation

This record tracks the unreleased 0.1.0 implementation. It is a framework construction audit, not a component performance ranking or release approval. The earlier `VALIDATION*.md` records describe earlier smoke-content candidates.

## Scope

The three unused root directories have been removed. Their build-composition, input-generation and fixture guidance is consolidated in [modules](modules.md). One Cargo package embeds the four-category content, independent scalar reference, native/product bindings and diagnostic-only allocator. Runtime generated sources remain in the cache.

The catalog has 2,182 concrete cases. A complete applicable run requires correctness evidence, 15 ordinary samples per measured case, all fresh-process observations and three separate diagnostics per applicable chain/workflow. Reports retain fatal failures, boundary warnings, skips and unexecuted work separately.

## Local construction environment

The host is macOS ARM64 on Apple M4 Max (16 logical CPUs, 64 GiB RAM). Rust 1.89.0, release optimization and target-cpu=native are fixed for generated drivers. Local construction included concurrent compilation and evaluation processes; these observations validate execution/evidence mechanisms and must not be used to rank component throughput.

Public engine input is fountain_engine 2.1.0. Public operators use fixed Git commit `ce232222603876f3c033109c548cf1091e874c10`. The local Arena checkout is `d777cad6b512ee17663e129512bae10bca3397b1`; the authorized private monorepo is `2b603359f883448d257ec7c5401fc8a9b4054621`. No upstream source was changed or uploaded.

## Verified findings

- The public native Slab and Simd bindings compile, then the independent GF reference detects `mul_add(src, 0, dst)` clearing an existing nonzero destination. This is retained as a fatal arithmetic result. Later cases are not executed; these combinations are not positive whole-suite coverage.
- The development Slab source has the same branch and Simd delegates to it. Compilation of the source-engine 3 ordinary and diagnostic bindings succeeded during construction. Runtime findings are recorded separately from this source observation.
- An earlier formal Arena/RQ construction candidate completed 1,871 validations, 20,550 ordinary samples and 684 diagnostic observations. It reported 210 boundary warnings and no fatal failures. That candidate's T digest was `79db77b6107fe6c7625f77704db66b2a673fceec155dffa39ab69f8dc073a2c6`; subsequent work refined shared-write preparation and replay resource scope. This earlier pass is not represented as final-revision coverage.
- An earlier long Arena attempt was deliberately interrupted after a superseding implementation moved repeated untimed fixture generation out of the per-instance path. Its sample prefix and interrupted outcome were retained during the construction audit.

## Remaining acceptance work

The final matrix, reproducibility/fault ledger and final CI status will be added after the active acceptance run. Arena is private: CI must not fetch it without credentials. The initial CI candidate incorrectly assumed public access; the corrected harness requires an explicit authorized local Arena path. CI covers public-component rejection and framework checks; whole-suite positive native/product evidence is local. Cases blocked by upstream fatal checks, especially native-3 alias paths, remain coverage gaps until a compatible component passes them. A green framework CI rejection check does not turn those component failures into passes.

The candidate does not expose hidden product solver stages, dynamic-library internal allocations or native invalid-input contracts absent from the upstream API. These are labelled observability limits in the test guide and report, not invented zero-cost measurements.
