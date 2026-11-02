# Configuration and complete-catalog acceptance — 2026-11-02

This internal 0.1.0 candidate introduces one strict TOML configuration, explicit scheme selection/source binding, complete test-content selection and per-case applicability coverage. It creates no tag or release. Concrete workloads remain framework acceptance fixtures; formal test-content implementation is deferred. Encoding-plan generation is classified as chain testing, and plan reuse in a complete encoding workload as integrated testing.

## Scope

Local acceptance uses the public engine 2.1.0 and operators pinned at `ce232222603876f3c033109c548cf1091e874c10`, Arena `d777cad6b512ee17663e129512bae10bca3397b1`, and private monorepo `2b603359f883448d257ec7c5401fc8a9b4054621`. Private source checkouts are read-only. Every primary component combination is exercised with both RQ and R10.

Framework tests check strict input fields, config-relative paths, whole-version selection, explicit sources, complete catalog expansion, coverage states, forged skips, missing observations and common-scope comparison. Native tests exercise actual scheme binding, config/source freezing, cache identity, reports, warnings, failures and interruption. CI runs public native combinations on Linux ARM64 and macOS; Linux x86_64 verifies the known upstream compile rejection. Private combinations have local macOS coverage only.

## Local results

The final native acceptance completed all **80 checked commands**, including 16 expected nonzero exits. The primary matrix contains 16 successful runs on an Apple M4 Max, macOS ARM64, with Rust 1.89.0. Every run evaluates all 26 registered cases for applicability.

| Interface and operators | Runs (RQ and R10) | Fatal checks passed | Warning-level checks | Recorded samples | Validation / measurement skips per run |
| --- | ---: | ---: | ---: | ---: | --- |
| Public 2.1: Slab, Simd, Arena | 6 | 60 | 72 | 396 | 4 / 4 |
| Source 3: Slab, Simd | 4 | 40 | 48 | 264 | 4 / 4 |
| Product ABI: Slab, Simd, Arena | 6 | 24 | 72 | 72 | 10 / 22 |
| Total | 16 | 124 | 192 | 732 | 100 / 172 total |

Slab and Simd produced 144 reported boundary deviations across these runs; Arena produced none. All fatal checks passed. These warnings neither failed correctness nor blocked timing or comparison. The product rows retain callback-side boundary diagnostics while explicitly skipping the unavailable complete operator surface and pure-operator measurements. Cases for the unselected scheme are also explicitly skipped. The union of native RQ and R10 validation observations covers all 26 registered cases; the harness rejects an unreachable catalog entry.

The harness verified 15 accepted comparisons, including cross-interface common coverage and repeated prepared input. Different schemes and incomplete results were rejected. Every successful run was read back through `report`, checking that coverage and the original configuration were preserved.

Additional checks confirmed:

- An explicit registry scheme and the identical default scheme share a build key. Cargo metadata identifies the actual selected scheme package.
- Explicit local scheme input is frozen with the configuration. After preparation, replacing the original config with invalid text and adding a compile error to the source does not affect prepared execution. A fresh run detects the changed source and fails compilation.
- Workspace inheritance, external path dependencies and changed dependency contents are resolved and reflected in build identity. Prepared operator replay remains independent of subsequent workspace changes.
- Product external-scheme selection is rejected explicitly; the current source-engine-3/Arena dependency conflict is also rejected.
- Incorrect operator output, protocol errors, child exit, interruption, timeout, cache corruption, unknown interface/test/scheme and an invalid local reference preserve their actual failed or unsupported outcomes. Failures do not publish successful performance summaries.

All 37 framework tests passed, along with formatting, strict Clippy, rustdoc warnings-as-errors, the catalog identity lock and the single-package/English-documentation checks. One subprocess helper is marked ignored and is invoked by its parent test. An installed CLI also ran outside the eval checkout from `/tmp`, using a config-relative output/cache location and the published engine/operator with R10: 66 samples, 12 warnings and four explicit validation skips. Package contents include the config example, config/coverage modules and embedded drivers.

## Construction attempts and cleanup

An earlier construction attempt was intentionally interrupted after 24 checked commands (12 successful primary runs). The test identity calculation was refined during review to keep interface registration changes separate from test-content identity. That attempt is not counted as final acceptance. The final harness copies the evaluator binary before starting, ensuring every command uses the same candidate even if the checkout is rebuilt.

After the summary was recorded and no acceptance driver remained active, the task-owned directories `config-acceptance-1`, `config-acceptance-2` and `config-installed` under `.fountain-eval/` were removed, together with their temporary logs and package listing. Their combined allocated size before removal was 5,616,084 KiB. Normal user runs and the project's build directory were retained. Upstream monorepo and Arena checkouts remained clean at the revisions above; no upstream source or Git remote was written.

CI runs the public subset on each push. The GitHub Actions result for the committed revision is the CI evidence; local private coverage does not imply private CI coverage. No formal performance matrix, performance threshold, tag or release is introduced by this change.
