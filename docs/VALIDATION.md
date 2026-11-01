# Implementation acceptance — 2026-11-01

This summary covers framework v0.2.0 implementation acceptance. It is not a formal performance report, and none of the short timings establish an optimization advantage. Registered I/T digests are frozen in [the catalog lock](../specs/catalog-lock.json).

## Local platform and component inputs

The complete private/public integration matrix ran on macOS, aarch64, Apple M4 Max (16 logical CPUs, 64 GiB), Darwin 25.5.0, Rust 1.89.0, Apple clang 21 and SDK 26.5. Native drivers used release, opt-level 3, one codegen unit, no LTO and `-C target-cpu=native`. Power source was AC. Test settings came entirely from the selected acceptance recipe.

| Input | Fixed identity |
|---|---|
| Public engine | crates.io `fountain_engine` 2.1.0; archive SHA-256 `3b21ca28960a99de22b41497324d140c7b3163065a73385ee5de776725dc5b90` |
| Public Slab/Simd | `shhyang/fountain_operators` commit `ce232222603876f3c033109c548cf1091e874c10`, package 2.0.0 |
| Registry-reference check | crates.io `fountain_operators` 2.0.0 |
| Arena | `Gideon-0709/fountain-arena-operator` commit `d777cad6b512ee17663e129512bae10bca3397b1`, package 0.2.0 |
| Development/product monorepo | `shhyang/erasure_coding_lib` commit `2b603359f883448d257ec7c5401fc8a9b4054621` |

The private checkout and Arena checkout were clean inputs and remained unmodified. Both path and pinned Git Arena references were exercised. The development engine was 3.0.0-dev; its operator package was 1.0.0. Product selection used `fountain_bin_ffi` 0.1.0 from the same monorepo revision.

## Native positive coverage

| Interface and components | Operator acceptance | Codec acceptance |
|---|---|---|
| Public engine 2.1 + Arena | Passed | Passed |
| Public engine 2.1 + Slab | Passed | Passed |
| Public engine 2.1 + Simd | Passed | Passed |
| Development engine 3 + Slab | Passed | Passed |
| Development engine 3 + Simd | Passed | Passed |
| Product API + Arena callbacks | Explicitly unsupported | Passed |
| Product API + Slab callbacks | Explicitly unsupported | Passed |
| Product API + Simd callbacks | Explicitly unsupported | Passed |

These are 13 positive component/test runs: five source-engine combinations × two tests, plus three product codec runs. They produced 94 correctness case records and 282 measured samples. Each case had one warmup and three recorded samples. Correctness included complete and missing-source RQ/R10 recovery and every operator trace prefix.

Product runs verified the actual shared-library loader path, binary hash, callback reads/writes and normal encoder/decoder operator destruction. Arena product callbacks resolved engine 2.1.0 while the product library resolved engine 3.0.0-dev. Slab/Simd product callbacks and the library each resolved engine 3.0.0-dev in their separate graphs.

The complete local matrix plus reports, comparisons, fixed references, failure and replay checks completed **58 checked CLI commands**. An installed CLI, invoked from outside the eval checkout, completed another **33 checked commands** against public sources, including workspace inheritance, external path dependencies and protocol failure. Expected nonzero exits are asserted negative checks, not successful component evaluations.

## Failure and reproduction boundaries

Verified boundaries include:

- invalid local references, malformed/mutable reference grammar, unknown I/T selection and ambiguous operator selection;
- source engine 3 + current Arena rejected for its exact engine 2.1 dependency constraint;
- product operator-only tests rejected as unsupported;
- actual native compile failure and wrong operator results rejected before measurement;
- actual driver nonzero exit, timeout, SIGINT and malformed stdout protocol captured as failure/interruption;
- process-group cleanup on timeout, crash and interruption, with stdout/stderr preserved;
- immutable dirty/untracked source snapshots, inherited workspace version/dependencies/lints, enabled native features, and external path dependencies;
- modifying an external dependency produced a new build key while the selected operator's own source digest remained unchanged;
- modifying the original package after prepare did not affect prepared replay; a fresh request saw the new source and failed compilation as intended;
- two replays of fixed input compared successfully; a corrupted binary was rejected without rebuilding it into a pass;
- same-condition and registered cross-I comparisons succeeded; changed T, changed E, identity collisions, invalid work counts, incomplete samples and failed results were rejected;
- sealed evidence could be read independently of its build cache; modified evidence was rejected;
- package contents included embedded driver templates and the catalog lock, and installation required no native eval checkout at runtime.

Framework tests cover metrics in decimal Gbit/s and operations/s, stage/not-executed states, partial evidence validity, source/archive boundaries and protocol identity. The normally ignored interruption helper is explicitly invoked by its parent test in a separate process; it is not missing coverage.

## Deferred same-ID semantics

An initial strict self-XOR diagnostic found different behavior: Arena returned zero, while public and development Slab/Simd treated the same-ID operation as a no-op. The public trait does not settle that contract. The project owner explicitly deferred this question for discussion with the component developers.

Consequently, the released acceptance trace uses distinct IDs, including distinct-ID cancellation and copy/write isolation. Same-ID arithmetic is not certified, scored as a component failure, or included in I equivalence claims. A future agreed contract requires a new T identity; this release must not be silently repointed.

## Development failure ledger

Early candidates exposed and corrected framework issues in TOML table insertion, registry archive handling, nested Cargo workspace composition and cache-internal source filtering. A source regression test now covers downloaded input inside the cache. Initial self-XOR failures were preserved as diagnostic findings until the scope decision above. A candidate spanning a test-content change was rejected by comparison for inconsistent T identity; it was not merged into the successful matrix.

Normal formatting/lint corrections and a host-filtered dependency check were completed separately. The latter avoids downloading unrelated foreign-platform framework dependencies during repository inspection. Unexpected native failures stopped their candidate immediately. Subsequent corrected candidates used separate work directories; failed attempts were not retried or relabeled as successful measurements.

## CI and evidence retention

The [CI workflow](https://github.com/Gideon-0709/fountain_eval/actions/workflows/ci.yml) runs formatting, strict Clippy, framework tests, documentation, package-boundary checks, and public native/failure/reproducibility acceptance on Linux and macOS. Consult the workflow for the exact commit's remote status. This local summary does not substitute for CI evidence.

Private Arena, development engine 3 and product API coverage in this delivery is **local macOS coverage**. Private combinations are not claimed as Linux or credential-free CI coverage. No upstream repository was modified or uploaded to.

Implementation work directories and first-failure logs were kept through analysis and summarized here. They are temporary construction artifacts, not a benchmark archive, and are cleaned after acceptance. Normal user `runs/` data is managed only by the user; the CLI never performs this cleanup automatically.
