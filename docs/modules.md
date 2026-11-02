# Implementation map

The repository contains one package and these responsibility boundaries:

| Location | Responsibility |
|---|---|
| `src/protocol.rs` | Normalized component/scheme request, reference grammar and offline inspection records |
| `src/config.rs` | Strict TOML input, config-relative paths, defaults and original/resolved input evidence |
| `src/coverage.rs` | Complete per-case validation/measurement coverage and skip reasons |
| `src/runner.rs` | Offline declaration inspection |
| `src/source.rs` | Immutable local/Git/registry snapshots and path dependency closure |
| `src/content.rs` | Pure T-owned expanded case definitions and deterministic matrix |
| `src/report.rs` | Offline HTML/Markdown/CSV and comparison views of evidence |
| `src/catalog.rs` | I/T registrations, supported families and embedded assets |
| `src/build.rs` | Workspace expansion, generated manifests, native builds and cache integrity |
| `src/process.rs` | Controlled build environment, subprocess logs, timeouts and process-group cleanup |
| `src/execution.rs` | Stage orchestration, prepared replay, capability and protocol checks |
| `src/validation.rs` | Registered fatal/warning severity, evidence checks and correctness reports |
| `src/measurement.rs` | Raw sample schema, units and deterministic summaries; part of T identity |
| `src/evidence.rs` | Common conditions, result records, seals and comparability |
| `src/util.rs` | Canonical hashing, file formats and common errors |
| `src/main.rs` | CLI parsing, report formats and exit codes |
| `drivers/common.rs` | Embedded process protocol, worker dispatch and event emission |
| `drivers/workloads.rs` | T-owned correctness, operation, chain and workflow execution |
| `drivers/reference.rs` | Independent scalar GF/state model and small Gauss-Jordan oracle |
| `drivers/telemetry.rs` | Diagnostic-only allocation accounting with explicit observation scope |
| `drivers/operator.rs` | I-owned operator binding and untimed participation wrapper |
| `drivers/native.rs` | Source engine 2/3 constructors, symbol conversion and materialization |
| `drivers/product.rs` | Real product facades and SymbolOps callbacks |
| `scripts/native_acceptance.py` | Maintainer native, failure and reproducibility checks |
| `tests/` | Framework contracts, source capture, evidence, units, processes and CLI tests |

Driver templates are embedded by `include_str!`. A generated driver is an isolated executable build artifact, not another published framework package. Native component code is never checked into this repository.

## Test content, build composition and fixtures

The test catalog expands the entire selected content version against the scheme and verified capabilities. Validation and measurement coverage remain separate; skipped cases carry reasons, and interrupted or failed execution leaves unfinished cases unexecuted. The [formal test plan](TEST_PLAN_0.1.0.md) defines correctness, individual-operation, chain and integrated-workflow responsibilities. Maintainer acceptance checks catalog reachability so registered content cannot silently become unused.

Build composition is maintained in `src/build.rs`. Users do not supply target manifests. Public integrations use registered support dependencies; development and product integrations use their selected source layouts. The generated view retains original/effective manifests, actual features and dependency graphs without relaxing upstream constraints. Product and callback dependencies remain separate provenance. Snapshots and generated builds belong in the ignored cache.

Deterministic inputs, scalar references and operation traces are embedded test content and participate in T identity. Framework tests create temporary synthetic inputs; `scripts/native_acceptance.py` creates negative operator fixtures inside its own work directory. These synthetic faults validate the framework and are not component performance measurements. A work directory named `fixtures` is a runtime artifact, not a repository-level source directory.
