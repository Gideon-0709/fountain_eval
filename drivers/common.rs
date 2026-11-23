//! Shared protocol and execution for formal T content. Native bindings remain in api/operator.
mod api;
mod config;
mod content;
mod contracts;
mod findings;
mod operator;
mod reference;
mod telemetry;
mod workloads;
use content::Case;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
#[derive(Default)]
pub struct Probe {
    pub reads: AtomicUsize,
    pub writes: AtomicUsize,
    pub arithmetic: AtomicUsize,
    pub drops: AtomicUsize,
    counts: Mutex<BTreeMap<String, usize>>,
}
pub fn hit(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::Relaxed);
}
impl Probe {
    pub fn hit(&self, name: &str) {
        match name {
            "read" => hit(&self.reads),
            "write" => hit(&self.writes),
            "drop" => hit(&self.drops),
            _ => hit(&self.arithmetic),
        };
        *self.counts.lock().unwrap().entry(name.into()).or_default() += 1;
    }
    pub fn snapshot(&self) -> Value {
        json!({"reads":self.reads.load(Ordering::Relaxed),"writes":self.writes.load(Ordering::Relaxed),"arithmetic_calls":self.arithmetic.load(Ordering::Relaxed),"drops":self.drops.load(Ordering::Relaxed),"operation_counts":*self.counts.lock().unwrap()})
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn elapsed(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap()
}
pub struct Stages {
    last: Instant,
    enabled: bool,
    values: BTreeMap<String, u64>,
}
impl Stages {
    pub fn new(start: Instant, enabled: bool) -> Self {
        Self {
            last: start,
            enabled,
            values: BTreeMap::new(),
        }
    }
    pub fn disabled() -> Self {
        Self::new(Instant::now(), false)
    }
    pub fn mark(&mut self, name: &str) {
        if self.enabled {
            let now = Instant::now();
            let ns = now.duration_since(self.last).as_nanos() as u64;
            *self.values.entry(name.into()).or_default() += ns;
            self.last = now;
        }
    }
    pub fn value(&self, total: u64) -> Value {
        let sum: u64 = self.values.values().sum();
        assert!(sum <= total, "stage accounting exceeds parent");
        json!({"clock":"monotonic worker-local","exclusive_ns":self.values,"total_ns":total,"unattributed_ns":total-sum})
    }
}
#[derive(Deserialize)]
struct Check {
    spec: Case,
    skip: Option<Value>,
    measurement_skip: Option<Value>,
    measured: bool,
    rule: String,
    severity: String,
}
#[derive(Deserialize)]
struct Plan {
    checks: BTreeMap<String, Check>,
}
thread_local! {static CURRENT:std::cell::RefCell<String>=const{std::cell::RefCell::new(String::new())};}
fn emit(value: Value) {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value).unwrap();
    writeln!(out).unwrap();
    out.flush().unwrap();
}
fn validation(check: &Check, equivalent: &mut BTreeMap<String, workloads::Validation>) {
    let c = &check.spec;
    CURRENT.with(|x| *x.borrow_mut() = c.id.clone());
    let result = if c.category == "operation" {
        // The precheck always uses one instance with identical inputs for both batches.
        let key = c.id.rsplit_once('/').unwrap().0.to_owned();
        equivalent
            .entry(key)
            .or_insert_with(|| {
                findings::scope(
                    findings::case_contract(c),
                    findings::case_context(c),
                    || workloads::validate(c),
                )
            })
            .clone()
    } else {
        findings::scope(
            findings::case_contract(c),
            findings::case_context(c),
            || workloads::validate(c),
        )
    };
    let mut event = json!({"event":"validation","case":c.id,"rule":check.rule,"severity":check.severity,"status":"passed","witness":result.witness,"details":result.details});
    if c.warning {
        let expected = result.expected.expect("warning reference missing");
        let actual = result.actual.expect("warning actual missing");
        event["status"] = json!(if expected == actual {
            "passed"
        } else {
            "failed"
        });
        event["expected"] = expected;
        event["actual"] = actual.clone();
        event["finding"] = json!(findings::boundary_violation(c, &event["expected"], &actual));
        event["witness"] = json!(digest(&serde_json::to_vec(&actual).unwrap()));
        event["message"] =
            json!("Boundary reference observation; deviations do not block timing or comparison");
    }
    emit(event);
}
fn validation_suite(plan: &Plan) {
    let mut checks: Vec<_> = plan.checks.values().filter(|c| c.skip.is_none()).collect();
    checks.sort_by(|a, b| a.spec.validation_key().cmp(&b.spec.validation_key()));
    let mut equivalent = BTreeMap::new();
    let mut fatal = false;
    for check in checks {
        if check.spec.validation_family().is_none() {
            if fatal {
                break;
            }
            validation(check, &mut equivalent);
            continue;
        }
        // Each V case owns its fixture. A failed case must not suppress other V cases.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            validation(check, &mut equivalent);
        }));
        if let Err(error) = result {
            fatal = true;
            let failure = findings::from_panic(
                error,
                findings::case_contract(&check.spec),
                findings::case_context(&check.spec),
            );
            let message = failure.message;
            emit(
                json!({"event":"validation","case":check.spec.id,"severity":"fatal",
                "status":"failed","rule":"execution-integrity","phase":"validate",
                "message":message,"witness":digest(message.as_bytes()),"finding":failure.violation}),
            );
        }
    }
}
fn measure(check: &Check, worker: usize, cold: bool) {
    let c = &check.spec;
    CURRENT.with(|x| *x.borrow_mut() = c.id.clone());
    let diagnostic = cfg!(feature = "diagnostics");
    let prepared = findings::scope(
        findings::case_contract(c),
        findings::case_context(c),
        || workloads::prepare(c),
    );
    let count = if cold || diagnostic {
        1
    } else {
        content::SAMPLES_PER_WORKER + 1
    };
    for iteration in 0..count {
        let outcome = findings::scope(
            findings::case_contract(c),
            findings::case_context(c),
            || workloads::run(c, &prepared, diagnostic),
        );
        if !cold && !diagnostic && iteration == 0 {
            continue;
        }
        assert!(outcome.ns > 0, "unresolved zero-duration observation");
        let rep = if cold || diagnostic {
            worker
        } else {
            worker * content::SAMPLES_PER_WORKER + iteration - 1
        };
        if diagnostic {
            emit(
                json!({"event":"diagnostic","case":c.id,"worker":worker,"elapsed_ns":outcome.ns,"observation":outcome.details,"witness":outcome.witness,"resources":outcome.resources}),
            );
        } else {
            emit(
                json!({"event":"sample","case":c.id,"repetition":rep,"worker":worker,"elapsed_ns":outcome.ns,"payload_bytes":c.payload_bytes(),"operations":if c.workload.ends_with("-replay"){outcome.details["calls"].as_u64().unwrap()}else{c.calls()},"witness":outcome.witness,"observation":outcome.observation,"details":outcome.details,"outcome":outcome.status}),
            );
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let worker = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0usize);
    emit(
        json!({"event":"hello","protocol":1,"build":config::BUILD,"interface":config::INTERFACE,"test":config::TEST,"operator":config::OPERATOR,"family":config::FAMILY,"scheme":config::SCHEME,"diagnostic":cfg!(feature="diagnostics"),"worker":worker}),
    );
    let result = std::panic::catch_unwind(|| {
        let plan: Plan = serde_json::from_str(config::PLAN).unwrap();
        match mode {
            "describe" => {}
            "validate" => validation_suite(&plan),
            "measure" | "diagnose" => {
                assert_eq!(mode == "diagnose", cfg!(feature = "diagnostics"));
                assert!(worker < content::WORKERS);
                let checks = plan.checks.values().filter(|c| {
                    c.skip.is_none()
                        && c.measured
                        && c.measurement_skip.is_none()
                        && !c.spec.fresh_process
                        && (mode != "diagnose" || c.spec.category != "operation")
                });
                let mut groups: BTreeMap<&str, Vec<&Check>> = BTreeMap::new();
                for check in checks {
                    groups.entry(&check.spec.category).or_default().push(check);
                }
                for (_, mut group) in groups {
                    if worker == 1 {
                        group.reverse();
                    } else if worker == 2 {
                        let middle = group.len() / 2;
                        group.rotate_left(middle);
                    }
                    for check in group {
                        measure(check, worker, false);
                    }
                }
            }
            "first-use" => {
                assert!(worker < content::REPETITIONS);
                let id = args.get(3).expect("first-use case missing");
                let check = &plan.checks[id];
                assert!(check.skip.is_none() && check.spec.fresh_process);
                measure(check, worker, true);
            }
            _ => panic!("unknown driver mode"),
        }
    });
    match result {
        Ok(()) => emit(json!({"event":"done","mode":mode})),
        Err(error) => {
            let case = CURRENT.with(|x| x.borrow().clone());
            let failure = findings::from_panic(
                error,
                findings::Contract::new(
                    "execution.integrity",
                    "The driver must complete the requested stage.",
                ),
                json!({"case":case,"phase":mode}),
            );
            let message = failure.message;
            emit(
                json!({"event":"validation","case":case,"severity":"fatal","status":"failed","rule":"execution-integrity","phase":mode,"message":message,"finding":failure.violation}),
            );
            emit(json!({"event":"error","message":message}));
            std::process::exit(4);
        }
    }
}
