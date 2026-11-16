//! T-owned correctness, individual-operation, chain and integrated workloads.
use crate::{
    Probe, Stages, api,
    content::{self, Action, Case},
    digest, elapsed,
    operator::{Operator, Program},
    reference::{self, State},
    telemetry,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Instant};
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
    packets: Vec<(usize, Vec<u8>)>,
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
fn state_witness(state: &State) -> String {
    digest(&serde_json::to_vec(&reference::compact(state)).unwrap())
}
fn packets(c: &Case, bytes: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, bytes, None);
    let base = encoder.repair_base();
    let mut omitted: Vec<_> = (0..c.k).collect();
    reference::shuffle(&mut omitted, c.seed ^ 0x10_55);
    omitted.truncate(c.missing);
    let mut stream: Vec<_> = (0..c.k)
        .filter(|id| !omitted.contains(id))
        .map(|id| (id, encoder.encode(id)))
        .collect();
    for id in base..base + c.budget() {
        stream.push((id, encoder.encode(id)));
    }
    if c.order == "mixed" {
        reference::shuffle(&mut stream, c.seed ^ 0x50_17);
    }
    stream
}
pub fn prepare(c: &Case) -> Prepared {
    let source = source(c);
    let mut expected = reference::initial(c.width, c.seed);
    for action in &c.actions {
        reference::apply(&mut expected, action, c.width, c.seed);
    }
    let program =
        (!c.actions.is_empty()).then(|| Program::new(&c.actions, c.direct, c.width, c.seed));
    let packets = if matches!(
        c.workload.as_str(),
        "decode" | "decode-chain" | "decode-replay"
    ) {
        packets(c, &source)
    } else {
        vec![]
    };
    let plan = if c.workload == "reuse-encode" {
        let mut plan = api::Plan::new(scheme(c), c.k);
        plan.load(c.width, &source, None, &mut Stages::disabled());
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
    Prepared {
        initial: reference::initial(c.width, c.seed),
        source,
        packets,
        program,
        expected,
        plan,
        replay,
    }
}
fn checked_state(op: &Operator, expected: &State, context: &str) {
    assert_eq!(op.snapshot(expected), *expected, "{context}");
}
fn linear_reference(c: &Case) -> Value {
    let k = c.k;
    let width = c.width;
    let bytes = source(c);
    let mut basis = vec![0; k * k];
    for i in 0..k {
        basis[i * k + i] = 1;
    }
    let mut generator = api::Encoder::new(scheme(c), k, k, &basis, None);
    let mut encoder = api::Encoder::new(scheme(c), k, width, &bytes, None);
    let base = encoder.repair_base();
    assert_eq!(base, generator.repair_base());
    let mut rows = Vec::new();
    let mut decoder = api::Decoder::new(scheme(c), k, width, None);
    for id in (0..k).filter(|i| *i % 3 != 0).chain(base..base + 128) {
        let coefficients = generator.encode(id);
        assert_eq!(coefficients.len(), k);
        let actual = encoder.encode(id);
        let mut expected = vec![0; width];
        for i in 0..k {
            for j in 0..width {
                expected[j] ^= reference::multiply(coefficients[i], bytes[i * width + j]);
            }
        }
        assert_eq!(
            actual, expected,
            "generator linearity and independent payload arithmetic"
        );
        let mut row = coefficients;
        row.extend_from_slice(&actual);
        rows.push(row);
        decoder.add(id, &actual);
        if decoder.decoded() {
            break;
        }
    }
    let solved =
        reference::solve(rows, k, width).expect("independent received matrix must have rank K");
    assert_eq!(solved, bytes);
    assert!(decoder.decoded());
    assert_eq!(decoder.result(), bytes);
    assert!(reference::solve(vec![vec![0; k + width]], k, width).is_none());
    json!({"independent_solver":"scalar Gauss-Jordan over GF(256)","generator_source":"basis payloads through selected scheme encoder; checks linear coding and recovery, not RFC wire conformance","k":k,"rank":k})
}
fn codec_contract(c: &Case) -> Value {
    if c.workload == "linear-reference" {
        return linear_reference(c);
    }
    let bytes = source(c);
    let probe = Arc::new(Probe::default());
    if c.workload == "plan-reuse-contract" {
        let mut plan = api::Plan::new(scheme(c), c.k);
        for round in 0..3 {
            let fresh = reference::bytes(c.k * c.width, c.seed + round);
            plan.load(c.width, &fresh, None, &mut Stages::disabled());
            let mut fresh_encoder = api::Encoder::new(scheme(c), c.k, c.width, &fresh, None);
            for id in (0..c.k).chain(plan.repair_base()..plan.repair_base() + 8) {
                assert_eq!(
                    plan.encode(id),
                    fresh_encoder.encode(id),
                    "reused plan leaked a previous payload"
                );
            }
        }
        return json!({"different_payloads":3,"reuse":plan.diagnostics(),"width_contract":"fixed width; changing geometry creates a new plan"});
    }
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, &bytes, Some(probe.clone()));
    let base = encoder.repair_base();
    assert!(base >= c.k);
    for id in 0..c.k {
        assert_eq!(
            encoder.encode(id),
            bytes[id * c.width..(id + 1) * c.width],
            "systematic or padding mismatch"
        );
    }
    let source_packets: Vec<_> = (0..c.k).map(|id| (id, encoder.encode(id))).collect();
    let repair_packets: Vec<_> = (base..base + 128)
        .map(|id| (id, encoder.encode(id)))
        .collect();
    drop(encoder);
    let mut scenarios = vec![source_packets.clone()];
    if c.workload == "receive-contract" {
        let mut mixed: Vec<_> = source_packets
            .iter()
            .skip(8.min(c.k))
            .cloned()
            .chain(repair_packets.clone())
            .collect();
        reference::shuffle(&mut mixed, c.seed);
        scenarios.push(mixed);
        let mut duplicates = Vec::new();
        for p in &source_packets {
            duplicates.push(p.clone());
            duplicates.push(p.clone());
        }
        scenarios.push(duplicates);
        scenarios.push(repair_packets.clone());
        let mut early = repair_packets[..8].to_vec();
        early.extend(source_packets.clone());
        scenarios.push(early);
    }
    for stream in &scenarios {
        let mut decoder = api::Decoder::new(scheme(c), c.k, c.width, Some(probe.clone()));
        assert!(!decoder.decoded());
        for (index, (id, v)) in stream.iter().enumerate() {
            decoder.add(*id, v);
            if index == 0 && c.k > 1 {
                assert!(!decoder.decoded(), "false completion from one symbol");
            }
            if decoder.decoded() {
                break;
            }
        }
        assert!(decoder.decoded(), "fixed recovery fixture did not finish");
        assert_eq!(decoder.result(), bytes);
        drop(decoder);
    }
    let counters = probe.snapshot();
    assert!(
        counters["reads"].as_u64().unwrap() > 0 && counters["writes"].as_u64().unwrap() > 0,
        "selected operator did not participate"
    );
    assert_eq!(
        counters["drops"].as_u64().unwrap(),
        1 + scenarios.len() as u64,
        "component resources were not released"
    );
    json!({"probe":counters,"source_symbols":c.k,"first_repair_id":base,"scenarios":scenarios.len(),"output_bytes":bytes.len(),"error_contract":if c.workload=="lifecycle-contract"{api::error_contract(scheme(c),c.k,c.width,&bytes)}else{Value::Null},"field_and_source_mapping":"selected binding verified during build; source bytes and padding checked here"})
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
    let mut operators: Vec<_> = (0..instances)
        .map(|_| {
            let mut op = Operator::new(c.width);
            use fountain_engine::traits::DataOperator;
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
    let before = telemetry::start();
    let start = Instant::now();
    for op in &mut operators {
        if c.boundary.is_some() {
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
    let mut outcomes: BTreeMap<String, (u64, Value)> = BTreeMap::new();
    for (index, op) in operators.iter().enumerate() {
        if c.boundary.is_none() && !c.warning {
            checked_state(op, &p.expected, &c.id);
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
    let observation = json!({"calls":instances,"outcomes":outcomes.into_values().map(|(count,actual)|json!({"count":count,"actual":actual})).collect::<Vec<_>>()});
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
        details: json!({"public_calls":instances,"initial_symbol_bytes":instances*16*c.width,"dispatch":if c.direct{"direct"}else{"execute"},"operations":c.actions,"boundary_contract":c.boundary,"logical_byte_rate":"not physical memory bandwidth"}),
    }
}
pub fn run(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    if !c.actions.is_empty() {
        return primitive(c, p);
    }
    if c.workload.ends_with("-replay") {
        let (ns, details, resources) = p.replay.as_ref().unwrap().execute();
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
        let details = plan.diagnostics();
        let bytes = reference::bytes(c.k * 31, c.seed);
        plan.load(31, &bytes, None, &mut Stages::disabled());
        for id in 0..c.k {
            assert_eq!(plan.encode(id), bytes[id * 31..(id + 1) * 31]);
        }
        return Outcome {
            ns,
            witness: digest(&bytes),
            details,
            resources,
            observation: None,
            status: "completed".into(),
        };
    }
    if matches!(c.workload.as_str(), "decode" | "decode-chain") {
        return decode(c, p, diagnostic);
    }
    if c.workload == "round-trip" {
        return round_trip(c, p, diagnostic);
    }
    encode(c, p, diagnostic)
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
    drop(decoder);
    stages.mark("component-release");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    if complete {
        assert_eq!(output, p.source, "decoded payload");
    }
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
        details: json!({"received_symbols":consumed,"received_bytes":received_bytes,"offered_symbols":p.packets.len(),"repair_budget":c.budget(),"output_ready_ns":ready,"lifecycle_ns":lifecycle,"stages":stages.value(lifecycle),"internal":internal,"probe":probe.map(|p|p.snapshot()),"recovery":if complete{"byte-verified"}else{"fixed budget exhausted; rank not established; no successful throughput"}}),
    }
}
fn encode(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    let probe = diagnostic.then(|| Arc::new(Probe::default()));
    let sources: Vec<_> = (0..c.blocks)
        .map(|block| {
            if block == 0 {
                p.source.clone()
            } else {
                reference::bytes(c.k * c.width, c.seed + block as u64)
            }
        })
        .collect();
    let before = telemetry::start();
    let start = Instant::now();
    let mut stages = Stages::new(start, diagnostic);
    let mut reused = None;
    let mut outputs = Vec::new();
    let mut block_rows = Vec::new();
    let mut generated = 0;
    let mut ready = 0;
    let mut implementation = Value::Null;
    if c.workload == "continuous-reuse" {
        reused = Some(api::Plan::new(scheme(c), c.k));
        stages.mark("plan-or-reuse-context-creation");
    }
    for (block, bytes) in sources.iter().enumerate() {
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
            drop(encoder);
            stages.mark("component-release");
        }
        if c.workload == "reuse-encode" || reused.is_some() {
            ready = elapsed(start);
        }
        let released = elapsed(start);
        generated += block_output.iter().map(Vec::len).sum::<usize>();
        outputs.push(block_output);
        if diagnostic {
            block_rows.push(json!({"block":block,"start_ns":block_start,"output_ready_ns":ready,"release_boundary_ns":released}));
        }
    }
    drop(reused);
    stages.mark("workflow-complete");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    for (block, result) in outputs.iter().enumerate() {
        if c.workload == "encode-sources" {
            assert_eq!(result.concat(), sources[block]);
        } else {
            let mut reference_encoder =
                api::Encoder::new(scheme(c), c.k, c.width, &sources[block], None);
            let base = reference_encoder.repair_base();
            for (i, v) in result.iter().enumerate() {
                assert_eq!(
                    *v,
                    reference_encoder.encode(base + i),
                    "repair output mismatch"
                );
            }
        }
    }
    let witness = digest(&sources.concat());
    let ns = if c.workload == "lifecycle" {
        lifecycle
    } else {
        ready
    };
    Outcome {
        ns,
        witness,
        resources,
        observation: None,
        status: "completed".into(),
        details: json!({"output_bytes":generated,"repair_output_bytes":if c.workload=="encode-sources"{0}else{generated},"blocks":c.blocks,"block_timeline":block_rows,"output_ready_ns":ready,"lifecycle_ns":lifecycle,"stages":stages.value(lifecycle),"implementation":implementation,"probe":probe.map(|p|p.snapshot()),"reuse_scope":if c.workload=="reuse-encode"{"prepared plan; replacement/loading included; creation excluded"}else{"creation included where required by workflow"}}),
    }
}
fn round_trip(c: &Case, p: &Prepared, diagnostic: bool) -> Outcome {
    let before = telemetry::start();
    let start = Instant::now();
    let mut stages = Stages::new(start, diagnostic);
    let mut encoder = api::Encoder::new(scheme(c), c.k, c.width, &p.source, None);
    let mut stream = Vec::new();
    for id in c.missing..c.k {
        stream.push((id, encoder.encode(id)));
    }
    for id in encoder.repair_base()..encoder.repair_base() + c.budget() {
        stream.push((id, encoder.encode(id)));
    }
    stages.mark("encoding");
    reference::shuffle(&mut stream, c.seed);
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
    drop(decoder);
    drop(encoder);
    stages.mark("component-release");
    let lifecycle = elapsed(start);
    let resources = telemetry::finish(before);
    if complete {
        assert_eq!(output, p.source);
    }
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
        details: json!({"output_ready_ns":ready,"lifecycle_ns":lifecycle,"received_symbols":received,"offered_symbols":stream.len(),"stages":stages.value(lifecycle),"scope":"directly timed in-memory encode-delivery-decode; not network throughput"}),
    }
}
