//! T-owned correctness, individual-operation, chain and integrated workloads.
use crate::findings::{self, Contract, FailureKind as Kind};
use crate::{
    Probe, Stages, api,
    content::{Action, Case},
    digest, elapsed,
    operator::{Operator, Program},
    reference::{self, State},
    telemetry,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Instant};

macro_rules! require {
    (($id:expr,$requirement:expr),$kind:ident,$condition:expr,$($message:tt)+) => {
        if !$condition { findings::fail(Contract::new($id,$requirement),Kind::$kind,json!({}),None,format!($($message)+)); }
    };
    (($id:expr,$requirement:expr),$kind:ident,$condition:expr $(,)?) => {
        if !$condition { findings::fail(Contract::new($id,$requirement),Kind::$kind,json!({}),None,stringify!($condition).into()); }
    };
}
macro_rules! require_eq {
    (($id:expr,$requirement:expr),$kind:ident,$actual:expr,$expected:expr,$($message:tt)+) => {
        { let (actual,expected)=(&$actual,&$expected); if actual!=expected {findings::equal(Contract::new($id,$requirement),Kind::$kind,actual,expected,||format!($($message)+));} }
    };
    (($id:expr,$requirement:expr),$kind:ident,$actual:expr,$expected:expr $(,)?) => {
        { let (actual,expected)=(&$actual,&$expected); if actual!=expected {findings::equal(Contract::new($id,$requirement),Kind::$kind,actual,expected,||stringify!($actual == $expected).into());} }
    };
}

#[derive(Clone)]
pub struct Validation {
    pub witness: String,
    pub details: Value,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
}
pub struct Outcome {
    pub ns: u64,
    pub witness: String,
    pub details: Value,
    pub resources: Value,
    pub observation: Option<Value>,
    pub status: String,
}
pub struct Prepared {
    source: Vec<u8>,
    additional_sources: Vec<Vec<u8>>,
    source_digest: String,
    predecessor: Option<Vec<u8>>,
    rebuild_outputs: RefCell<Option<Vec<Vec<Vec<u8>>>>>,
    packets: Vec<(usize, Vec<u8>)>,
    packet_digest: Option<String>,
    program: Option<Program>,
    expected: State,
    initial: State,
    plan: Option<RefCell<api::Plan>>,
    replay: Option<api::Replay>,
}
fn scheme(c: &Case) -> &str {
    c.scheme.as_deref().unwrap_or(crate::config::SCHEME)
}
fn source(c: &Case) -> Vec<u8> {
    reference::bytes(c.k * if c.width == 0 { 31 } else { c.width }, c.seed)
}
fn stream_digest(stream: &[(usize, Vec<u8>)]) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (id, symbol) in stream {
        hash.update((*id as u64).to_le_bytes());
        hash.update((symbol.len() as u64).to_le_bytes());
        hash.update(symbol);
    }
    format!("{:x}", hash.finalize())
}

fn state_witness(state: &State) -> String {
    digest(&serde_json::to_vec(&reference::compact(state)).unwrap())
}
/// Shared omission and arrival order for the decoding workloads.
fn offered_ids(c: &Case, base: usize) -> Vec<usize> {
    let mut omitted: Vec<_> = (0..c.k).collect();
    reference::shuffle(&mut omitted, c.seed ^ 0x10_55);
    omitted.truncate(c.missing);
    let omitted: std::collections::BTreeSet<_> = omitted.into_iter().collect();
    let mut ids: Vec<_> = (0..c.k).filter(|id| !omitted.contains(id)).collect();
    ids.extend(base..base + if c.missing == 0 { 0 } else { c.budget() });
    ids
}

fn order_packets(c: &Case, stream: &mut [(usize, Vec<u8>)]) {
    if c.order == "mixed" {
        reference::shuffle(stream, c.seed ^ 0x50_17);
    }
}

fn packets(c: &Case, bytes: &[u8]) -> Vec<(usize, Vec<u8>)> {
    // Systematic source bytes are known independently of the selected encoder.
    let mut encoder =
        (c.missing > 0).then(|| api::Encoder::new(scheme(c), c.k, c.width, bytes, None));
    let base = encoder.as_ref().map_or(c.k, api::Encoder::repair_base);
    require!(
        (
            "codec.repair-domain",
            "Repair IDs must begin after the source symbol domain."
        ),
        AssertionFailed,
        base >= c.k,
        "{}: invalid repair base {base}",
        c.id
    );
    let mut stream: Vec<_> = offered_ids(c, base)
        .into_iter()
        .map(|id| {
            let data = if id < c.k {
                bytes[id * c.width..(id + 1) * c.width].to_vec()
            } else {
                encoder.as_mut().unwrap().encode(id)
            };
            require_eq!(
                (
                    "codec.symbol-width",
                    "Every supplied symbol must have the configured byte width."
                ),
                LengthMismatch,
                data.len(),
                c.width,
                "{}: offered symbol {id} width",
                c.id
            );
            (id, data)
        })
        .collect();
    order_packets(c, &mut stream);
    stream
}
pub fn prepare(c: &Case) -> Prepared {
    let source = if c.actions.is_empty() {
        source(c)
    } else {
        Vec::new()
    };
    let mut initial = if c.actions.is_empty() {
        State::new()
    } else {
        reference::initial(c.width, c.seed)
    };
    if c.workload == "unshared-first-write" {
        // Give the private destination the same bytes as the shared destination.
        initial.insert(8, initial[&0].clone());
    }
    let mut expected = initial.clone();
    for action in &c.actions {
        reference::apply(&mut expected, action, c.width, c.seed);
    }
    let program =
        (!c.actions.is_empty()).then(|| Program::new(&c.actions, c.direct, c.width, c.seed));
    let packets = if matches!(c.workload.as_str(), "decode" | "decode-replay") {
        packets(c, &source)
    } else {
        vec![]
    };
    let predecessor =
        (c.workload == "reuse-encode").then(|| reference::bytes(c.k * c.width, c.seed ^ 0x22_12));
    let plan = if let Some(bytes) = &predecessor {
        let mut plan = api::Plan::new(scheme(c), c.k);
        plan.load(c.width, bytes, None, &mut Stages::disabled());
        Some(RefCell::new(plan))
    } else {
        None
    };
    let replay = if c.workload.ends_with("-replay") {
        Some(api::Replay::capture(
            scheme(c),
            c.k,
            c.width,
            &source,
            &packets,
            c.workload == "decode-replay",
        ))
    } else {
        None
    };
    let packet_digest = (!packets.is_empty()).then(|| stream_digest(&packets));
    let additional_sources: Vec<_> = (1..c.blocks)
        .map(|block| reference::bytes(c.k * c.width, c.seed + block as u64))
        .collect();
    use sha2::{Digest, Sha256};
    let mut input = Sha256::new();
    input.update(&source);
    for block in &additional_sources {
        input.update(block);
    }
    let source_digest = format!("{:x}", input.finalize());
    Prepared {
        initial,
        source,
        additional_sources,
        source_digest,
        predecessor,
        rebuild_outputs: RefCell::new(None),
        packets,
        packet_digest,
        program,
        expected,
        plan,
        replay,
    }
}
fn checked_state(op: &Operator, expected: &State, context: &str) {
    findings::states(
        Contract::new(
            "operator.result",
            "The prepared operator call must produce the required live symbol bytes.",
        ),
        expected,
        &op.snapshot(expected),
        ("Expected", "Observed"),
        Kind::ValueMismatch,
        context,
    );
}
fn check_bytes(
    contract_id: &str,
    expected: &[u8],
    actual: &[u8],
    context: impl FnOnce() -> String,
) {
    findings::bytes(
        Contract::new(
            contract_id,
            "The output bytes must match the registered reference.",
        ),
        expected,
        actual,
        context,
    );
}

fn basis_encoder(c: &Case) -> api::Encoder {
    require!(
        (
            "fixture.basis-width",
            "A coefficient basis fixture must fit all K basis bytes."
        ),
        AssertionFailed,
        c.width >= c.k
    );
    let mut basis = vec![0; c.k * c.width];
    for i in 0..c.k {
        basis[i * c.width + i] = 1;
    }
    api::Encoder::new(scheme(c), c.k, c.width, &basis, None)
}

fn coefficients(generator: &mut api::Encoder, c: &Case, id: usize) -> Vec<u8> {
    let mut row = generator.encode(id);
    require_eq!(
        (
            "codec.symbol-width",
            "Every coefficient symbol must have the configured byte width."
        ),
        LengthMismatch,
        row.len(),
        c.width,
        "{}: basis symbol {id} width",
        c.id
    );
    require!(
        (
            "codec.basis-inactive",
            "Coefficient symbols must have zero bytes outside their active basis."
        ),
        ValueMismatch,
        row[c.k..].iter().all(|&b| b == 0),
        "{}: basis symbol {id} has nonzero inactive lanes",
        c.id
    );
    row.truncate(c.k);
    row
}

fn linear_symbol(row: &[u8], source: &[u8], width: usize) -> Vec<u8> {
    let mut output = vec![0; width];
    for (&factor, symbol) in row.iter().zip(source.chunks_exact(width)) {
        for (dst, &src) in output.iter_mut().zip(symbol) {
            *dst ^= reference::multiply(factor, src);
        }
    }
    output
}

fn linear_reference(c: &Case) -> Value {
    let bytes = source(c);
    let mut generator = basis_encoder(c);
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, &bytes, None);
    let base = encoder.repair_base();
    require_eq!(
        (
            "codec.repair-domain",
            "Equivalent encoders must expose the same repair domain."
        ),
        ValueMismatch,
        base,
        generator.repair_base(),
        "{}: basis repair domain",
        c.id
    );
    let mut rows = Vec::new();
    let mut rank = reference::Rank::new(c.k);
    let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, None);
    require!(
        (
            "codec.initial-incomplete",
            "A decoder must be incomplete before receiving input."
        ),
        PrematureCompletion,
        !decoder.decoded(),
        "{}: decoded before input",
        c.id
    );
    let mut completion = None;
    let mut repair_rank = None;
    let offered = (0..c.k)
        .filter(|i| *i % 3 != 0)
        .chain(base..base + c.budget());
    // The withheld sources are a fixed positive control after the repair-only portion.
    let repair_end = c.k - c.k.div_ceil(3) + c.budget();
    for (index, id) in offered.chain((0..c.k).step_by(3)).enumerate() {
        if index == repair_end {
            repair_rank = Some(rank.value());
            require!(
                (
                    "codec.full-rank-completion",
                    "Rank-K input must complete recovery within the fixed stream."
                ),
                IncompleteRecovery,
                rank.value() < c.k,
                "{}: rank K after repair budget without completion",
                c.id
            );
        }
        let mut row = coefficients(&mut generator, c, id);
        let actual = encoder.encode(id);
        check_bytes(
            "codec.linear-symbol",
            &linear_symbol(&row, &bytes, c.width),
            &actual,
            || format!("{}: V09 linear encoding, symbol {id}", c.id),
        );
        rank.insert(row.clone());
        row.extend_from_slice(&actual);
        rows.push(row);
        decoder.add(id, &actual);
        if decoder.decoded() {
            require_eq!(
                (
                    "codec.completion-rank",
                    "Reported completion requires rank K."
                ),
                PrematureCompletion,
                rank.value(),
                c.k,
                "{}: V09 false completion at input {}, symbol {id}",
                c.id,
                index + 1
            );
            completion = Some(index + 1);
            break;
        }
    }
    let solved = reference::solve(rows, c.k, c.width).unwrap_or_else(|| {
        findings::fail(
            Contract::new(
                "reference.linear-recovery",
                "The independent positive-control input must have rank K.",
            ),
            Kind::IncompleteRecovery,
            json!({}),
            None,
            format!("{}: V09 positive control lacks rank K", c.id),
        )
    });
    check_bytes("reference.linear-recovery", &bytes, &solved, || {
        format!("{}: V09 independent recovery", c.id)
    });
    let count = completion.unwrap_or_else(|| {
        findings::fail(
            Contract::new(
                "codec.full-rank-completion",
                "Rank-K input must complete recovery within the fixed stream.",
            ),
            Kind::IncompleteRecovery,
            json!({}),
            None,
            format!("{}: V09 full-rank input did not complete", c.id),
        )
    });
    check_bytes("codec.recovered-payload", &bytes, &decoder.result(), || {
        format!("{}: V09 decoder recovery at input {count}", c.id)
    });
    json!({"coefficient_source":"basis payloads at the same symbol width through the selected encoder", "independent_solver":"scalar Gauss-Jordan over GF(256)", "rank":rank.value(), "completion_input":count, "rank_at_exhausted_repair_budget":repair_rank})
}

fn scheme_contract(c: &Case, bytes: &[u8]) -> Value {
    let (padded, repair_base) = reference::geometry(scheme(c), c.k);
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, None);
    require_eq!(
        (
            "codec.scheme.repair-domain",
            "The first repair ID must match the selected scheme."
        ),
        ValueMismatch,
        encoder.repair_base(),
        repair_base,
        "{}: V07 first internal repair ID",
        c.id
    );
    let geometry = encoder.diagnostics();
    if let Some(actual) = geometry["block_k"].as_u64() {
        require_eq!(
            (
                "codec.scheme.padding",
                "Padded K must match the selected scheme geometry."
            ),
            ValueMismatch,
            actual,
            padded as u64,
            "{}: V07 padded geometry",
            c.id
        );
    }
    let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, None);
    for (id, expected) in bytes.chunks_exact(c.width).enumerate() {
        let actual = encoder.encode(id);
        check_bytes("codec.systematic-symbol", expected, &actual, || {
            format!("{}: V07 systematic symbol {id}", c.id)
        });
        decoder.add(id, &actual);
    }
    require!(
        (
            "codec.all-source-completion",
            "A complete source set must make the application output available."
        ),
        IncompleteRecovery,
        decoder.decoded(),
        "{}: V07 application output unavailable",
        c.id
    );
    let output = decoder.result();
    check_bytes("codec.recovered-payload", bytes, &output, || {
        format!("{}: V07 application output boundary", c.id)
    });
    json!({"source_symbols":c.k,"expected_block_k":padded,"observed_block_k":geometry["block_k"],"padding_symbols":padded-c.k,"padding_provider":geometry["padding_provider"],"first_internal_repair_id":repair_base,"output_bytes":output.len()})
}

fn plan_contract(c: &Case) -> Value {
    let mut plan = api::Plan::new(scheme(c), c.k);
    for round in 0..3 {
        let fresh = reference::bytes(c.k * c.width, c.seed + round);
        plan.load(c.width, &fresh, None, &mut Stages::disabled());
        let mut rebuilt = api::Encoder::new(scheme(c), c.k, c.width, &fresh, None);
        let base = plan.repair_base();
        require_eq!(
            (
                "codec.plan.repair-domain",
                "Reused and rebuilt plans must expose the same repair domain."
            ),
            ValueMismatch,
            base,
            rebuilt.repair_base(),
            "{}: V08 load {round} repair domain",
            c.id
        );
        for id in (0..c.k).chain(base..base + c.repairs) {
            let expected = rebuilt.encode(id);
            let actual = plan.encode(id);
            require_eq!(
                (
                    "codec.symbol-width",
                    "Every reused-plan symbol must have the configured byte width."
                ),
                LengthMismatch,
                actual.len(),
                c.width,
                "{}: V08 load {round}, symbol {id} width",
                c.id
            );
            check_bytes("codec.plan.reuse-output", &expected, &actual, || {
                format!("{}: V08 load {round}, symbol {id}", c.id)
            });
        }
    }
    json!({"different_payloads":3,"same_geometry":true,"reuse":plan.diagnostics(),"comparison":"reuse versus rebuild"})
}

fn receive_contract(c: &Case, bytes: &[u8]) -> Value {
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, None);
    let mut basis = basis_encoder(c);
    let base = encoder.repair_base();
    require_eq!(
        (
            "codec.repair-domain",
            "Basis and payload encoders must expose the same repair domain."
        ),
        ValueMismatch,
        base,
        basis.repair_base(),
        "{}: V10 basis repair domain",
        c.id
    );
    let repair_end = base + c.budget();
    let expected_repairs: BTreeMap<_, _> = (base..repair_end)
        .map(|id| {
            let row = coefficients(&mut basis, c, id);
            let payload = encoder.encode(id);
            check_bytes(
                "codec.linear-symbol",
                &linear_symbol(&row, bytes, c.width),
                &payload,
                || format!("{}: V10 repair fixture symbol {id}", c.id),
            );
            (id, (row, payload))
        })
        .collect();
    let source_ids: Vec<_> = (0..c.k).collect();
    let repair_ids: Vec<_> = (base..repair_end).collect();
    let missing: Vec<_> = (c.repairs..c.k).chain(base..repair_end).collect();
    let mut mixed = missing.clone();
    reference::shuffle(&mut mixed, c.seed);
    // Give both mixed cases the same early source and repair. Only repetition changes.
    for (index, id) in [base, c.repairs].into_iter().enumerate() {
        let pos = mixed.iter().position(|&x| x == id).unwrap();
        mixed.swap(index, pos);
    }
    let mut duplicated = mixed.clone();
    duplicated.splice(0..2, [base, base, c.repairs, c.repairs]);
    let doubled_sources = source_ids.iter().flat_map(|&id| [id, id]).collect();
    let early = (base..base + c.repairs).chain(0..c.k).collect();
    let scenarios = [
        ("all-sources", source_ids),
        ("duplicate-sources", doubled_sources),
        ("missing-source-first", missing),
        ("missing-mixed", mixed),
        ("missing-with-duplicates", duplicated),
        ("repairs-only", repair_ids),
        ("repairs-first", early),
    ];
    let mut results = Vec::new();
    for (name, stream) in scenarios {
        let mut rank = reference::Rank::new(c.k);
        let mut seen = std::collections::BTreeSet::new();
        let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, None);
        require!(
            (
                "codec.initial-incomplete",
                "A decoder must be incomplete before receiving input."
            ),
            PrematureCompletion,
            !decoder.decoded(),
            "{}: V10 {name} decoded before input",
            c.id
        );
        let mut consumed = 0;
        let mut duplicates = 0;
        let mut repairs = 0;
        let mut complete = false;
        for &id in &stream {
            let (row, payload) = if id < c.k {
                let mut row = vec![0; c.k];
                row[id] = 1;
                (row, &bytes[id * c.width..(id + 1) * c.width])
            } else {
                let (row, payload) = &expected_repairs[&id];
                (row.clone(), payload.as_slice())
            };
            consumed += 1;
            repairs += usize::from(id >= base);
            if seen.insert(id) {
                rank.insert(row);
            } else {
                duplicates += 1;
            }
            findings::scope(
                Contract::new(
                    "codec.receive-input",
                    "The decoder must accept valid offered symbols, including repeated inputs.",
                ),
                json!({"scenario":name,"input_count":consumed,"input_symbol":id}),
                || decoder.add(id, payload),
            );
            if decoder.decoded() {
                require_eq!(
                    (
                        "codec.completion-rank",
                        "Reported completion requires rank K."
                    ),
                    PrematureCompletion,
                    rank.value(),
                    c.k,
                    "{}: V10 {name} false completion at input {consumed}, symbol {id}",
                    c.id
                );
                findings::scope(
                    Contract::new(
                        "codec.recovered-payload",
                        "Recovered application bytes must equal the original source bytes.",
                    ),
                    json!({"scenario":name,"input_count":consumed,"input_symbol":id}),
                    || {
                        check_bytes("codec.recovered-payload", bytes, &decoder.result(), || {
                            format!("{}: V10 {name} input {consumed}, symbol {id}", c.id)
                        });
                    },
                );
                complete = true;
                break;
            }
        }
        require!(
            (
                "codec.full-rank-completion",
                "Rank-K input must complete recovery within the fixed stream."
            ),
            IncompleteRecovery,
            complete || rank.value() < c.k,
            "{}: V10 {name} exhausted {consumed} inputs at rank K, last symbol {:?}",
            c.id,
            stream.last()
        );
        results.push(json!({"scenario":name,"status":if complete{"completed"}else{"rank_deficient"},"offered":stream.len(),"consumed":consumed,"repair_inputs":repairs,"duplicates":duplicates,"rank":rank.value()}));
    }
    json!({"scenarios":results})
}

fn source_roundtrip(c: &Case, bytes: &[u8], probe: Option<Arc<Probe>>) -> Vec<u8> {
    let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, probe);
    for (id, data) in bytes.chunks_exact(c.width).enumerate() {
        decoder.add(id, data);
    }
    require!(
        (
            "codec.all-source-completion",
            "A complete source set must decode."
        ),
        IncompleteRecovery,
        decoder.decoded(),
        "{}: all-source control",
        c.id
    );
    decoder.result()
}

fn participation_contract(c: &Case, bytes: &[u8]) -> Value {
    let encode_probe = Arc::new(Probe::default());
    let decode_probe = Arc::new(Probe::default());
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, Some(encode_probe.clone()));
    let repair = encoder.encode(encoder.repair_base());
    require_eq!(
        (
            "codec.symbol-width",
            "Every selected-operator repair must have the configured byte width."
        ),
        LengthMismatch,
        repair.len(),
        c.width,
        "{}: V01 repair width",
        c.id
    );
    let output = source_roundtrip(c, bytes, Some(decode_probe.clone()));
    check_bytes("codec.recovered-payload", bytes, &output, || {
        format!("{}: V01 selected decode operator", c.id)
    });
    let encode_evidence = encode_probe.snapshot();
    let decode_evidence = decode_probe.snapshot();
    for (label, evidence) in [("encode", &encode_evidence), ("decode", &decode_evidence)] {
        require!(
            (
                "codec.operator-participation",
                "The selected encoder and decoder operator must receive the expected read and write calls."
            ),
            MissingParticipation,
            evidence["reads"].as_u64().unwrap() > 0 && evidence["writes"].as_u64().unwrap() > 0,
            "{}: V01 selected {label} operator did not participate",
            c.id
        );
    }
    json!({"encoder":encode_evidence,"decoder":decode_evidence})
}

fn lifecycle_contract(c: &Case, bytes: &[u8]) -> Value {
    let encode_probe = Arc::new(Probe::default());
    let decode_probe = Arc::new(Probe::default());
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, Some(encode_probe.clone()));
    let repair = encoder.encode(encoder.repair_base());
    drop(encoder);
    require_eq!(
        (
            "codec.encoder-ownership",
            "The encoder must release its selected operator exactly once."
        ),
        OwnershipMismatch,
        encode_probe.snapshot()["drops"],
        1,
        "{}: V11 encoder ownership",
        c.id
    );
    require_eq!(
        (
            "codec.output-ownership",
            "Caller-owned output must retain the configured width after release."
        ),
        LengthMismatch,
        repair.len(),
        c.width,
        "{}: V11 handoff width",
        c.id
    );
    let mut abandoned = api::Decoder::new(scheme(c), c.k, c.width, Some(decode_probe.clone()));
    abandoned.add(0, &bytes[..c.width]);
    drop(abandoned);
    require_eq!(
        (
            "codec.decoder-ownership",
            "An abandoned decoder must release its selected operator exactly once."
        ),
        OwnershipMismatch,
        decode_probe.snapshot()["drops"],
        1,
        "{}: V11 partially loaded decoder ownership",
        c.id
    );
    let output = source_roundtrip(c, bytes, Some(decode_probe.clone()));
    require_eq!(
        (
            "codec.decoder-ownership",
            "A completed decoder must release its selected operator exactly once."
        ),
        OwnershipMismatch,
        decode_probe.snapshot()["drops"],
        2,
        "{}: V11 completed decoder ownership",
        c.id
    );
    check_bytes("codec.output-ownership", bytes, &output, || {
        format!("{}: V11 caller-owned output after component release", c.id)
    });
    json!({"encoder_drops":1,"abandoned_decoder_drops":1,"completed_decoder_drops":1,"output_verified_after_release":true,"error_contract":api::error_contract(scheme(c),c.k,c.width,bytes)})
}

fn codec_contract(c: &Case) -> Value {
    let bytes = source(c);
    match c.workload.as_str() {
        "scheme-padding" => scheme_contract(c, &bytes),
        "plan-reuse-contract" => plan_contract(c),
        "linear-reference" => linear_reference(c),
        "receive-contract" => receive_contract(c, &bytes),
        "participation" => participation_contract(c, &bytes),
        "lifecycle-contract" => lifecycle_contract(c, &bytes),
        _ => unreachable!("unknown codec validation: {}", c.workload),
    }
}
pub fn validate(c: &Case) -> Validation {
    if c.category == "correctness" && !c.warning && c.boundary.is_none() {
        let details = if c.scheme.is_some() {
            codec_contract(c)
        } else {
            crate::contracts::validate::<Operator>(c)
        };
        return Validation {
            witness: digest(c.id.as_bytes()),
            details,
            expected: None,
            actual: None,
        };
    }
    let mut validation_case = c.clone();
    validation_case.instances = 1;
    let prepared = prepare(&validation_case);
    let outcome = run(&validation_case, &prepared, false);
    let (expected, actual) = if c.warning {
        (
            Some(reference::compact(&prepared.expected)),
            outcome
                .observation
                .as_ref()
                .map(|v| v["outcomes"][0]["actual"].clone()),
        )
    } else {
        (None, None)
    };
    Validation {
        witness: outcome.witness,
        details: outcome.details,
        expected,
        actual,
    }
}
fn primitive(c: &Case, p: &Prepared) -> Outcome {
    let instances = if c.category == "correctness" {
        1
    } else {
        c.instances
    };
    use fountain_engine::traits::DataOperator;
    let mut operators: Vec<_> = (0..instances)
        .map(|_| {
            let mut op = Operator::new(c.width);
            for (id, v) in &p.initial {
                op.0.insert_vector(v, *id);
            }
            op.prepare_shared_write(&c.actions);
            op
        })
        .collect();
    let hook = c.boundary.is_some().then(std::panic::take_hook);
    if c.boundary.is_some() {
        std::panic::set_hook(Box::new(|_| {}));
    }
    let mut panics = Vec::with_capacity(instances);
    let read_id = match c.actions.as_slice() {
        [Action::Read { id }] => Some(*id),
        _ => None,
    };
    let mut reads = Vec::with_capacity(if read_id.is_some() { instances } else { 0 });
    let before = telemetry::start();
    let start = Instant::now();
    for op in &mut operators {
        if let Some(id) = read_id {
            reads.push(std::hint::black_box(op.0.get_vector(id)));
        } else if c.boundary.is_some() {
            panics.push(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    op.run(p.program.as_ref().unwrap())
                }))
                .is_err(),
            );
        } else {
            std::hint::black_box(op).run(std::hint::black_box(p.program.as_ref().unwrap()));
        }
    }
    let ns = elapsed(start);
    let resources = telemetry::finish(before);
    if let Some(hook) = hook {
        std::panic::set_hook(hook);
    }
    if let Some(id) = read_id {
        for bytes in reads {
            check_bytes("operator.read", &p.expected[&id], bytes, || {
                format!("{}: timed read of symbol {id}", c.id)
            });
        }
    }
    let mut outcomes: BTreeMap<String, (u64, Value)> = BTreeMap::new();
    for (index, op) in operators.iter().enumerate() {
        if c.boundary.is_none() && !c.warning {
            if read_id.is_none() {
                checked_state(op, &p.expected, &c.id);
            }
            continue;
        }
        let value = if c.boundary.is_some() {
            crate::contracts::boundary_observation(op, c, &p.initial, &p.expected, panics[index])
        } else {
            unreachable!("non-boundary operations were checked above")
        };
        let key = digest(&serde_json::to_vec(&value).unwrap());
        outcomes.entry(key).or_insert((0, value)).0 += 1;
    }
    let observation = json!({"calls":instances,"outcomes":outcomes.into_values().map(|(count,actual)|json!({"count":count,"finding":findings::boundary_violation(c,&reference::compact(&p.expected),&actual),"actual":actual})).collect::<Vec<_>>()});
    let witness = if c.warning {
        digest(&serde_json::to_vec(&observation).unwrap())
    } else {
        state_witness(&p.expected)
    };
    Outcome {
        ns,
        witness,
        resources,
        observation: c.warning.then_some(observation),
        status: "completed".into(),
        details: json!({"public_calls":instances,"initial_symbol_bytes":instances*16*c.width,"symbol_bytes":c.width,"field_polynomial":0x11d,"dispatch":if c.direct{"direct"}else{"execute"},"operations":c.actions,"boundary_contract":c.boundary,"logical_byte_rate":"not physical memory bandwidth"}),
    }
}
pub fn run(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    if !c.actions.is_empty() {
        return primitive(c, p);
    }
    if c.workload.ends_with("-replay") {
        let (ns, mut details, resources) = p.replay.as_ref().unwrap().execute();
        details["input_digest"] = json!(
            p.packet_digest
                .as_ref()
                .cloned()
                .unwrap_or_else(|| digest(&p.source))
        );
        return Outcome {
            ns,
            witness: digest(&p.source),
            details,
            resources,
            observation: None,
            status: "completed".into(),
        };
    }
    if matches!(c.workload.as_str(), "plan-build" | "plan-export") {
        let mut existing = (c.workload == "plan-export").then(|| api::Plan::new(scheme(c), c.k));
        let before = telemetry::start();
        let start = Instant::now();
        let mut created = if c.workload == "plan-build" {
            Some(api::Plan::new(scheme(c), c.k))
        } else {
            None
        };
        let exported = existing.as_ref().map(|p| p.export());
        std::hint::black_box(&exported);
        let ns = elapsed(start);
        let resources = telemetry::finish(before);
        let plan = created.as_mut().or(existing.as_mut()).unwrap();
        let mut details = plan.diagnostics();
        if let Some(operations) = exported {
            require_eq!(
                (
                    "plan.export-count",
                    "Exported operation count must match the retained plan."
                ),
                ValueMismatch,
                operations.len() as u64,
                details["operations"].as_u64().unwrap()
            );
            require_eq!(
                (
                    "plan.export-bytes",
                    "Exported operations must match the retained plan digest."
                ),
                ValueMismatch,
                digest(format!("{operations:?}").as_bytes()),
                details["program_digest"]
            );
            details["timed_work"] = json!("clone the recorded operation list");
        } else {
            details["timed_work"] = json!("scheme setup through retained symbolic operations");
        }
        let bytes = &p.source;
        plan.load(31, bytes, None, &mut Stages::disabled());
        let mut encoder = api::Encoder::new(scheme(c), c.k, 31, bytes, None);
        let base = plan.repair_base();
        require_eq!(
            (
                "plan.repair-domain",
                "An applied plan must match the encoder repair domain."
            ),
            ValueMismatch,
            base,
            encoder.repair_base(),
            "{}: planning repair domain",
            c.id
        );
        for id in base..base + c.repairs {
            let expected = encoder.encode(id);
            check_bytes("plan.applied-output", &expected, &plan.encode(id), || {
                format!("{}: applied plan repair {id}", c.id)
            });
        }
        return Outcome {
            ns,
            witness: digest(bytes),
            details,
            resources,
            observation: None,
            status: "completed".into(),
        };
    }
    if c.workload == "decode" {
        return decode(c, p, diagnostic);
    }
    if c.workload == "round-trip" {
        return round_trip(c, p, diagnostic);
    }
    encode(c, p, diagnostic)
}
/// These relationships are expectations of the registered fixture and its timings.
/// Unavailable subdivisions are described without inventing an independent interval.
fn link_coverage(c: &Case, stages: &Value, diagnostic: bool) -> Value {
    if !diagnostic || !c.family.split(',').any(|family| family.starts_with('L')) {
        return Value::Null;
    }
    let product = crate::config::FAMILY == "product-abi";
    let rows: &[(&str, &str, bool)] = if c.workload == "decode" {
        &[
            ("L01", "decoder-context-setup", false),
            ("L07", "ingestion-and-solve", true),
            ("L08", "ingestion-and-solve", true),
            ("L09", "output-materialization", false),
            ("L10", "component-release", false),
        ]
    } else if product {
        &[
            (
                "L01",
                "product-construction-loading-and-plan-application",
                true,
            ),
            (
                "L04",
                "product-construction-loading-and-plan-application",
                true,
            ),
            (
                "L05",
                "product-construction-loading-and-plan-application",
                true,
            ),
            ("L06", "product-generation-and-output", true),
            ("L09", "product-generation-and-output", true),
            ("L10", "component-release", false),
        ]
    } else {
        &[
            ("L01", "scheme-and-storage-setup", false),
            ("L04", "source-loading-and-padding", false),
            ("L05", "encoder-context-and-plan-application", true),
            ("L06", "symbol-generation", false),
            ("L09", "output-materialization", false),
            ("L10", "component-release", false),
        ]
    };
    let map: BTreeMap<_, _> = rows.iter().map(|(family, stage, combined)| {
        require!(("measurement.stage-coverage","Every promised exclusive stage must be observed."), AssertionFailed, stages["exclusive_ns"][stage].as_u64().is_some(),
            "{}: required stage {stage} missing for {family}", c.id);
        (*family, json!({"interval":stage,"scope":if *combined { "combined parent" } else { "exclusive interval" }}))
    }).collect();
    json!(map)
}

fn decode(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    let probe = diagnostic.then(|| Arc::new(Probe::default()));
    let before = telemetry::start();
    let start = Instant::now();
    let mut stages = Stages::new(start, diagnostic);
    let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, probe.clone());
    stages.mark("decoder-context-setup");
    let mut consumed = 0;
    let mut received_bytes = 0;
    for (id, v) in &p.packets {
        decoder.add(*id, v);
        consumed += 1;
        received_bytes += v.len();
        if decoder.decoded() {
            break;
        }
    }
    stages.mark("ingestion-and-solve");
    let complete = decoder.decoded();
    require!(
        (
            "codec.all-source-completion",
            "A complete source set must decode."
        ),
        IncompleteRecovery,
        complete || c.missing > 0,
        "{}: W04 complete source set did not decode",
        c.id
    );
    let output = if complete { decoder.result() } else { vec![] };
    stages.mark("output-materialization");
    let ready = elapsed(start);
    let internal = if diagnostic {
        decoder.diagnostics()
    } else {
        Value::Null
    };
    if diagnostic {
        stages.mark("diagnostic-query-overhead");
    }
    let release_started = elapsed(start);
    drop(decoder);
    let released = elapsed(start);
    stages.mark("component-release");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    if complete {
        check_bytes("codec.recovered-payload", &p.source, &output, || {
            format!("{}: output after {consumed} inputs", c.id)
        });
    }
    let stages = stages.value(lifecycle);
    let links = link_coverage(c, &stages, diagnostic);
    Outcome {
        ns: ready,
        witness: digest(&p.source),
        resources,
        observation: None,
        status: if complete {
            "completed"
        } else {
            "incomplete_recovery"
        }
        .into(),
        details: json!({"received_symbols":consumed,"received_bytes":received_bytes,"received_sources":p.packets[..consumed].iter().filter(|(id,_)| *id<c.k).count(),"received_repairs":p.packets[..consumed].iter().filter(|(id,_)| *id>=c.k).count(),"offered_symbols":p.packets.len(),"repair_budget":if c.missing==0{0}else{c.budget()},"input_digest":p.packet_digest,"output_ready_ns":complete.then_some(ready),"attempt_end_ns":ready,"component_released_ns":released,"release_ns":released-release_started,"lifecycle_ns":lifecycle,"stages":stages,"link_coverage":links,"internal":internal,"probe":probe.map(|p|p.snapshot()),"recovery":if complete{"byte-verified"}else{"fixed budget exhausted; rank not established; no successful throughput"}}),
    }
}
fn encode(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    let probe = diagnostic.then(|| Arc::new(Probe::default()));
    let sources: Vec<_> = std::iter::once(&p.source)
        .chain(&p.additional_sources)
        .collect();
    let mut outputs = Vec::with_capacity(c.blocks);
    let mut block_rows = Vec::with_capacity(c.blocks);
    let before = telemetry::start();
    let start = Instant::now();
    let mut stages = Stages::new(start, diagnostic);
    let mut reused = None;
    let mut ready = 0;
    let mut released = None;
    let mut release_ns = 0;
    let mut plan_constructed = None;
    let mut implementation = Value::Null;
    if c.workload == "continuous-reuse" {
        reused = Some(api::Plan::new(scheme(c), c.k));
        plan_constructed = Some(elapsed(start));
        stages.mark("plan-or-reuse-context-creation");
    }
    for bytes in &sources {
        let block_start = elapsed(start);
        let mut block_output = Vec::new();
        if c.workload == "reuse-encode" {
            let mut plan = p.plan.as_ref().unwrap().borrow_mut();
            plan.load(c.width, bytes, probe.clone(), &mut stages);
            for id in plan.repair_base()..plan.repair_base() + c.repairs {
                block_output.push(plan.encode(id));
            }
            stages.mark("symbol-generation-and-output");
            if diagnostic {
                implementation = plan.diagnostics();
                stages.mark("diagnostic-query-overhead");
            }
        } else if let Some(plan) = reused.as_mut() {
            plan.load(c.width, bytes, probe.clone(), &mut stages);
            for id in plan.repair_base()..plan.repair_base() + c.repairs {
                block_output.push(plan.encode(id));
            }
            stages.mark("symbol-generation-and-output");
        } else {
            let mut encoder = api::Encoder::new_staged(
                scheme(c),
                c.k,
                c.width,
                bytes,
                probe.clone(),
                &mut stages,
            );
            let ids: Vec<_> = if c.workload == "encode-sources" {
                (0..c.k).collect()
            } else {
                (encoder.repair_base()..encoder.repair_base() + c.repairs).collect()
            };
            block_output = encoder.outputs(&ids, &mut stages);
            ready = elapsed(start);
            if diagnostic {
                implementation = encoder.diagnostics();
                stages.mark("diagnostic-query-overhead");
            }
            let drop_start = elapsed(start);
            drop(encoder);
            let drop_end = elapsed(start);
            released = Some(drop_end);
            release_ns += drop_end - drop_start;
            stages.mark("component-release");
        }
        if c.workload == "reuse-encode" || reused.is_some() {
            ready = elapsed(start);
        }
        outputs.push(block_output);
        block_rows.push((block_start, ready, released));
    }
    if let Some(plan) = reused {
        let drop_start = elapsed(start);
        drop(plan);
        let drop_end = elapsed(start);
        released = Some(drop_end);
        release_ns += drop_end - drop_start;
        stages.mark("component-release");
    }
    stages.mark("workflow-complete");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    let mut repair_reference = p.rebuild_outputs.borrow_mut();
    let source_outputs = c.workload == "encode-sources";
    if !source_outputs && repair_reference.is_none() {
        *repair_reference = Some(
            sources
                .iter()
                .map(|bytes| {
                    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, None);
                    let base = encoder.repair_base();
                    (base..base + c.repairs)
                        .map(|id| encoder.encode(id))
                        .collect()
                })
                .collect(),
        );
    }
    let expected_count = if source_outputs { c.k } else { c.repairs };
    let mut generated = 0;
    for (block, result) in outputs.iter().enumerate() {
        require_eq!(
            (
                "codec.output-count",
                "The encoder must produce the requested number of output symbols."
            ),
            LengthMismatch,
            result.len(),
            expected_count,
            "{}: output count for block {block}",
            c.id
        );
        for (id, data) in result.iter().enumerate() {
            require_eq!(
                (
                    "codec.symbol-width",
                    "Every encoded output must have the configured byte width."
                ),
                LengthMismatch,
                data.len(),
                c.width,
                "{}: output width for block {block} symbol {id}",
                c.id
            );
            let expected = if source_outputs {
                &sources[block][id * c.width..(id + 1) * c.width]
            } else {
                &repair_reference.as_ref().unwrap()[block][id]
            };
            check_bytes("codec.encoded-output", expected, data, || {
                format!("{}: block {block} output {id}", c.id)
            });
            generated += data.len();
        }
    }
    if let Some(bytes) = &p.predecessor {
        p.plan
            .as_ref()
            .unwrap()
            .borrow_mut()
            .load(c.width, bytes, None, &mut Stages::disabled());
    }
    let timeline: Vec<_> = block_rows.iter().enumerate().map(|(block, (begin, ready, release))| {
        json!({"block":block,"start_ns":begin,"output_ready_ns":ready,"component_released_ns":release})
    }).collect();
    let first = block_rows[0];
    let gaps: Vec<_> = block_rows
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].1)
        .collect();
    let batch_total = if c.blocks > 1 { Some(lifecycle) } else { None };
    let ns = batch_total.unwrap_or(ready);
    let stages = stages.value(lifecycle);
    let links = link_coverage(c, &stages, diagnostic);
    Outcome {
        ns,
        witness: p.source_digest.clone(),
        resources,
        observation: None,
        status: "completed".into(),
        details: json!({"input_digest":p.source_digest,"output_bytes":generated,"repair_output_bytes":if source_outputs{0}else{generated},"retained_input_bytes":c.blocks*c.k*c.width,"blocks":c.blocks,"block_timeline":timeline,"first_block_ready_ns":first.1,"plan_constructed_ns":plan_constructed,"max_block_gap_ns":gaps.iter().max(),"batch_total_ns":batch_total,"output_ready_ns":ready,"component_released_ns":released,"release_ns":released.map(|_| release_ns),"lifecycle_ns":lifecycle,"stages":stages,"link_coverage":links,"implementation":implementation,"probe":probe.map(|p|p.snapshot()),"reuse_scope":if c.workload=="reuse-encode"{"prepared plan; replacement/loading included; creation excluded"}else{"creation included where required by workflow"}}),
    }
}
fn round_trip(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    let before = telemetry::start();
    let start = Instant::now();
    let mut stages = Stages::new(start, diagnostic);
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, &p.source, None);
    stages.mark("encoder-construction");
    let ids = offered_ids(c, encoder.repair_base());
    stages.mark("delivery-plan");
    let mut stream: Vec<_> = ids.into_iter().map(|id| (id, encoder.encode(id))).collect();
    stages.mark("encoding");
    order_packets(c, &mut stream);
    stages.mark("in-memory-delivery-ordering");
    let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, None);
    let mut received = 0;
    for (id, v) in &stream {
        decoder.add(*id, v);
        received += 1;
        if decoder.decoded() {
            break;
        }
    }
    let complete = decoder.decoded();
    let output = if complete { decoder.result() } else { vec![] };
    stages.mark("decoding-and-output");
    let ready = elapsed(start);
    let drop_start = elapsed(start);
    drop(decoder);
    drop(encoder);
    let released = elapsed(start);
    stages.mark("component-release");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    if complete {
        check_bytes("codec.recovered-payload", &p.source, &output, || {
            format!("{}: round trip after {received} inputs", c.id)
        });
    }
    require!(
        (
            "codec.symbol-width",
            "Every round-trip symbol must have the configured byte width."
        ),
        LengthMismatch,
        stream.iter().all(|(_, data)| data.len() == c.width),
        "{}: round-trip symbol width",
        c.id
    );
    let received_bytes: usize = stream[..received].iter().map(|(_, data)| data.len()).sum();
    let offered_repairs = stream.iter().filter(|(id, _)| *id >= c.k).count();
    let actual_repairs = stream[..received]
        .iter()
        .filter(|(id, _)| *id >= c.k)
        .count();
    Outcome {
        ns: ready,
        witness: digest(&p.source),
        resources,
        observation: None,
        status: if complete {
            "completed"
        } else {
            "incomplete_recovery"
        }
        .into(),
        details: json!({"output_ready_ns":complete.then_some(ready),"attempt_end_ns":ready,"component_released_ns":released,"release_ns":released-drop_start,"lifecycle_ns":lifecycle,"received_symbols":received,"received_bytes":received_bytes,"received_sources":received-actual_repairs,"received_repairs":actual_repairs,"offered_symbols":stream.len(),"repair_output_bytes":offered_repairs*c.width,"input_digest":stream_digest(&stream),"stages":stages.value(lifecycle),"scope":"directly timed in-memory encode-delivery-decode; not network throughput"}),
    }
}
