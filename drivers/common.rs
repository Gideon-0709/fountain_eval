//! Shared acceptance semantics. This source participates in T's content identity.
mod api;
mod config;
mod operator;
use operator::{Operator, Program};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

#[derive(Clone, Debug)]
pub enum Action {
    Zero(usize),
    Range(usize, usize),
    Copy(usize, usize),
    Move(usize, usize),
    Xor(usize, usize),
    XorTwo(usize, usize, usize),
    Scale(u8, usize),
    MulAdd(usize, u8, usize),
    Remove(usize),
}
#[derive(Default)]
pub struct Probe {
    pub reads: AtomicUsize,
    pub writes: AtomicUsize,
    pub arithmetic: AtomicUsize,
    pub drops: AtomicUsize,
}
pub fn hit(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::Relaxed);
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
thread_local! { static CURRENT_CASE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
fn begin_case(case: &str) {
    CURRENT_CASE.with(|v| *v.borrow_mut() = case.to_owned());
}
fn emit(mut value: Value) {
    if value["event"] == "validation" && value.get("severity").is_none() {
        value["severity"] = json!("fatal");
        value["status"] = json!("passed");
        value["rule"] = json!(
            if value["case"].as_str().unwrap_or("").starts_with("codec/") {
                "codec-payload"
            } else {
                "operator-core"
            }
        );
    }
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value).unwrap();
    writeln!(out).unwrap();
    out.flush().unwrap();
}
fn source(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((i.wrapping_mul(131).wrapping_add(17)) ^ (i >> 8)) as u8)
        .collect()
}
fn mul(mut a: u8, mut b: u8) -> u8 {
    let mut out = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            out ^= a;
        }
        a = if a & 128 != 0 {
            (a << 1) ^ 0x1d
        } else {
            a << 1
        };
        b >>= 1;
    }
    out
}
fn trace() -> Vec<Action> {
    use Action::*;
    vec![
        Copy(0, 8),
        Xor(1, 8),
        Scale(7, 8),
        MulAdd(8, 3, 2),
        Copy(2, 9),
        Scale(0, 2),
        Move(9, 10),
        Xor(0, 10),
        Copy(0, 11),
        Xor(0, 11),
        MulAdd(1, 7, 5),
        Zero(6),
        Remove(3),
        Range(3, 5),
    ]
}
fn reference(width: usize, actions: &[Action]) -> BTreeMap<usize, Vec<u8>> {
    let mut values: BTreeMap<_, _> = (0..8)
        .map(|i| {
            (
                i,
                source(width)
                    .iter()
                    .map(|v| v.wrapping_add(i as u8))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    for action in actions {
        match *action {
            Action::Zero(i) => {
                values.insert(i, vec![0; width]);
            }
            Action::Range(a, b) => {
                for i in a..b {
                    values.insert(i, vec![0; width]);
                }
            }
            Action::Copy(a, b) => {
                let v = values[&a].clone();
                values.insert(b, v);
            }
            Action::Move(a, b) => {
                if a != b {
                    let v = values.remove(&a).unwrap();
                    values.insert(b, v);
                }
            }
            Action::Xor(a, b) => {
                let v = values[&a].clone();
                for (x, y) in values.get_mut(&b).unwrap().iter_mut().zip(v) {
                    *x ^= y;
                }
            }
            Action::XorTwo(a, b, target) => {
                for id in [a, b] {
                    let src = values[&id].clone();
                    for (x, y) in values.get_mut(&target).unwrap().iter_mut().zip(src) {
                        *x ^= y;
                    }
                }
            }
            Action::Scale(c, i) => {
                for v in values.get_mut(&i).unwrap() {
                    *v = mul(*v, c);
                }
            }
            Action::MulAdd(a, c, b) => {
                let v = values[&a].clone();
                for (x, y) in values.get_mut(&b).unwrap().iter_mut().zip(v) {
                    *x ^= mul(y, c);
                }
            }
            Action::Remove(i) => {
                values.remove(&i);
            }
        }
    }
    values
}
fn checked_ns(start: Instant) -> u64 {
    let n = u64::try_from(start.elapsed().as_nanos()).unwrap();
    assert!(n > 0, "clock returned zero duration");
    n
}
fn sample(case: &str, rep: usize, ns: u64, bytes: u64, operations: u64, witness: String) {
    emit(
        json!({"event":"sample","case":case,"repetition":rep,"elapsed_ns":ns,"payload_bytes":bytes,"operations":operations,"witness":witness}),
    );
}
fn operator_suite(measure: bool) {
    assert_eq!(mul(0x80, 2), 0x1d);
    assert_eq!(mul(0xff, 2), 0xe3);
    for width in [1, 31, 128] {
        let actions = trace();
        let expected = reference(width, &actions);
        let witness = digest(&serde_json::to_vec(&expected).unwrap());
        for direct in [false, true] {
            let case = format!(
                "operator/{width}/{}",
                if direct { "direct" } else { "execute" }
            );
            begin_case(&case);
            let program = Program::new(&actions, direct);
            if !measure {
                for end in 1..actions.len() {
                    let mut op = Operator::new(width);
                    for i in 0..8 {
                        op.insert(
                            i,
                            &source(width)
                                .iter()
                                .map(|v| v.wrapping_add(i as u8))
                                .collect::<Vec<_>>(),
                        );
                    }
                    op.run(&Program::new(&actions[..end], direct));
                    for (id, bytes) in reference(width, &actions[..end]) {
                        assert_eq!(
                            op.read(id),
                            bytes,
                            "operator prefix {end} mismatch in {case}"
                        );
                    }
                }
            }
            for rep in 0..if measure { 4 } else { 1 } {
                let mut op = Operator::new(width);
                for i in 0..8 {
                    let bytes: Vec<_> = source(width)
                        .iter()
                        .map(|v| v.wrapping_add(i as u8))
                        .collect();
                    op.insert(i, &bytes);
                }
                let start = Instant::now();
                op.run(&program);
                let ns = checked_ns(start);
                for (id, bytes) in &expected {
                    assert_eq!(op.read(*id), *bytes, "operator mismatch in {case} at {id}");
                }
                drop(op);
                if measure && rep > 0 {
                    sample(&case, rep - 1, ns, 0, actions.len() as u64, witness.clone());
                }
            }
            if !measure {
                emit(
                    json!({"event":"validation","case":case,"witness":witness,"checks":["scalar-reference","alias-isolation","move-remove-zero"]}),
                );
            }
        }
    }
}
/// Each timed sample uses independent preloaded operators, so repeated calls cannot cancel a deviation.
fn boundary_suite(measure: bool) {
    for width in [1, 31, 128] {
        for direct in [false, true] {
            for rule in ["self-xor", "duplicate-source-xor"] {
                let case = format!(
                    "operator/{width}/{}/{rule}",
                    if direct { "direct" } else { "execute" }
                );
                begin_case(&case);
                let actions = if rule == "self-xor" {
                    vec![Action::Xor(0, 0)]
                } else {
                    vec![Action::XorTwo(0, 0, 1)]
                };
                let program = Program::new(&actions, direct);
                let expected = serde_json::to_value(reference(width, &actions)).unwrap();
                let initial = reference(width, &[]);
                let calls = if measure { 256 } else { 1 };
                for rep in 0..if measure { 4 } else { 1 } {
                    let mut operators: Vec<_> = (0..calls)
                        .map(|_| {
                            let mut op = Operator::new(width);
                            for (id, bytes) in &initial {
                                op.insert(*id, bytes);
                            }
                            op
                        })
                        .collect();
                    let ns = if measure {
                        let start = Instant::now();
                        for op in &mut operators {
                            std::hint::black_box(op).run(std::hint::black_box(&program));
                        }
                        checked_ns(start)
                    } else {
                        operators[0].run(&program);
                        0
                    };
                    let mut outcomes: BTreeMap<String, (u64, Value)> = BTreeMap::new();
                    for op in &operators {
                        let actual: BTreeMap<_, _> = (0..8).map(|id| (id, op.read(id))).collect();
                        let actual = serde_json::to_value(actual).unwrap();
                        let key = digest(&serde_json::to_vec(&actual).unwrap());
                        outcomes.entry(key).or_insert((0, actual)).0 += 1;
                    }
                    drop(operators);
                    if !measure {
                        let (witness, (_, actual)) = outcomes.into_iter().next().unwrap();
                        emit(
                            json!({"event":"validation","case":case,"rule":rule,"severity":"warning","status":if actual==expected {"passed"} else {"failed"},"expected":expected,"actual":actual,"witness":witness,"operation":if rule=="self-xor" {json!({"kind":"AddOneToVector","src_id":0,"target_id":0})}else{json!({"kind":"AddTwoToVector","s0":0,"s1":0,"target_id":1})},"symbol_bytes":width,"dispatch":if direct {"direct"}else{"execute"},"scope":if config::FAMILY=="product-abi" {"callback-operator"} else {"operator"},"message":if actual==expected {"Matches the scalar reference"} else {"Boundary behavior differs from the scalar reference; reported without failing the run"}}),
                        );
                    } else if rep > 0 {
                        let outcomes: Vec<_> = outcomes
                            .into_values()
                            .map(|(count, actual)| json!({"count":count,"actual":actual}))
                            .collect();
                        let observation = json!({"calls":calls,"outcomes":outcomes});
                        let witness = digest(&serde_json::to_vec(&observation).unwrap());
                        emit(
                            json!({"event":"sample","case":case,"repetition":rep-1,"elapsed_ns":ns,"payload_bytes":0,"operations":calls,"witness":witness,"observation":observation}),
                        );
                    }
                }
            }
        }
    }
}
const K: usize = 64;
const WIDTH: usize = 128;
const REPAIRS: usize = 128;
fn codec_round(codec: &str, missing: usize, measure: bool, rep: usize) {
    let prefix = format!("codec/{codec}/missing-{missing}");
    begin_case(&format!("{prefix}/encode"));
    let bytes = source(K * WIDTH);
    let probe = (!measure).then(|| Arc::new(Probe::default()));
    let start = Instant::now();
    let mut encoder = api::Encoder::new(codec, K, WIDTH, &bytes, probe.clone());
    let base = encoder.repair_base();
    let mut packets = Vec::new();
    for id in 0..K {
        packets.push((id, encoder.encode(id)));
    }
    for id in base..base + REPAIRS {
        packets.push((id, encoder.encode(id)));
    }
    let encode_ns = checked_ns(start);
    for (id, payload) in packets.iter().take(K) {
        assert_eq!(
            *payload,
            bytes[id * WIDTH..(id + 1) * WIDTH],
            "systematic output mismatch"
        );
    }
    let encoded_witness = digest(
        &packets
            .iter()
            .flat_map(|(_, p)| p.iter().copied())
            .collect::<Vec<_>>(),
    );
    drop(encoder);
    let received: Vec<_> = packets.iter().filter(|(id, _)| *id >= missing).collect();
    begin_case(&format!("{prefix}/decode"));
    let start = Instant::now();
    let mut decoder = api::Decoder::new(codec, K, WIDTH, probe.clone());
    let mut consumed = 0;
    for (id, payload) in received {
        decoder.add(*id, payload);
        consumed += 1;
        if !measure && consumed == 1 {
            assert!(
                !decoder.decoded(),
                "single symbol incorrectly completes decoding"
            );
        }
        if decoder.decoded() {
            break;
        }
    }
    let decoded = decoder.decoded();
    let recovered = if decoded {
        decoder.result()
    } else {
        Vec::new()
    };
    let decode_ns = checked_ns(start);
    assert!(decoded, "repair budget exhausted for {codec}");
    assert_eq!(recovered, bytes, "decoded payload mismatch");
    if missing > 0 {
        assert!(consumed > K - missing, "repair path was not exercised");
    }
    drop(decoder);
    if let Some(probe) = probe {
        let reads = probe.reads.load(Ordering::Relaxed);
        let writes = probe.writes.load(Ordering::Relaxed);
        let drops = probe.drops.load(Ordering::Relaxed);
        assert!(
            reads > 0 && writes > 0,
            "selected operator did not participate"
        );
        assert_eq!(drops, 2, "operator lifecycle did not close");
        for phase in ["encode", "decode"] {
            emit(
                json!({"event":"validation","case":format!("{prefix}/{phase}"),"witness":digest(&bytes),"encoded_witness":encoded_witness,"checks":["payload-equality","incomplete-state","selected-operator","release"],"operator_reads":reads,"operator_writes":writes,"operator_drops":drops,"received":consumed}),
            );
        }
    } else if rep > 0 {
        sample(
            &format!("{prefix}/encode"),
            rep - 1,
            encode_ns,
            (K * WIDTH) as u64,
            0,
            digest(&bytes),
        );
        sample(
            &format!("{prefix}/decode"),
            rep - 1,
            decode_ns,
            (K * WIDTH) as u64,
            0,
            digest(&bytes),
        );
    }
}
fn codec_suite(measure: bool) {
    for codec in [config::SCHEME] {
        for missing in [0, 8] {
            for rep in 0..if measure { 4 } else { 1 } {
                codec_round(codec, missing, measure, rep);
            }
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    emit(
        json!({"event":"hello","protocol":1,"build":config::BUILD,"interface":config::INTERFACE,"test":config::TEST,"operator":config::OPERATOR,"family":config::FAMILY,"scheme":config::SCHEME}),
    );
    let result = std::panic::catch_unwind(|| match mode {
        "describe" => {}
        "validate" | "measure" => {
            assert_eq!(config::TEST, "0.1.0");
            if config::FAMILY != "product-abi" {
                operator_suite(mode == "measure");
            }
            if mode == "validate" || config::FAMILY != "product-abi" {
                boundary_suite(mode == "measure");
            }
            codec_suite(mode == "measure");
        }
        _ => panic!("unknown driver command"),
    });
    match result {
        Ok(()) => emit(json!({"event":"done","mode":mode})),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "driver panic".into());
            let case = CURRENT_CASE.with(|v| v.borrow().clone());
            emit(
                json!({"event":"validation","case":case,"severity":"fatal","status":"failed","rule":"execution-integrity","phase":mode,"message":message}),
            );
            emit(json!({"event":"error","message":message}));
            std::process::exit(4);
        }
    }
}
