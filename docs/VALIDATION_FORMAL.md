# Formal-content construction validation

This record tracks the unreleased 0.1.0 implementation. It is a framework construction audit, not a component performance ranking or release approval. The earlier `VALIDATION*.md` records describe earlier smoke-content candidates.

## Scope

The three unused root directories have been removed. Their build-composition, input-generation and fixture guidance is consolidated in [modules](modules.md). One Cargo package embeds the four-category content, independent scalar reference, native/product bindings and diagnostic-only allocator. Runtime generated sources remain in the cache.

The catalog has 2,182 concrete cases. A complete applicable run requires correctness evidence, 15 ordinary samples per measured case, all fresh-process observations and three separate diagnostics per applicable chain/workflow. Reports retain fatal failures, boundary warnings, skips and unexecuted work separately.

## Local construction environment

The host is macOS ARM64 on Apple M4 Max (16 logical CPUs, 64 GiB RAM). Rust 1.89.0, release optimization and target-cpu=native are fixed for generated drivers. Local construction included concurrent compilation and evaluation processes; these observations validate execution/evidence mechanisms and must not be used to rank component throughput.

Public engine input is fountain_engine 2.1.0. Public operators use fixed Git commit `ce232222603876f3c033109c548cf1091e874c10`. The local Arena checkout is `d777cad6b512ee17663e129512bae10bca3397b1`; the authorized private monorepo is `2b603359f883448d257ec7c5401fc8a9b4054621`. No upstream source was changed or uploaded.

The final harness uses a frozen evaluator binary from implementation revision `e36ed08c573a6662f6b82cad6bd82d5b341eaa2b`. Documentation-only changes do not alter its I/T identities:

| Identity | SHA-256 content digest |
| --- | --- |
| T `0.1.0` | `ac85b5a057ae1708ef7d59b8b688f2ca709e52e317750552d22b47e885e8078f` |
| I `native-2@0.1.0` | `c6b908b5f8a3d14de614d4fc28241fc6456e28db69f625914c003458f6992044` |
| I `native-3@0.1.0` | `5f72feae01f69a9887cc14d3e3eef70886e4595c993b352fd6f66d9a3c724e92` |
| I `product-abi@0.1.0` | `b39944ef87db2453228e9944496d8c005a19036ac550c1bfc2efab5ee64e3ec8` |

The frozen evaluator binary SHA-256 is `0f1ce1375e4e5a19b2ef957284e4ff393a03c60b6590de98fa1bf40c821acebe`. All generated component drivers use the release profile independently of the evaluator's own build profile.

## Primary native matrix

All 16 primary runs completed with their actual outcomes. Eight succeeded over their full applicable plan; eight stopped at an independently detected component fatal error. Counts below are **per run**, not totals across grouped operators.

| Integration | Operator / scheme | Runs | Outcome | Validation observations | Ordinary samples | Diagnostics | Warnings |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: |
| Public engine 2.1 | Arena / RQ | 1 | Succeeded with warnings and skips | 1,871 | 20,550 | 684 | 210 |
| Public engine 2.1 | Arena / R10 | 1 | Succeeded with warnings and skips | 1,869 | 20,550 | 684 | 210 |
| Public engine 2.1 | Slab, Simd / RQ, R10 | 4 | Fatal GF result; no measurement | 31, including the failure | 0 | 0 | 0 |
| Source engine 3 | Slab, Simd / RQ, R10 | 4 | Fatal GF result; no measurement | 31, including the failure | 0 | 0 | 0 |
| Product ABI | Slab, Simd, Arena / RQ | 3 | Succeeded with capability skips | 261 | 3,240 | 630 | 0 |
| Product ABI | Slab, Simd, Arena / R10 | 3 | Succeeded with capability skips | 259 | 3,240 | 630 | 0 |

The eight successful primary runs produced 60,540 ordinary samples and 5,148 diagnostic observations. Reports were read back through the CLI and checked against the stored coverage. Product lifecycle cases recorded actual callback reads, writes, arithmetic and operator drops, checked byte-for-byte recovery, and exercised safe-facade length rejection and callback-failure propagation/release. The actual dynamic-library path and digest and the callback/library dependency graphs are retained by the result schema.

Eight same-scheme comparisons passed. The two Arena self-comparisons contain 1,370 measured rows each; six native/product comparisons contain the 216 genuinely common codec rows and identify the combined component/interface change. RQ-versus-R10 comparison was rejected. Source engine 3 with the current Arena was rejected before measurement for its exact engine 2.1 dependency requirement.

## Framework and packaging checks

The local framework checks passed: 41 tests, formatting of the package and all embedded driver templates, strict Clippy, rustdoc with warnings denied, the catalog identity seal, and the single-package/English-documentation/link checks. The ignored subprocess entry point is invoked by its parent test; it is not omitted interruption coverage.

Package listing confirmed the embedded content, reference, telemetry and binding assets without caches or results. A freshly installed CLI, invoked from `/tmp`, built both the ordinary and diagnostic source-engine-3 drivers. The installed CLI read a complete product result, rendered HTML/Markdown and produced a successful self-comparison without relying on the eval checkout.

Static HTML/CSV checks verified balanced tables, the case-filter control/script, 216 product measurement rows, 3,240 unique raw sample rows with positive durations, and no external assets. Browser policy rejected local `file://` preview, so interactive and visual browser inspection is not claimed.

## Public CI coverage

[Actions run 41445345199](https://github.com/Gideon-0709/fountain_eval/actions/runs/41445345199), at `e36ed08`, passed on Linux x86_64, Linux ARM64 and macOS. All three run the framework quality checks. ARM64/macOS additionally verify the public Slab/Simd fatal arithmetic rejection and fault lifecycle. Linux x86_64 verifies the pinned upstream unresolved `portable` compile rejection. Neither rejection is positive whole-suite component coverage.

Arena is private and the monorepo requires authorized local access. The initial CI candidate attempted an unauthenticated Arena fetch and failed; the corrected harness requires an explicit local `--arena` input and CI does not fetch private sources. Private positive coverage is macOS ARM64 only.

## Verified findings

- The public native Slab and Simd bindings compile, then the independent GF reference detects `mul_add(src, 0, dst)` clearing an existing nonzero destination. This is retained as a fatal arithmetic result. Later cases are not executed; these combinations are not positive whole-suite coverage.
- The development bindings have the same defect. Their RQ and R10 runs compiled both driver variants and failed the independent runtime check before measurement. The operator contract for distinct IDs is `dst ^= scalar * src`; zero must preserve `dst`. This finding is separate from warning-level self-XOR and alias semantics.
- An earlier formal Arena/RQ construction candidate completed 1,871 validations, 20,550 ordinary samples and 684 diagnostic observations. It reported 210 boundary warnings and no fatal failures. That candidate's T digest was `79db77b6107fe6c7625f77704db66b2a673fceec155dffa39ab69f8dc073a2c6`; subsequent work refined shared-write preparation and replay resource scope. This earlier pass is not represented as final-revision coverage.
- An earlier long Arena attempt was deliberately interrupted after a superseding implementation moved repeated untimed fixture generation out of the per-instance path. Its sample prefix and interrupted outcome were retained during the construction audit.

- The first Arena/R10 formal attempt stopped at K=1 because the catalog initially applied the 4..8192 domain only to the product API. The registered native R10 scheme uses the same supported domain; its source test enumerates 4..8192. The corrected applicability rule reports both K=1 success rows as outside-domain skips. The original failed attempt remains part of this construction audit.

## Construction attempt ledger

These are separate candidates and attempts, not retries merged into a passing result:

| Attempt | Outcome and correction |
| --- | --- |
| Early development Arena run | Deliberately interrupted with 1,871 validation observations and 944 raw samples when fixture preparation was refined. |
| `formal-acceptance-1` | Public Slab/Simd correctly failed GF arithmetic. Arena was deliberately interrupted after 20,496 samples while a replacement candidate was prepared. |
| `formal-acceptance-2` | Framework template compilation failed because `EnsureZeroRange` was emitted with a `range` field. The binding was corrected to use the actual `start`/`end` fields. |
| `formal-acceptance-3` | Arena/RQ passed. Arena/R10 failed at the erroneous K=1 success case; the registered R10 applicability domain was corrected without changing upstream code. |
| Initial product preparation | A construction command used the wrong monorepo subdirectory. The corrected reference selects the actual root `fountain_bin_ffi` package. |
| Initial public CI | Private Arena fetch failed without credentials. The harness now requires an explicitly supplied authorized checkout for that input. |
| Protocol fault fixture | The fixture was refined to emit malformed stdout and exit normally, isolating protocol rejection from the separate wrong-output fatal path. The final harness checks the actual protocol error. |

Failure and interruption records were preserved while candidates were diagnosed. The final acceptance and temporary-artifact disposition are recorded separately below.

## Catalog coverage boundary

The union of actual correctness observations reaches 2,148 cases with passed or warning-level evidence. Thirty-two applicable cases remain unexecuted: 16 `operation/alias` and 16 `operation/shared-first-write` cases, covering eight widths and two working sets through the native-3 direct interface. The source-engine-3 combinations stop at the earlier fatal arithmetic check. These are an explicit positive-coverage gap, not capability skips or passes; rerunning a corrected compatible component is necessary to close it.

The remaining two registered rows are R10 K=1 success cases at widths 31 and 128. All selected bindings declare them outside the supported 4..8192 input domain. Their applicability exclusions remain visible in the catalog. Product invalid-input rejection is exercised separately through its error contract.

Successful product coverage does not certify the entire callback operator: the ABI exposes codec workflows and hides the full primitive interface. Its reports therefore retain those capability skips even when codec results pass. Public Arena's 210 warnings per scheme concern overlapping copy/move and target-in-sources behavior; they remain non-blocking and their operation timings are retained.

## Reproducibility and failure handling

A prepared Arena/RQ input completed a second whole run after a compile error was inserted into its original temporary source copy. The frozen run produced 1,871 validation observations, 20,550 samples and 684 diagnostics and compared successfully with the primary run under the same T/E conditions. A fresh run against the changed source failed compilation, as required. Corrupting the prepared driver then produced an artifact-checksum rejection; the evaluator did not silently rebuild the damaged cache.

The final harness completed **47 checked CLI commands**, including 20 expected nonzero exits. The primary component failures are the eight fatal results described above. Separate negative checks verified wrong operator output, driver exit 23, malformed stdout protocol, unknown interface/test/scheme, a missing local reference, SIGINT, dependency conflict, changed-source compilation, damaged-cache rejection and cross-scheme comparison rejection. Each retained its actual failure, unsupported or interrupted status without a successful performance summary.

Short real-process tests separately verified timeout, crash and SIGINT cleanup of descendant process groups, with stdout/stderr preserved. Evidence tests covered altered E/T, identity collisions, modified seals, missing/duplicate samples, incorrect units/work counts, incomplete recovery and invalid diagnostic accounting. A green framework CI rejection check does not turn component failures or unexecuted cases into passes.

The candidate does not expose hidden product solver stages, dynamic-library internal allocations or native invalid-input contracts absent from the upstream API. These are labelled observability limits in the test guide and report, not invented zero-cost measurements.

## Temporary artifact cleanup

After recording the outcomes and verifying that no acceptance or generated-driver process remained active, the task-owned directories `.fountain-eval/formal-dev` and `.fountain-eval/formal-acceptance-1` through `formal-acceptance-4` were removed. Their combined allocated size immediately before removal was 8,935,952 KiB. This removed the temporary snapshots, drivers, installed CLI, raw attempts and rendered verification reports; their failures, corrections, coverage and final counts are summarized above.

Normal user results, other caches and the repository's Cargo build directory were retained. The two upstream checkouts remained clean at the recorded revisions. The existing local `rust-toolchain.toml` editor-component change was preserved without including it in these commits. No tag, release, upstream modification or private-source upload was made.
