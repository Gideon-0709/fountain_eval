# Fatal and warning acceptance — 2026-11-02

The concrete checks in this summary are framework acceptance fixtures, not the team's formal test content. The fixtures exercise the general severity/reporting mechanism and remain available for development regression. Formal test design is a later task.

This update implements the project owner's decision to classify ambiguous operator boundary deviations as warnings. Fatal checks still reject incorrect required behavior. Warning outcomes and timings are reported together, including comparisons between implementations with different warning outcomes.

Package, I and T use the planned internal `0.1.0` baseline. This development round is not a release; the owner will publish the first team release after acceptance. No extra version series, historical compatibility layer, tag or GitHub Release is created.

## Acceptance scope

The public/private component pins and local macOS host are the same as the [initial implementation acceptance](VALIDATION.md). Public x86 AVX2 compilation remains an upstream limitation; Linux ARM64 and macOS provide native positive CI coverage. Private combinations have local macOS coverage only.

The native harness asserts fatal-check counts, boundary-check counts, warning summaries, retained samples, `correctness.json`, report output and warning-tolerant comparisons. It confirms that wrong ordinary operator behavior still fails before measurement. Framework tests cover measured boundary variation, forbidden severity downgrades, missing warning coverage, evidence corruption and rejection of different draft content digests.

## Completed local acceptance

- Framework checks: 32 tests passed; one ignored subprocess helper is invoked explicitly by its parent test. Formatting, strict Clippy, rustdoc and repository/documentation checks passed.
- Native acceptance: 64 checked CLI commands, including 13 primary component/test combinations, their reports and comparisons, preparation/replay and failure scenarios. Every command returned its expected status.
- The primary runs completed 94 fatal checks and 156 warning-level checks, and retained 462 measurement samples. All fatal checks passed. Ten Slab/Simd runs each reported 12 boundary deviations (120 warnings in total); the three Arena runs reported no deviations in these fixtures.
- Warnings remained visible in reports and comparisons. Wrong ordinary operator results still failed validation before measurement. Crash, timeout, interruption, invalid references, unknown I/T, incompatible dependencies, invalid interfaces, damaged caches and non-comparable results exercised their failure paths.
- A freshly installed CLI ran outside the checkout, from `/tmp`, using a fresh cache with registry engine 2.1.0 and Slab 2.0.0. Its operator acceptance succeeded with 12 warnings and 54 samples; the installed `report` command reproduced the correctness report.

| Interface family | Operator | Primary acceptance runs | Fatal checks | Warning-level checks | Deviations | Samples |
|---|---|---|---:|---:|---:|---:|
| Public 2.1 | Arena | operator, codec | 14 | 24 | 0 | 78 |
| Public 2.1 | Slab | operator, codec | 14 | 24 | 24 | 78 |
| Public 2.1 | Simd | operator, codec | 14 | 24 | 24 | 78 |
| Development 3 | Slab | operator, codec | 14 | 24 | 24 | 78 |
| Development 3 | Simd | operator, codec | 14 | 24 | 24 | 78 |
| Product ABI | Arena | codec | 8 | 12 | 0 | 24 |
| Product ABI | Slab | codec | 8 | 12 | 12 | 24 |
| Product ABI | Simd | codec | 8 | 12 | 12 | 24 |

These counts describe framework acceptance coverage. They do not establish a complete component correctness contract or performance ranking. CI executes the public coverage declared in `.github/workflows/ci.yml`; private monorepo combinations are covered only by the local macOS run.

## Construction observations

The first warning candidate exposed zero-duration readings for single no-op calls on the local clock. The next candidate used a fixed batch of 256 independently initialized operators per timing sample and retained grouped outcomes for every call. Public Slab/Simd/Arena combinations passed that candidate before it was intentionally interrupted to apply the owner's internal-version policy. Those partial attempts are not counted as final acceptance. The final candidate incorporates the same batch methodology into the single 0.1.0 baseline.

## Temporary artifact cleanup

After recording this summary, the three task-owned `warnings-acceptance-*` directories and `warnings-installed` directory under `.fountain-eval/`, together with their temporary logs and package-list output, were removed. Their combined allocated directory size before cleanup was 3,683,524 KiB (approximately 3.51 GiB). The failed first attempt and interrupted second attempt remain documented above; neither is relabeled as a passing run. No normal user results or upstream source checkouts were removed or modified.
