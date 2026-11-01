# Embedded native drivers

`common.rs` owns the shared T workload, scalar reference, correctness checks, timing and NDJSON emission. `operator.rs`, `native.rs`, and `product.rs` own I-specific bindings. `src/catalog.rs` embeds these files so an installed CLI does not read an eval checkout.

Runtime generation supplies a selected operator alias and immutable build/test identity constants. The generated crate has its own Cargo.lock and release profile. The product family additionally builds the real shared library and calls the selected operator through SymbolOps. Unsupported product operator-only tests fail before measurement.

See the [process contract](../specs/README.md) and [design](../DESIGN.md) for capability, timing and equivalence boundaries. Native declarations and implementation code remain in their original repositories.
