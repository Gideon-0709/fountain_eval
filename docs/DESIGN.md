# Fountain evaluation architecture

## From a component change to a comparable result

A developer describes an engine/operator/scheme combination and a complete test content version in one TOML file. Arena is an operator implementation. Scheme selection is explicit; its implementation source defaults to a registered dependency or the selected monorepo and can be overridden for native APIs. Utility and product-facade dependencies remain supporting provenance.

The execution sequence is:

1. Parse the config, resolve its relative paths and load the complete registered test content.
2. Resolve real packages and freeze their source inputs, including dirty local bytes and path dependencies.
3. Select an interface integration, expand every case with explicit applicability reasons, derive common build settings and observe the host environment.
4. Generate an isolated Cargo build view, check version constraints, build, and probe the actual process/library.
5. Validate fixed test inputs independently of the tested operator's arithmetic, then collect the prescribed samples.
6. Seal a unique local result, preserving any failure stage and partial evidence.
7. Compare only complete results with compatible identities and conditions.

`prepare` stops after step 4. `run --prepared` repeats steps 5–6 against that fixed input, observing current runtime conditions. Offline `inspect` only reads declarations; it is not a shortened correctness gate.

## The four-tuple

A completed input identity is `(C,I,T,E)`:

| Dimension | Owns | May change independently |
|---|---|---|
| C: selected component combination | Engine/operator/scheme identities, implementation choice, source digests, resolved dependency provenance | Algorithms, layouts, kernels, allocation and internal implementation |
| I: interface integration | Native API conversions, feature/source binding, supported capabilities, driver family | Package/API/trait/product ABI integration |
| T: test content | Input generation, operations, correctness oracle and severity, timing boundary, sampling and metric semantics | Test scenarios, coverage, workload and methodology |
| E: effective experiment configuration | Selected scheme algorithm/mode, T-derived parameters, common compiler/build settings and observed runtime conditions | Host, OS, toolchain, CPU/power/affinity and common build environment |

I and T have independent version-plus-content identities. A published version must not be repointed to changed content. The catalog lock and repository checks detect content that differs from the explicitly recorded candidate. During the current unreleased round the owner has fixed the planned baseline at 0.1.0; published-version rules apply after that first release. Components' declared package versions alone are insufficient: source bytes and dependency locks distinguish two different checkouts carrying the same version string.

Framework package version, serialization schema, run ID, wall-clock time and report renderer are provenance, not additional tuple axes. A semantic change to interface calls or measurement still belongs to I or T even if delivered in a small framework patch.

A failed early stage has no complete tuple. After a successful build/probe, the fixed input tuple can exist even if subsequent correctness fails; that record remains failed and is not eligible for comparison. Raw sample prefixes never imply completion.

## Stable responsibilities and dependencies

The framework is one Cargo package. `src/main.rs` is the CLI; library modules own requests, resolution, build views, execution, metrics and evidence. Native Fountain crates are not linked into the framework package. Embedded source templates produce an independent generated driver executable with its own lockfile and release build. The product library has a separate native build and lockfile as well.

```mermaid
flowchart TD
  User[One config: engine + operator + scheme + test version] --> Eval[fountain-eval package]
  Eval --> Frozen[Immutable source snapshots]
  Eval --> Registry[I and T catalog]
  Frozen --> View[Generated build view]
  Registry --> View
  View --> Driver[Native driver process]
  View --> Product[Product shared library when selected]
  Product --> Driver
  Driver --> Evidence[Validation, raw samples and sealed results]
  Evidence --> Compare[T/E and I-equivalence checks]
```

Component authors keep their normal repositories and Cargo manifests. They do not copy code into eval, write target manifests, select I, or manually reproduce K/width/repair settings. Changes to registered package identity, workspace layout, features, traits, constructors, operation semantics or product ABI must be communicated to the eval maintainer when the existing mapping cannot compile or preserve the contract.

A stable trait is necessary but not sufficient: its semantics, lifecycle, required support dependencies and available capabilities also matter. The correctness stage detects behavioral conflicts that Cargo cannot see. Version constraints and duplicated trait identities are checked before measurement.

## Configuration and complete test execution

`src/config.rs` is the sole user configuration boundary. The config contains engine/operator references, a scheme name or detailed source selection, the complete content version, and optional storage paths. Relative paths use the config directory. Unknown fields and user case filters are rejected. No includes, environment substitution or global/CLI component overrides exist. All settings are normalized once and stored before execution; replay uses that frozen configuration.

Config is an input to `(C,I,T,E)`, not an additional identity axis. Raw text, comments and filesystem location do not participate in experiment equality. Scheme implementation changes belong to C; the requested scheme algorithm/mode belongs to E. T owns workload parameters and severity. I owns feature mapping and invocation. Source forms (path/Git/registry) are independent of invocation forms (Rust API/product ABI).

The content version selects the full catalog, not an individual suite or category. Every case remains in the expanded plan with validation and measurement applicability. Skips have stable reason codes and explanations. Build errors, fatal checks, crashes, interruption and missing expected outputs are failures or unexecuted work; they are not skips. A canonical-plan check prevents an edited skip from concealing a missing result. Native acceptance executes both configured schemes across registered integrations and verifies coverage.

Formal test content has four categories: correctness checks, individual operations, execution chains and integrated workflows. Encoding-plan generation is an engine stage within chain testing, not an atomic operation or fifth category. Plan reuse inside complete encoding is integrated testing. The registered formal content implements these four categories; driver and framework acceptance remain separate from component pass/fail results.

The [0.1.0 test plan and report guide](TEST_PLAN_0.1.0.md) defines the concrete formal cases, their relationships, measurement boundaries and report interpretation. The construction record states which combinations have actually executed and which are blocked by component failures.

## Source freezing and build composition

Local Git inputs include tracked and nonignored untracked files, including current modifications and deletions. Non-Git inputs retain their package/workspace layout. Git references fetch exactly the selected full commit, initialize submodules and reject a modified cached checkout. Registry references use an exact version and verify the crate archive against its Cargo lock checksum before extracting it into an owned snapshot.

Path dependencies are captured before computing a build-cache key, including dependencies outside the original repository. Snapshot checks detect concurrent file changes; source hashes include relative paths, bytes and executable bits. Monorepo-relative source includes remain intact. File symlinks within the snapshot are materialized; links escaping the snapshot and directory symlinks are rejected.

Generated build views preserve source files and record original/effective manifests. They expand workspace inheritance and bind selected native dependencies through paths. They do not edit upstream manifests or relax declared semantic version requirements. Native-only dependency feature choices are I-owned and recorded with Cargo's actual resolved features. Unmapped workspace patches/replacements fail explicitly.

Default public engine 2.1 integrations bind scheme and utility 2.1.0 releases. Engine 3 defaults to its monorepo packages. An explicit native scheme source is frozen, bound and checked against actual Cargo metadata. Product integration uses its bundled scheme and rejects external scheme substitution. The current Arena's exact engine 2.1 requirement rejects source engine 3. Successful compilation establishes concrete Rust type identity, and metadata checks reject duplicated/misbound engine identities in the driver graph.

Product integration builds the selected monorepo's real `fountain_bin_ffi` shared library and uses its RQ/R10 facades with SymbolOps callbacks into the selected operator. Binary-side dependencies and callback-side dependencies are separate graphs. Dynamic-loader traces and library hashes establish which shared library was actually loaded. A Linux SONAME alias and platform rpath support normal loading; resources are released through the real facade lifecycles.

Completed cache entries include manifests, source files, lockfiles, driver/library binaries, build logs and a checked metadata index. Reuse verifies these artifacts. Source closure, implementation, I/T identities and common conditions form the cache key. A new external path dependency revision cannot reuse an old build. Dependency resolution is locked per completed build; a fresh cache may resolve different allowed supporting releases, which are recorded in C provenance.

## Interface process boundary

Drivers communicate through a versioned NDJSON protocol on stdout; stderr and build output remain separate logs. The framework checks a hello identity, permitted event types, sample coverage and an explicit completion event. Each invocation uses a bounded child process group. Timeout, interrupt, nonzero exit and protocol errors produce failed/interrupted evidence and terminate remaining child processes.

Correctness and measurement are separate process invocations. Correctness can wrap operators with participation and destruction counters; measurement uses the raw selected operator. A product API without the full operation surface records those validation or measurement cases as capability skips while executing its supported codec and callback diagnostic cases.

The three integration families share the same T-owned input generation, scalar oracle, workload ordering, clock boundaries and metrics. Their native binding implementations are I-owned. This permits equal T identities across different API families without hiding the effect of a different interface path.

## Correctness severity and formal content

T owns the complete case matrix, scalar reference and fatal/warning policy. The 0.1.0 candidate has four categories and 2,182 concrete cases. The [test guide](TEST_PLAN_0.1.0.md) defines each family and its relation to the others. `src/content.rs` declares the cases; `drivers/workloads.rs` executes them; bindings remain in I templates.

Ordinary arithmetic, payload, ownership and evidence failures are fatal. Ambiguous alias/overlap/self-operation boundaries are observations: mismatches and caught boundary rejections remain warnings alongside their call timings. A panic outside those registered boundaries is fatal. There is no component-name-specific correctness exemption.

Three ordinary workers each execute one warmup and five recorded samples per applicable case. First-use cases instead use fifteen fresh processes. Inputs and output validation stay outside the measured scope. Ordinary builds have no allocator/profiling wrapper. Separate diagnostic binaries observe stage partitions and resources; their overhead remains visible and their timings never replace ordinary samples.

The same-flow exclusive stages plus an explicit residual equal that diagnostic lifecycle observation. Inclusive upstream solver counters are a separate tree, not extra summands. Hidden product stages remain combined. Standalone plan construction and recorded replay are independent measurements and cannot be added to reconstruct a separate full-encode sample.

Every successful run requires its full applicable validation, ordinary sample and diagnostic coverage. Bounded recovery may report an incomplete outcome with consumed work and no successful throughput. Construction failures and component failures preserve their prefixes and leave later work unexecuted. Actual coverage is documented in [formal construction validation](VALIDATION_FORMAL.md).

## Comparability and attribution

Two results can be compared over their common measured cases when both succeeded, all applicable validation/samples are present, T version and digest match, and E's common conditions match. Registered I implementations must support that T and agree on its timing and fatal-check contract; matching common fatal-check witnesses supply per-run evidence. Canonical applicability is verified before taking the intersection; missing applicable evidence cannot be hidden as a skip. Both coverage tables accompany the comparison, and no common measured cases means rejection. Warning observations are allowed to differ, are included in the comparison and do not participate in its rejection rules. Different I values are therefore not an automatic rejection, and a matching trait name is not equivalence evidence by itself.

E excludes component-specific dependency graphs, native feature mappings, binary hashes and internal implementation choices. Including these as common environment requirements would prevent the component comparisons the framework exists to support. It includes the common build recipe, toolchain/compiler details, relevant Cargo configuration digests, hardware/OS and observable power/affinity conditions. Run IDs, temporary paths, timestamps and uptime observations do not affect equality.

A C-only change is attributed to the selected component combination, including changed supporting dependencies when applicable. An I-only change is attributed to the interface path. When both change, the report states their joint effect. Comparing multiple simultaneous changes does not isolate a single optimization. Equal declared conditions do not eliminate background interference; retain dispersion and worker grouping.

## Evidence lifecycle and ownership

Each invocation creates a unique output directory. Checkpoints record stage state; failures retain already completed phases, live logs and any valid sample prefix. `config.toml` and `config-resolved.json` retain the supplied and normalized input. `test-plan.json` and `coverage.json` account for every registered case. `correctness.json` and the result record distinguish fatal failures from semantic warnings; a successful run with warnings remains eligible for measurement and comparison. Seals detect accidental evidence modification. `report` and `compare` can operate after build cache deletion. Prepared execution still requires its verified build cache.

The CLI does not upload, archive, retry into success, overwrite existing results or delete normal user results. Git ignores `runs/` and `.fountain-eval/`. Implementation acceptance artifacts are temporary: maintainers write an acceptance summary, then clean their explicitly identified test work directories. Private source access does not authorize upstream writes.

Linux/macOS CI checks the framework and publicly accessible native combinations. Private Arena and monorepo combinations are checked on an authorized local host. Platform coverage is explicit; missing CI credentials do not count as private-component coverage. The formal content and applicability rules are specified in the test guide; an unexecuted private or fatal-blocked combination is not positive coverage.
