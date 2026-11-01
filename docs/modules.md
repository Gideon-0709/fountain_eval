# Internal module roadmap

The framework remains one Rust package.

| Responsibility | Current location | Next boundary |
|---|---|---|
| Request/inspection | `src/protocol.rs` | Resolved identities and driver protocol |
| Declaration inspection | `src/runner.rs` | Orchestration over resolution/build stages |
| CLI | `src/main.rs` | Keep only engine/operator/test mandatory |
| Source/build | Planned internal module | Freeze sources, resolve dependencies, build isolated drivers |
| Measurement | Planned internal module | T-defined timing and sampling |
| Reference validation | Planned internal module | Check behavior independently of the tested algorithm |
| Analysis | Planned internal module | Validate I equivalence and T/E comparability before attribution |

Introduce modules when their responsibilities are implemented. Avoid empty crates, placeholder trait hierarchies and duplicate implementations of the tested codecs.
