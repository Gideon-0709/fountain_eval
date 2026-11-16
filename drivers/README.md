# Embedded native drivers

`common.rs` owns process dispatch and NDJSON emission. `workloads.rs` and `reference.rs` own T workload, scalar reference, correctness checks and timing. `contracts.rs` separates the V02 arithmetic oracle, V03 storage/ownership checks and V05 dispatch/composition relations. Its execution/observation boundary is implemented by `operator.rs`; fault-injection tests run these same checks against controlled defective subjects. `operator.rs`, `native.rs`, and `product.rs` own I-specific bindings. `src/catalog.rs` embeds these files so an installed CLI does not read an eval checkout.

`scalar_operator.rs` is an I-owned adapter for engine-only plan validation after timing; it uses the independent scalar reference and materializes implicit zero rows in symbolic XOR operations. It is not a production operator or a measured codec partner.

Runtime generation supplies a selected operator alias and immutable build/test identity and selected-scheme constants. The generated crate has its own Cargo.lock and release profile. The product family additionally builds the real shared library and calls the selected operator through SymbolOps. Product operation-surface gaps become explicit applicability skips; the driver must still emit every applicable observation. The CLI does not expose per-case selection.

See the [process contract](../specs/README.md) and [design](../docs/DESIGN.md) for capability, timing and equivalence boundaries. Native declarations and implementation code remain in their original repositories.
