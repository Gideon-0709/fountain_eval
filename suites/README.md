# Test content and applicability

Users select the entire `0.1.0` content version in config. Its 26 registered cases are framework acceptance fixtures, not the formal team test suite. All cases are planned. No individual case or category selector is exposed.

`src/catalog.rs` defines canonical content, expands the configured scheme and interface capabilities, and supplies explicit reasons for inapplicability. Native APIs validate 22 cases and record 66 samples per scheme. Product ABI validates 16 cases and records 12 samples; its unexposed operator surface is reported as skipped. The four codec cases for the other scheme are always visible as scheme-not-selected skips. Boundary warnings do not fail execution or suppress comparison.

Validation and measurement coverage are separate. Failure/interruption leaves unfinished work `not_executed` or incomplete. A skipped check is not passed. Missing applicable observations fail evidence validation. Acceptance runs both schemes and requires the union of executed cases to cover the entire catalog, preventing unreachable fixtures from remaining unnoticed.

Formal test categories are correctness, individual operations, execution chains and integrated workflows. Encoding-plan generation belongs to chain tests; plan reuse in complete encoding belongs to integrated tests. The [0.1.0 test plan and report guide](../docs/TEST_PLAN_0.1.0.md) describes the proposed cases, matrix and methodology for review; their implementation remains pending.

This is an unreleased internal 0.1.0 candidate. Content digests identify each candidate; the owner controls the first release. See the [usage guide](../README.md).
