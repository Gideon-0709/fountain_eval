# Fountain evaluation tests

This guide describes the implemented, unreleased **0.1.0** test content. It explains the fixtures, correctness expectations, measurement boundaries and relationships between results. Configuration, component selection, integration and evidence lifecycle are defined in [DESIGN](DESIGN.md); commands and batch reports are documented in the [README](../README.md).

The full catalog contains 2,511 concrete cases across RQ and R10. Each selected C receives the same catalog with explicit applicability. The configured scheme and available component surfaces determine which cases execute. Engine-only C executes standalone plan construction and operation-list cloning; operator-only C executes the pure-operator surface. Missing components, unavailable capabilities and the R10 K<4 domain are reported as skips. Applicable cases blocked by a fatal failure remain unexecuted.

## Test categories and notation

| Category | Question | Main evidence |
| --- | --- | --- |
| Validation tests (V01–V11) | Does this input produce the required result, and which boundary behaviors differ from the reference? | Fatal checks, non-blocking warnings, expected/actual values and lifecycle observations. |
| Operation tests (O01–O09) | How expensive is one public operation under a specified state and working set? | Time per call, calls per second and explicitly defined logical-byte rates. |
| Link tests (L01–L12) | Where is time and work spent along an actual coding path? | Stage durations, operation composition, plan construction/replay and resource diagnostics. |
| Workflow tests (W01–W08) | How fast does the component combination complete a useful coding task? | Output-ready and lifecycle durations, throughput, first-use cost and sustained behavior. |

**C** is one selected engine/operator combination or a single component. **I** identifies the calling interface, **T** the test content, and **E** the common experiment conditions. In workload notation, **B** is bytes per symbol, **K** the application source-symbol count, **K'** an internal padded source count, **R** generated repairs and **M** omitted sources. Symbol width always uses B, avoiding confusion with T. A case is one concrete parameter and capability expansion of a test family.

The categories share deterministic inputs and correctness rules, but measure different boundaries. A scalar reference validates operations. Recorded real operation sequences connect those operations to coding chains. Link tests explain workflow performance. Workflow performance remains the directly measured outcome rather than a sum of separate microbenchmarks.

```mermaid
flowchart TD
    Inputs[Fixed config, inputs and capabilities] --> Validation[Validation tests V]
    Validation --> Operations[Operation tests O]
    Validation --> Links[Link tests L]
    Validation --> Workflows[Workflow tests W]
    Operations -. operation costs .-> Links
    Links -. stage and work evidence .-> Workflows
    Operations --> Report[Coverage, results and comparison report]
    Links --> Report
    Workflows --> Report
    Validation --> Report
```

Encoding-plan generation belongs to link testing because it combines parameter processing, symbolic solving and instruction construction. A plan-generation timing can appear as its own row without becoming an operation-test category. Reusing that plan across complete blocks belongs to workflow testing. There is no fifth test category for plans.

The labels V01, O01, L01 and W01 below are family references. **V** comes from **Validation**, **O** from **Operation**, **L** from **Link**, and **W** from **Workflow**. A link test follows connected stages along the coding execution path. C remains reserved for component selections in the CITE tuple. Each expanded case carries its family, category, concrete parameters and requirements; a chain can cover several families through exclusive stages and separately labelled internal observations.

## Fatal and warning expectations

Fatal rules cover wrong output under a defined contract, cross-object data corruption, false completion, invalid resource ownership, crashes on valid inputs, timeouts and malformed or missing required evidence. Warning rules cover explicitly registered ambiguous boundary semantics, including same-ID XOR and repeated-source XOR.

A warning cannot be promoted to fatal merely because two operators disagree. It cannot be downgraded or hidden to make an operator appear clean. Conversely, an end-to-end wrong decoded payload is fatal even if a separate boundary test classified the triggering primitive's ambiguity as warning.

Reference-matching and reference-deviating implementations are both timed. The framework does not equalize their internal work, suppress a fast no-op result, or reject a comparison solely because warning observations differ.

A fatal validation stops measurement for the affected C; other configured C entries still execute. Completed checks and unfinished cases remain visible in the final report index. No automatic retry, enlarged repair budget or relaxed timeout turns that attempt into a pass.

## Validation tests (V01–V11)

### V01 — Selected component and scheme participation

Verify source identities, resolved dependency bindings, enabled native features, actual shared-library loading where applicable, scheme parameters and finite-field configuration. Independent untimed encoder and decoder probes record participation. Each must show its own expected read/write path. The selected implementation must receive the work expected by that scenario; a genuine zero-arithmetic systematic receive path is not required to invent arithmetic calls. It still must demonstrate the expected storage/access path.

Wrong bindings, silent fallback and incorrect requested configuration are fatal. Product binary-side and callback-side provenance remain separate. A callback observation proves callback participation, not that all work inside the product library was delegated to it.

### V02 — Independent arithmetic values

V02 owns the numeric result of the elementary arithmetic calls. It runs **direct calls** for single-source XOR, multiplication by alpha, scalar multiplication and multiply-add. Scalars are 0, 1, 7 and 255. Each call starts from independent initialized storage using zero, 0xff and deterministic pseudorandom bytes at every correctness width. Sources and targets are distinct where the operation has both. The independently implemented GF(256) reference uses polynomial 0x11d, not the tested implementation's tables or kernels.

Compare every expected live symbol byte-for-byte, including untouched sources and neighbors. Any mismatch is fatal. For `dst ^= scalar * src`, scalar zero must preserve the destination. At B=1, direct scalar multiplication additionally exhausts all 256 byte values against all 256 scalars. Successful evidence names the checked operations, data patterns and ordinary/exhaustive check counts.

Zero initialization and storage belong to V03. Multiple-source XOR, broadcast and range operations are established through elementary-call composition in V05. V05 also covers `execute`, so V02 does not repeat the full oracle through both entry points. V06 retains ambiguous alias/overlap cases and its warning policy.

### V03 — Storage, copying, moving and ownership

V03 owns **direct-call storage behavior**: insertion at a new ID, replacement at an existing ID, reading, single-symbol zeroing, single-symbol copy/move, removal and informational association. It uses the same correctness widths and three data patterns as V02. Each operation starts from independent initialized storage; required values and untouched live symbols are checked against the state model.

A copy is followed by an in-place zero write and replacement of its destination, then an in-place zero write and replacement of its source. The zero-write probes use scalar multiplication by 0 and require stored zeros while all other symbols remain unchanged. This checks writing the copied storage, including deferred copy behavior, without repeating V02's GF value matrix. Replacement checks the new stored bytes as well as isolation. After move or removal, insert a new payload under the former source/removed ID and verify that the destination and other live symbols remain correct. Payload, isolation and reuse violations are fatal.

The native trait has no safe membership query. The check does not read a removed/moved ID before reinsertion and does not claim to prove its absence. The informational association has no query API either; only preservation of symbol bytes is observable. Both limits are explicit in successful evidence. Explicit sharing and first-write checks remain in the applicable O08 premeasurement checks. Batch storage and alternate dispatch belong to V05; process/resource destruction belongs to V11.

### V04 — State histories across mixed operations

V04 checks interactions caused by preceding operations. It applies a complete, predetermined trace to an independent scalar state model and the selected operator, comparing every known-live symbol byte after **every public call**. No implicit state-restoration calls occur between trace steps. The histories use defined-domain operations; ambiguous aliases and overlapping ranges belong to V06.

| Scenario | History and coverage | Calls per trace |
| --- | --- | --- |
| `copy-chain` | Eight rounds of chained copies, writes to source/destination, XOR/GF arithmetic, movement, replacement, deletion and reuse. Exercises deferred copies and later mutations. | 144 |
| `move-reuse` | Sixteen rounds move a live symbol, reuse its former ID, remove/recreate the destination, copy and modify it, then move over an existing destination. | 208 |
| `sparse-churn` | Grow from 16 to 80 live symbols using dense and sparse IDs, remove half of the added symbols, reinsert them in reverse order, then churn sparse IDs through temporary ID 2048. | 352 |
| `seeded` | Generate 256 operations using current ID liveness. Includes insertion, zeroing, copy/move/removal, read, alpha/scalar multiplication, multiply-add, single/two/multiple-source XOR and broadcast. Arithmetic operands are distinct, broadcast targets are unique, and removed IDs are not read. | 256 |

Each trace runs from fresh identical initial bytes through three routes: direct methods, `execute`, and alternating direct/`execute`. Insertion and lookup have no instruction-enum equivalent and always use their sole public entry. Each route is checked against the independent state model, not merely against another route. Trace digests exclude the route, allowing identical inputs to be verified across routes.

All 19 correctness widths apply. Directed histories use seed `0x1971`; the generated history uses all three registered seeds. This produces `(3 + 3) × 3 × 19 = 342` cases. Each step has its own deterministic input seed for replacement bytes. The generator runs before component execution and has no dependency on observed component behavior. These are synthetic state histories, not real engine plans; L11/L12 own real operation replay.

Any defined-domain mismatch, unreadable expected-live symbol or component panic is fatal. Failure evidence records scenario, B, seed, route, one-based step and total calls, actual dispatch, action/operands, per-step input seed, trace and pre-state digests, the preceding eight actions, and the first differing symbol/byte with expected/actual values and lengths. The visible report shows the first failure and replay identity; its result index retains the bounded preceding trace. A panic retains the same step context. Success records exact checked-prefix/call counts, direct/execute counts, peak live-symbol count and maximum ID.

The histories exercise logical growth and reuse. They do not prove physical reclamation or absence of deleted IDs: the current safe interface has no membership query. Counts describe this bounded workload, not exhaustive state-machine coverage.

### V05 — Dispatch and batch composition equivalence

V05 owns relationships between equivalent public calls. It compares observed states directly and does not recompute scalar reference values. Each pair starts from identical independent states at every correctness width and each of the three data patterns.

| Relation | Calls being compared | Fatal condition |
| --- | --- | --- |
| Dispatch | A direct method versus `execute` for every registered operation with an instruction-enum equivalent. | Any expected live symbol differs after the call or the storage follow-up probes. |
| Zero composition | List/range zeroing versus successive direct single-symbol zero calls. | Different live symbol bytes. |
| XOR composition | Two-, three- and general multiple-source XOR versus successive direct single-source XOR calls. | Different live symbol bytes. |
| Broadcast composition | One broadcast versus direct single-source XOR into each destination. | Different live symbol bytes. |
| Copy/move composition | A disjoint range call versus successive direct single-symbol copy/move calls. | Different bytes immediately, after copy-isolation probes, or after source-ID reuse. |

Insertion, lookup and explicit sharing have no instruction-enum counterpart and therefore do not create dispatch pairs. The inputs use distinct XOR operands, unique broadcast destinations and disjoint copy/move ranges. V06 owns ambiguous boundaries. Copy probes modify both sides in place and by replacement; move/removal probes reinsert former IDs on both sides. Observation uses known-live IDs without probing invalid ones.

Successful evidence records the number and kind of comparisons and the observation limits. A direct method may delegate to `execute`; agreement still checks the exposed contract, not whether a fast override exists. Two identically wrong primitive implementations can agree here: V02/V03 provide their independent value/ownership checks. Conversely, a batch defect shared by direct and `execute` is detected by comparison with the elementary sequence.

The dependency is explicit: **V02 establishes arithmetic primitives; V03 establishes storage primitives; V05 establishes their alternate entry points and compositions.** These are different assertions, even when a comparison must execute a primitive again. Family numbering does not prescribe execution order; any fatal prevents successful measurement for that C.

### V06 — Boundary contracts and semantic observations

Each registered boundary carries its expectation, contract basis, permitted affected IDs and severity policy. The same definition drives correctness and performance prechecks. Definitions are based on the registered public engine 2.1 and development engine 3 `DataOperator`/`Operation` surfaces. Explicit API requirements, T-derived arithmetic rules and unresolved semantics are distinguished below; observed agreement between implementations is not a contract source.

In this table, X and Y denote original symbol contents. All inputs are live, initialized symbols.

| Case | Required result or observational reference | Contract basis and deviation severity |
| --- | --- | --- |
| `self-xor` | `X ^= X` produces zero. | Same-ID alias behavior is unspecified; the agreed T policy retains **warning**. |
| `duplicate-source-xor` | `Y ^= X ^ X` leaves Y unchanged. | Duplicate-source handling is unspecified; the agreed T policy retains **warning**. |
| `target-in-sources` | `Y ^= X ^ Y` produces the original X using pre-call sources. | Source/target snapshot order is unspecified: **warning**. |
| `duplicate-broadcast-target` | Two XOR writes to the same target cancel. | List occurrences versus deduplicated destinations are unspecified: **warning**. |
| `self-copy` | The symbol stays readable and byte-identical. | `Operation::CopyTo` requires keeping the source intact; coincident IDs do not conflict with this requirement: **fatal**. |
| `self-move` | Preserve the symbol. | `MoveTo` invalidates its source; precedence for coincident IDs is unspecified: **warning**. |
| `empty-xor` | The existing target stays readable and byte-identical. | T derives the empty XOR fold as adding zero. This is a test contract, not an explicit upstream empty-list guarantee: **fatal**. |
| `overlapping-copy` | Snapshot IDs 0..2, then write their original bytes to IDs 1..3. | `CopyToN` does not define overlap order or snapshot behavior: **warning**. |
| `overlapping-move` | Snapshot IDs 0..2, remove that range, then write IDs 1..3. | `MoveToN` does not define overlap order: **warning**. Source absence is not observable. |

The correctness matrix contains `9 × 19 widths × 2 routes = 342` cases: 76 defined-contract and 266 ambiguous-contract checks. The performance matrix additionally contains `9 × 8 widths × 2 routes × 2 working sets = 288` cases with the same policy. A boundary that matches its reference passes; only a deviation creates a warning. Direct and `execute` differences remain under that boundary's policy rather than becoming a V05 equivalence failure.

**Scope protection is always fatal.** A warning permits different behavior only at the registered affected IDs. Every other initialized symbol must remain readable and byte-identical, including after a caught boundary-call panic. Protected symbols are checked outside timing on validation and on every measured instance. Thus a no-op, snapshot difference or ambiguous-target rejection can be a warning, while corruption of an unrelated symbol cannot be hidden as one. Copy-source ID 0 remains protected in overlapping-copy; overlapping-move permits differences at IDs 0..3.

For defined cases, wrong bytes, unreadable output and call rejection are fatal. For ambiguous cases, a caught panic is recorded as `rejected_by_panic`; unreadable affected output is `output_unreadable_after_boundary_call`. Unrecoverable process failure and timeout remain execution failures. Boundary timing includes the common panic-catching and rejection-flag bookkeeping wrapper; state comparison and reporting are outside the timer. A rejected call reports rejection latency, not successful operation throughput. Nominal calls are still counted without normalizing away a fast return.

Warnings retain expected/actual observations and the per-case contract basis. They do not block measurement or comparison solely because components differ. Incorrect end-to-end recovered bytes remain fatal. V03 continues to own defined copy-isolation requirements.

### V07 — Scheme, source count and padding

V07 checks the declared geometry and application output at each registered K and at B=31 and B=128. It compares the first **internal** repair ID against the independently calculated total intermediate-symbol count. The RQ fixture uses the relevant K′, S and H rows from [RFC 6330 Table 2](https://www.rfc-editor.org/rfc/rfc6330.html#section-5.6). The registered R10 fixture uses the [RFC 5053 intermediate-symbol construction](https://www.rfc-editor.org/rfc/rfc5053.html#section-5.4.2.3) with K′=K. Wire-format encoding symbol IDs require a separate mapping and conformance check.

The native observation includes K′ from the selected scheme and identifies the adapter as the provider of zero pad symbols. The product surface exposes the repair base and output but leaves the internal padding operation unobserved. Every systematic output is checked once at its ID, with its exact width. The full source set verifies the application output has exactly K × B correct bytes. This check has no repair generation or decoder-progress assertions. Geometry, mapping and byte mismatches are fatal.

### V08 — Plan application and reuse

At K=64 and B=128, load three distinct payloads into one prepared context. For each payload compare the internal repair base, source symbols and the first eight repairs with a rebuilt encoder. Differences in width, bytes or symbol domain are fatal and identify the load round and symbol. The check establishes reuse/rebuild equivalence. V09 provides the independent arithmetic check.

Native drivers retain and apply a public recorded operation list. The product case observes the same encoder accepting three loads through its existing source-replacement API. Its internal plan reconstruction is unobservable. This fixture fixes geometry and does not exercise cross-instance plan import. L02 and W03 own planning and reuse timings.

### V09 — Encoding and recovery mathematics

At K=8 and B=31, the selected encoder supplies coefficient rows using a basis payload with zero inactive lanes. The coefficient and data encoders use the same symbol width. The scalar reference computes each output from its row and the source bytes, independently of the selected operator. An incremental reference rank check rejects false decoder completion on any rank-deficient prefix. An independent Gauss–Jordan solve of the received rows verifies recovery, and the decoder's materialized output must agree with the original payload.

The stream omits source IDs 0, 3 and 6, then offers up to 128 repairs. If that prefix lacks rank K and has not completed, the fixed positive control supplies the withheld sources and records the rank at budget exhaustion. Rank K at the end of the repair budget without completion, wrong encoded or decoded bytes, and completion below rank K are fatal. Since the selected encoder supplies the coefficients, this establishes linear coding and recovery consistency for the tested IDs. Independent wire-format vectors remain outside this fixture.

### V10 — Ordering, duplicates and incremental progress

At K=64 and B=128, test seven arrival sequences: all sources, repeated sources, a fixed eight-source omission with source-first order, the same omission and repair set in mixed order, that mixed sequence with identical source and repair duplicates inserted, repairs only, and repairs before the full source set. The mixed pair preserves the order of the distinct IDs. Fixture generation verifies repairs against basis rows and independent scalar arithmetic so rank decisions apply to the actual offered bytes.

Each new decoder starts incomplete. For every distinct ID, insert its row into the independent reference rank. A duplicate adds no information. At reported completion require rank K and exact recovered bytes. A rank-K stream that finishes offering inputs without decoder completion is fatal, as are false completion and output corruption. A rank-deficient exhausted stream is a valid incomplete observation. Preserve the scenario, rank, offered and consumed counts and duplicates. Fatal observations include the scenario, arrival index, symbol ID and first bad byte. Calls after completion and conflicting payloads for one ID remain outside the fixture.

### V11 — Errors and resource lifecycle

Observe each encoder and decoder separately. Check release of the encoder, a decoder abandoned after a partially loaded prefix, and a decoder that produced an owned result. Verify that result after component release. Native release counts observe the public operator wrapper; the product counts observe callback ownership.

The product negative fixture checks callback-error propagation and callback release on failed construction. Invalid replacement length and invalid received-symbol width must return `InvalidArg`, then a valid replacement and a valid input stream must still work. R10 also checks rejection of K=1. Native methods lack a recoverable invalid-input contract, so these rejection checks are unobservable there. Ownership violations, wrong output and documented errors that fail to propagate are fatal. Allocation and retention are separate diagnostic observations, with product library allocations outside the callback-side allocator's measurement scope.

V07–V11 define no warning-only exceptions. An unavailable optional capability remains an explicit skip. Rank-deficient and budget-exhausted observations keep their own status.

## Operation tests (O01–O09)

### Common boundary

An individual operation is one public operation semantic, not every internal function or machine instruction. A range operation can process several symbols and still be one public call. Reports therefore include both calls and the operation's width, arity or range length.

Prepare inputs and required storage before timing. For `execute`, prepare the operation object before timing; instruction-list construction and allocation belong to link tests. Stop the timer before inspecting outputs. Preserve the operation's actual effects and prevent compiler elimination without adding validation to the timed region.

Use independently initialized states where repetition would alter the operation being measured. Repeated XOR on one target, removing an already removed ID or copying an already shared value must not silently replace the intended workload. State restoration is outside the timer and its cache effects must be standardized and recorded.

| Test | Operation and variants | Timed work and interpretation |
| --- | --- | --- |
| O01 | Insert into a new ID; overwrite an existing ID. | The public insertion call into fresh ID 32 or existing ID 1 in a 16-symbol state. Record that fresh insertion may allocate payload or grow the index. Materializing fixture bytes is outside the timer. |
| O02 | Retrieve a stored symbol reference. | Lookup/reference-return cost, including saving references in a preallocated array for post-timing byte verification. No payload copying is timed. |
| O03 | Zero one symbol, an ID list or a consecutive range. | The selected zero operation on existing symbols. List and range cases clear four symbols. The fixture measures clearing preloaded storage. |
| O04 | One-, two-, three- and general multi-source XOR. | One public call with fixed arity. The 2- and 3-source list controls use the same operands and initial states as their specialized calls. V06 owns alias-boundary policy and rejection observations. |
| O05 | Broadcast one source into multiple destinations. | One call into four or eight distinct preloaded destinations. Arity is part of the test case and the operation recipe. |
| O06 | Multiply by alpha, multiply by scalar and multiply-add. | Arithmetic after GF(256) setup with polynomial 0x11d. The cases keep scalar 0, 1, 7 and 255 distinct. Field setup is outside the timer. |
| O07 | Single and range copy/move. | Single and four-symbol nonoverlapping calls replace existing destinations. Elapsed time reflects the component storage policy, including delayed copy behavior if present. |
| O08 | Establish sharing; first write after sharing. | Measure creation and the first GF write separately. Compare the first write against a private destination containing identical bytes. Applicable through the registered sharing surface. |
| O09 | Remove a live symbol; informational ID association. | Separate deletion and informational notification rows. The API exposes no safe membership or association query. Removal timing includes any physical release occurring within the call. |

Operations with instruction-enum counterparts use both direct and `execute` paths. Insert, lookup and explicit sharing have one public entry and therefore no alternate-path case. A missing required operator surface produces explicit applicability skips. Applicability uses the registered interface capability. An implementation declining sharing under that registration reports an explicit preparation or validation failure for the case. The fixture does not infer physical sharing from time alone.

Working-set variants use batches of independently initialized operator instances: up to 64 for the small set and 512 for the large set, capped at 64 MiB of initial symbol bytes. Each instance receives one measured call. Width-tail cases exercise non-aligned lengths; this catalog does not vary caller buffer offsets or claim a particular cache residency. Internal alignment is a component property.

The one-instance precheck is shared by the small and large batch rows for the same operation, width and entry point. Every measured instance still receives its output check. V06 observations keep the outcome and timing distinction between completed calls and rejected calls. Batch timings include the timer loop and entry dispatch. O04 and O06 indicate primitive arithmetic cost. O01/O02/O07/O08/O09 explain why a fast arithmetic kernel may still produce a slow operator or codec. No arithmetic-kernel-only result substitutes for the public-operation measurement.

## Link tests (L01–L12)

### Stable stage meanings

Stage names describe work, not a particular engine version's function layout. I maps actual calls and exposed observations to these meanings. Components may fuse, defer or reorder stages while preserving their public contract. The framework follows the actual execution path and does not force a less efficient path merely to obtain more columns.

| Test | Processing stage | Required result and relationship |
| --- | --- | --- |
| L01 | Scheme and context preparation. | Parameter preparation, field setup, operator/storage creation and encoder/decoder context creation, split where observable. Explains costs excluded from preloaded O tests. |
| L02 | Encoding-plan generation. | Time from the required planning inputs to a usable symbolic plan, excluding payload execution. Include solving and instruction construction. Report plan size and operation composition where exposed. |
| L03 | Clone the retained operation list. | Time the clone alone and verify its count and content digest. Binding remains with plan application. This case keeps the `plan-export` ID for the existing engine-only selection. |
| L04 | Source loading and padding. | Input import, ID assignment, allocation and zero-padding required for this block. Report actual and padded source counts. |
| L05 | Plan application / intermediate-symbol computation. | Apply the prepared work to payload storage. Include operator calls actually made by this path. Link to its operation histogram and O measurements. |
| L06 | Requested symbol generation. | Source/repair selection, coefficient or degree-set work, instruction generation/dispatch and payload operations for the specified outputs. Report first-output and subsequent-output costs when observable. |
| L07 | Decode input ingestion. | Received-symbol registration, storage and duplicate handling. Some engines perform solving within this call; the parent interval must retain that combined cost. |
| L08 | Decode constraint and solve progression. | Constraint construction, solve attempts and recovered-data computation. Use supported internal observations for finer attribution; solver-specific names are optional diagnostic children. |
| L09 | Output materialization. | Produce the owned application output from internal storage, including actual copying and allocation. A borrowed view is a different boundary and cannot replace this row silently. |
| L10 | Component release. | Destroy coding contexts and their internal operator/callback resources after output handoff. Caller-owned output remains valid for verification outside timing. |
| L11 | Recorded encoding-operation replay. | Generate and capture the real operation sequence outside the replay timer, then execute it against fixed payload state. Isolates the execution portion of encoding. |
| L12 | Recorded decoding-operation replay. | Capture an actual decode scenario's sequence and replay it under its required state. Different loss/order scenarios can yield different programs; retain that provenance. |

L02 constructs a new symbolic plan per observation, including scheme setup, extracting operations and setting the execution mode. Outside the timer, apply the retained plan and compare repair outputs with a rebuilt encoder. The independent arithmetic check remains V09. L03 validates the cloned list and separately checks application of the source plan. W01 separately measures fresh-process complete encoding; there is no dedicated fresh-process plan-only row. A separate cached-plan hit must not masquerade as fast plan generation. A fresh process controls process-local caches, but does not establish cold CPU caches or a cold OS page cache.

Plan generation reports elapsed time and plans/s, plus operation counts and logical plan storage where available. It does not report payload Gbit/s for a symbolic task independent of symbol bytes. Serialized plan size, allocated plan memory and peak construction memory are different metrics.

### How replay supports attribution

An identical replay sequence, initial state, dispatch path and finite field can isolate operator execution differences. Save the sequence digest, operation histogram, IDs, width and state-generation identity. The chosen replay sequence must be derived from actual registered coding work rather than a favorable hand-picked mix.

Different engines may generate different valid sequences. Their replay comparison then includes changed work, not only different operator speed. Report instruction-count and composition changes alongside elapsed time. A drop from 10,000 operations to 7,000 operations must not be described solely as a faster kernel.

Replay includes input insertion and field calls. Each encoding output is verified at its recorded output checkpoint, before later operations can overwrite it. A decode capture can consume its full input without completing. L12 then measures replay of the recorded work with `capture_decoded=false` and zero recovered-output checkpoints. Replay reports call-time metrics. Its payload-throughput denominator is disabled. When the selected integration lacks the recorded-operation surface, replay is skipped with the capability reason; its normal coding workload still runs. An independently generated surrogate program cannot be silently substituted for that engine's real program.

### Observable and combined stages

Use public call boundaries, existing profiling facilities and optional wrappers in the generated driver. Do not patch upstream algorithm source to force a decomposition. Profiling support can expose implementation-specific children such as sparse reduction, inactive-system work or back-substitution, but the stable report keeps them under the corresponding semantic parent.

Current source interfaces support recorded operation paths. Some development interfaces provide internal profiling. Product creation can combine plan construction, loading and execution behind one API, while source replacement exposes a reuse path. These differences affect observability and I provenance, not the definitions of correctness and useful output.

The single-block W02 eight-repair case provides the representative encoding chain. The W05 10%-omission, source-first, first-seed case provides the representative decoding chain. Both carry their L families and use the same ordinary result as the workflow. Separate diagnostics require each declared interval to be present and report the combined parent where appropriate. Every planned stage must be represented by a measured interval, an identified combined parent, or an explicit unavailable diagnostic. Stage unavailability does not imply that the work takes no time. If a supported required observation is missing, treat it as an evidence failure rather than inventing a skip after execution.

## Workflow tests (W01–W08)

| Test | Workflow | Setup, timer and output |
| --- | --- | --- |
| W01 | First-use encoding. | In a fresh worker process, measure construction through the first batch of eight owned repair outputs. Process startup is outside the timer and has no separate benchmark row. Library loading is included when it occurs inside the measured constructor; its cost is not separately isolated. No warmup precedes this first-use sample. |
| W02 | Rebuild for every block. | Construct scheme/context/operator as prescribed, import input, compute and materialize the requested output for each block. Process-level tables may already be warm; label this as per-block reconstruction rather than universally cold execution. |
| W03 | Reuse a plan for independent blocks. | For the single-block case, load a fixed predecessor before each timed replacement with different bytes. For continuous reuse, build once and load 32 distinct blocks. Report initial construction, first block, later blocks and amortized cost separately. State whether reuse is component-owned or composition of an exported public plan. |
| W04 | Receive all source symbols. | Pre-generate valid inputs outside the timer. Construct a decoder, offer only K systematic symbols in ordered or deterministically shuffled order, and materialize the exact block. Completion is required. Valid fast paths that avoid solving retain their performance advantage. |
| W05 | Recover missing sources. | Pre-generate a deterministic received sequence. Time decoder construction through recovery and owned output, with the specified missing-source set and ordering. Record symbols consumed and repairs required at completion. |
| W06 | Encode-to-decode round trip. | Directly time encoding, the fixed in-memory omission/order procedure and decoding as one workflow. Use the same omission and order generation as W05 for the matched geometry, omission count and seed. The encoder generates the complete declared repair budget. Report the harness delivery portion. This is not a network transport benchmark. |
| W07 | Consecutive independent blocks. | Process 32 distinct payload blocks using the prescribed reconstruction or supported reuse policy. Record first/later readiness, gaps, final release and total batch duration. Input fixture bytes are prepared once per worker case and all inputs and outputs remain owned until validation. The native scope includes continuous encoding. The policy is part of the case. |
| W08 | Complete component lifecycle. | Derive release and lifecycle endpoints from the relevant encode, decode and round-trip samples. The former independent eight-repair lifecycle case shares the W02 run. The owned output has been handed to the caller and is retained for later checking. Distinguish this endpoint from output-ready time. |

Encoding output cases separate source-symbol materialization and repair generation. For repair cases, the selected R determines the work; it must be visible next to the throughput. Decoder input preparation is excluded from W04/W05 so encoder cost cannot leak into decode throughput.

W03 uses actual advertised reuse capabilities. Eval may compose a plan explicitly exported through a supported interface, but it must label that path and include all binding/loading work in its proper scope. It must not reimplement the coding algorithm to manufacture a reuse result. A missing reuse API causes that case to be skipped, while W01/W02 remain applicable.

W07 measures useful-block continuity as well as isolated task latency. A lower output-ready time can coexist with slower release or a larger gap before the next block. Sustained throughput is total completed source bytes divided by the directly measured batch interval, not an average of individual block throughput ratios.

## Workload matrix

The following dimensions expand only the families that consume them; they are not a Cartesian product across all tests.

| Dimension | Values or expansion rule |
| --- | --- |
| Main link/workflow geometry | K = 64, 512, 5008; B = 128, 1016; all six combinations. |
| Operation-test widths | B = 16, 31, 32, 33, 128, 1016, 4096, 16384. |
| Additional correctness widths | B = 1, 15, 17, 63, 64, 65, 127, 129, 1023, 1024, 1025, plus the operation widths where relevant. |
| Validation K coverage | K = 1, 4, 9, 10, 11, 63, 64, 65, 511, 512, 513, 5007, 5008, 5009, 5120; B = 31, 128. R10 K=1 is outside its domain. |
| Repair generation | R = 1, 8, ceil(0.1K), deduplicated; source-output materialization is a separate case. |
| Main decode omissions | M = 0, 1, ceil(0.01K), ceil(0.1K), deduplicated. |
| Main reception order | Remaining sources followed by repairs; fixed shuffled mixture of sources and repairs. |
| V10 reception | Missing first eight sources, repeated source and repair IDs, repairs first, repair-only recovery and prefix-rank checks. Post-completion calls are outside this fixture. |
| Payload patterns | V02/V03/V05 use zero, 0xff and pseudorandom bytes; V04/V06 and codec workloads use deterministic pseudorandom inputs. |
| Workload randomness | Seeds `0x1971`, `0x2003`, `0x5008`; SplitMix64 bytes and Fisher–Yates order. Three seeds apply to generated V04 histories, V01/V08–V11 and W04/W05. Other cases use the first seed. |
| Consecutive-block count | 32 different payload blocks per registered continuity case. |
| Repair budget | Zero for W04. For the registered missing-source tests, max(128, 2M + 64). Track the offered domain and consumed prefix separately. |

This is a layered matrix, not a full Cartesian product of every row. Main coding performance covers the six geometries and prescribed output/recovery cases. Width-tail and state-boundary rules expand only relevant checks. For example, a plan-generation case whose contract is independent of B should not duplicate the same work and present the duplicates as additional evidence.

The executable expansion is in `src/content.rs` and each report's resolved plan in its result index. Operation widths and arities expand independently of codec geometry; plan generation has no payload-width dimension. The catalog retains actual seeds, operands and work counts, including each applicability reason.

Only the configured scheme runs. Both schemes are exercised by maintainer acceptance of the catalog. Scheme-specific reference fixtures may differ, but a baseline/candidate comparison for the same case uses identical inputs. Repair IDs, omission sets, ordering and payload-generation identities belong in evidence.

Stopping at decoder completion is part of the registered recovery workflow, not a changing test budget. Record how much of the fixed offered stream each implementation consumed. A changed amount of work can explain changed time. Budget exhaustion is preserved without automatic extension; no successful decode throughput is emitted for an incomplete block.

## Timing, sampling and resource diagnostics

### Two observation modes

Every applicable workflow gets a minimally instrumented timing run for its main performance result. A separate diagnostic run supplies stage, operation and resource observations. These modes use the same declared input/work definition but have distinct observation settings and provenance. Their samples are not pooled.

Diagnostic wrappers, profiling features and allocation tracking can perturb allocation, dispatch and cache behavior. Measure and disclose the difference from the minimally instrumented run. A negative observed difference is possible due to measurement noise or other execution effects; it is not evidence that instrumentation has negative cost. Neither an assumed zero overhead nor a subtraction-based correction replaces the actual main result.

Input generation, correctness comparisons, logging and report serialization are outside main timers. Work required by the caller-visible operation, such as copying its output into owned storage, remains inside the relevant timer. Avoid logging, filesystem writes and reference-oracle work in measured loops. Perform checked output consumption afterward.

### Additive stage accounting

Use monotonic timestamps within one process and one sample. On a serial path:

```text
flow_time = sum(exclusive_observed_stage_times) + unattributed_time
lifecycle_time = output_ready_time + subsequent_component_release_interval
```

The second equation applies when release follows output handoff in the same measured flow without an intervening validation/reporting interval. Caller-owned input/output lifetime must be stated. Component release excludes freeing the caller's retained result before it can be verified.

An inclusive parent duration contains its children. Display the tree, but add only disjoint intervals at a chosen level. Independently measured medians cannot be summed to reconstruct another sample's total. Independently timed plan build plus independently timed replay is an explanatory model, not a measured full-encode duration.

Unattributed time includes uninstrumented work and the accounting overhead remaining between boundaries; show it explicitly. Substantial unexplained residue is a measurement diagnostic, not automatically a component correctness failure. Negative or overlapping accounting that contradicts the declared timing model is an evidence error. Never clip negative residue to zero to hide incorrect nesting.

If a future component overlaps phases or uses worker threads, report wall-clock intervals, overlap and worker CPU time separately. Their inclusive durations are not additive wall time. Cross-process timestamps are not subtracted without a declared clock relationship.

### Samples and repeat structure

For an ordinary performance case, use three independent worker processes, each with one excluded warmup and five recorded samples: three warmups and 15 samples in total per case/seed. A measured sample may contain a batch of elementary calls. First-use cases instead require fresh-process first observations without warmup; subsequent work is not reused as another first-use sample.

The candidate uses deterministic fixed batches: up to 64 independent instances for the small set and 512 for the large set, capped at 64 MiB of initial symbol bytes. Sixteen symbols are initialized per instance. These sizes are part of T and the actual call count is retained. There is no adaptive retry or hidden change in batch size. Define state preparation consistently and preserve all observed outputs or sufficient checked evidence. A result below usable clock resolution must be reported as unresolved at that resolution rather than an exact zero-time operation.

Ordinary and diagnostic workers use sorted, reverse and half-rotated case order within each category. C entries still execute independently in configured order. Keep worker/process grouping, repetition index and workload seed. Three seeds measure different inputs; repeats of one seed measure execution variability for that input. Fifteen samples are not 15 independent machines, nor a basis for precise tail-percentile or production-stability claims.

Use every recorded sample in its outcome group. Keep completed latency and budget-exhaustion latency separately and retain counts and worker grouping. A recovery row with an incomplete sample has no successful throughput or comparison speed percentage. The result index retains the input and replay digests, work summaries, and ordinary release and output endpoints. Do not filter slow observations. Individual samples are temporary execution evidence and are cleaned after the verified Markdown report is published. Record case order and host observations. When orchestrating a baseline/candidate study, use a prescribed balanced order; comparing already existing files cannot retroactively remove order drift.

### Resource scope

Resource diagnostics report allocation count, cumulative allocated bytes, peak live bytes and retained bytes over the stated lifecycle interval. Process RSS and physical memory/copy traffic are not measured. State which allocator or API surface was observed. Separate build/driver startup and input-fixture allocations from component work where evidence permits.

Shared-library allocations may not pass through a callback-side allocator. An incomplete observation domain must be labeled, not extrapolated to the whole product. Similarly, nominal copy bytes derived from operation calls are not measured memory-bus traffic. Product library allocations are explicitly outside the observed allocator scope. Diagnostic helper allocations remain included; retained caller-owned outputs must be considered when interpreting live bytes.

## Metric definitions and units

Raw elapsed nanoseconds and actual work counts are authoritative. For N completed blocks with identical K and B:

```text
source_payload_Gbit_per_s = N * K * B * 8 / elapsed_ns
repair_output_Gbit_per_s  = actual_repair_output_bytes * 8 / elapsed_ns
operations_per_s         = actual_public_calls * 1e9 / elapsed_ns
ns_per_operation         = elapsed_ns / actual_public_calls
plans_per_s              = completed_plan_builds * 1e9 / elapsed_ns
```

These equations use decimal Gbit/s because nanoseconds supply the factor of 1e9. GB/s and GiB/s are different units and must not share a label. Compute a batch rate from total work and its elapsed time, not an unweighted average of unequal per-item rates.

For example, K=512 and B=1016 represent 520,192 source bytes. Finishing one such block in 1,000,000 ns gives **4.161536 Gbit/s of source-payload-equivalent throughput**. This example is arithmetic, not a measured component result.

Encoding R repair symbols can be normalized to source bytes for application comparison, but also report the actual repair-output rate and R. A small R can produce a high source-equivalent rate without having emitted that many bytes. Decode source-equivalent throughput likewise does not describe network traffic. Report received source/repair counts and bytes separately.

Symbol lookup, move and alias operations primarily report ns/call and calls/s. Logical-byte rates are optional and must say what logical bytes mean. They do not prove that payload bytes were copied. Symbolic plan generation reports time, plans/s and plan/resource sizes, not payload Gbit/s.

Warnings do not change denominators. A boundary call that returns immediately still counts as the nominal call requested by the test, with its actual result visible. Internally executed arithmetic counts are diagnostic data and cannot silently replace the workload's public-call count.

## Expected report contents and reading guide

### Deliverables

The Markdown report for a selected run includes every C, groups results by test category, and separates execution wall time from benchmark time. Its index preserves validated aggregate results and exact per-case coverage.

The retained index includes completed codec validation observations, bounded regression evidence and observed work/digest summaries. Comparison exposes when the offered stream, consumed counts or recorded trace differ across C entries. The [artifact contracts](../specs/README.md) define temporary execution evidence and the retained Markdown result index. `result.json`, raw samples, logs and build cache are working files, not additional final reports. Framework checks detect missing observations, inconsistent work counts and invalid diagnostic accounting before cleanup.

### Read the report

Start with each component’s outcome and the first failed check or semantic warning group. Read codec throughput at K=512/5008 and widths 128/1016, separately measured encoding-plan timings (L02/L03), the largest instrumented lifecycle stages and the direct/execute ns per core operation. Coverage distinguishes skipped cases from checks left unfinished. The result index contains timings for the entire matrix, including other widths and seeds, continuity/reuse workloads, allocation metrics and all stage observations. Read it with `feval report REPORT.md --json`.

### How to interpret common patterns

| Observation | Supported interpretation | Additional evidence needed for a stronger claim |
| --- | --- | --- |
| XOR microbenchmarks improve but full encoding does not. | That primitive improved under its tested state; the complete workload did not show the same benefit. | Actual XOR share, instruction mix, setup and output costs. |
| Plan generation slows while plan reuse improves. | First-use and steady-state costs moved in different directions. | Application block count and measured amortized/continuity performance. |
| Replay improves with fewer operations. | Execution became faster for a different program. | Identical-program replay to isolate operator speed. |
| All-source decode is much faster than missing-source recovery. | The two cases can exercise substantially different work, including a legitimate fast path. | Solve activity, symbols consumed and stage evidence. |
| Output-ready latency improves but sustained throughput falls. | Work after output handoff or between blocks offsets the isolated latency gain. | Release intervals, block gaps and allocation/state behavior. |
| A warning-level operation is faster because it returns immediately. | The requested call is faster and has the reported semantic deviation. | Developer judgment about accepting that behavior; the framework does not veto it. |
| Diagnostic timing differs materially from ordinary timing. | Instrumentation or execution conditions affect the observation. | Same-work comparisons and explicit overhead evidence before attributing the diagnostic split. |

An optimization opportunity is a supported location to investigate, not a promised speedup. A measured serial fraction can support a qualified theoretical upper bound, but actual gains depend on interactions, changed work and cache/resource behavior. Avoid automatic causal claims from a single percentage change.

Ordinary workers have a 600-second deadline; fresh-process cases have 60 seconds. Three separate diagnostic workers observe the same ordinary cases. These bounds do not automatically expand after failure. The defined samples characterize these fixed inputs and this host, not production stability.
