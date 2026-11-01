# Protocol boundary

The draft request is engine + operator + test references. `src/protocol.rs` defines the implemented request and inspection models.

Inspection uses `format_version = 1`. It is an offline declaration report, not a resolved four-tuple or published I/T release. The root README documents reference syntax.

Before native execution, define resolved identities, capability negotiation, immutable recipes, environment observations and evaluation evidence. Keep these models independent of native Operation, CodeParams, DecodeStatus, GF tables and Rust trait objects.
