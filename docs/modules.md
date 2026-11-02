# Implementation map

The repository contains one package and these responsibility boundaries:

| Location | Responsibility |
|---|---|
| `src/protocol.rs` | Normalized component/scheme request, reference grammar and offline inspection records |
| `src/config.rs` | Strict TOML input, config-relative paths, defaults and original/resolved input evidence |
| `src/coverage.rs` | Complete per-case validation/measurement coverage and skip reasons |
| `src/runner.rs` | Offline declaration inspection |
| `src/source.rs` | Immutable local/Git/registry snapshots and path dependency closure |
| `src/catalog.rs` | I/T registrations, supported families and embedded assets |
| `src/build.rs` | Workspace expansion, generated manifests, native builds and cache integrity |
| `src/process.rs` | Controlled build environment, subprocess logs, timeouts and process-group cleanup |
| `src/execution.rs` | Stage orchestration, prepared replay, capability and protocol checks |
| `src/validation.rs` | Registered fatal/warning severity, evidence checks and correctness reports |
| `src/measurement.rs` | Raw sample schema, units and deterministic summaries; part of T identity |
| `src/evidence.rs` | Common conditions, result records, seals and comparability |
| `src/util.rs` | Canonical hashing, file formats and common errors |
| `src/main.rs` | CLI parsing, JSON output and exit codes |
| `drivers/common.rs` | T-owned correctness, workload and timing implementation |
| `drivers/operator.rs` | I-owned operator binding and untimed participation wrapper |
| `drivers/native.rs` | Source engine 2/3 constructors, symbol conversion and materialization |
| `drivers/product.rs` | Real product facades and SymbolOps callbacks |
| `scripts/native_acceptance.py` | Maintainer native, failure and reproducibility checks |
| `tests/` | Framework contracts, source capture, evidence, units, processes and CLI tests |

Driver templates are embedded by `include_str!`. A generated driver is an isolated executable build artifact, not another published framework package. Native component code is never checked into this repository.
