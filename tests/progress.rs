mod support;
use fountain_eval::{content, evidence::Record, progress::DriverProgress};
use serde_json::{Value, json};
fn record() -> Record {
    let mut r = support::record("native-2", "rq");
    r.built = Some(support::built(std::path::Path::new("/fixture/build")));
    r.samples.clear();
    r.validation.clear();
    r.diagnostics.clear();
    r
}
fn hello(r: &Record, worker: usize, diagnostic: bool) -> String {
    let b = r.built.as_ref().unwrap();
    json!({"event":"hello","protocol":1,"build":b.key,"interface":b.interface.id,"test":r.recipe.as_ref().unwrap().identity.id,"operator":b.implementation,"family":b.family,"scheme":r.request.scheme.name,"worker":worker,"diagnostic":diagnostic}).to_string()
}
fn sample(case: &str, worker: usize, rep: usize) -> Value {
    json!({"event":"sample","case":case,"worker":worker,"repetition":rep,"elapsed_ns":1000+rep,"operations":1,"payload_bytes":128,"outcome":"completed","witness":"fixture","details":{}})
}
#[test]
fn validation_warns_without_stopping_and_reports_fatal_distinctly() {
    let r = record();
    let recipe = r.recipe.as_ref().unwrap();
    let check = recipe
        .checks
        .values()
        .find(|c| c.skip.is_none() && c.spec.warning)
        .unwrap();
    let good = recipe
        .checks
        .values()
        .find(|c| c.skip.is_none() && !c.spec.warning)
        .unwrap();
    let mut p = DriverProgress::new(&r, "validate", 0, None);
    assert!(p.observe(&hello(&r, 0, false)).is_none());
    let warn = json!({"event":"validation","case":check.spec.id,"severity":"warning","rule":check.rule,"status":"failed"});
    let line = p.observe(&warn.to_string()).unwrap();
    assert!(line.contains("[WARN]") && line.contains("timing remains enabled"));
    let pass = json!({"event":"validation","case":good.spec.id,"severity":"fatal","rule":good.rule,"status":"passed"});
    assert!(p.observe(&pass.to_string()).unwrap().contains("[PASS]"));
    let fatal = json!({"event":"validation","case":good.spec.id,"severity":"fatal","rule":"execution-integrity","status":"failed","message":"arithmetic mismatch"});
    assert!(p.observe(&fatal.to_string()).unwrap().contains("[FATAL]"));
}
#[test]
fn worker_progress_is_not_full_completion_until_all_samples_arrive() {
    let mut r = record();
    let case = "chain/plan-build/rq/k64";
    for worker in 0..content::WORKERS {
        let mut p = DriverProgress::new(&r, "measure", worker, None);
        assert!(p.observe(&hello(&r, worker, false)).is_none());
        let mut events = Vec::new();
        let mut messages = Vec::new();
        for i in 0..content::SAMPLES_PER_WORKER {
            let event = sample(case, worker, worker * content::SAMPLES_PER_WORKER + i);
            if let Some(line) = p.observe(&event.to_string()) {
                messages.push(line);
            }
            events.push(event);
        }
        assert_eq!(messages.len(), 1);
        let text = &messages[0];
        assert!(text.contains(&format!("{}/15 samples", (worker + 1) * 5)));
        if worker < 2 {
            assert!(text.contains("[COLLECTED]") && !text.contains("median"));
        } else {
            assert!(
                text.contains("[RECORDED]") && text.contains("median") && text.contains("Gbit/s")
            );
        }
        drop(p);
        r.samples.extend(
            events
                .into_iter()
                .map(|v| serde_json::from_value(v).unwrap()),
        );
    }
}
#[test]
fn incomplete_recovery_is_never_displayed_as_successful_throughput() {
    let mut r = record();
    let case = "chain/plan-build/rq/k64";
    r.samples = (0..14)
        .map(|rep| serde_json::from_value(sample(case, rep / 5, rep)).unwrap())
        .collect();
    let mut p = DriverProgress::new(&r, "measure", 2, None);
    p.observe(&hello(&r, 2, false));
    let mut event = sample(case, 2, 14);
    event["outcome"] = json!("incomplete_recovery");
    let text = p.observe(&event.to_string()).unwrap();
    assert!(text.contains("1 incomplete samples; no successful throughput"));
    assert!(!text.contains("Gbit/s"));
}
#[test]
fn invalid_identity_duplicates_and_unplanned_cases_stop_live_counts() {
    let r = record();
    let mut p = DriverProgress::new(&r, "measure", 0, None);
    assert!(p.observe("{}").unwrap().contains("[PROTOCOL]"));
    assert!(p.observe(&hello(&r, 0, false)).is_none());
    for case in ["unregistered", "chain/plan-build/rq/k64"] {
        let mut p = DriverProgress::new(&r, "measure", 0, None);
        p.observe(&hello(&r, 0, false));
        let e = sample(case, 0, 0).to_string();
        if case == "unregistered" {
            assert!(p.observe(&e).unwrap().contains("[PROTOCOL]"));
        } else {
            assert!(p.observe(&e).is_none());
            assert!(p.observe(&e).unwrap().contains("[PROTOCOL]"));
        }
    }
}
#[test]
fn fresh_processes_and_diagnostics_have_separate_completion_counts() {
    let mut r = record();
    let case = r
        .recipe
        .as_ref()
        .unwrap()
        .checks
        .values()
        .find(|c| c.skip.is_none() && c.spec.fresh_process)
        .unwrap()
        .spec
        .id
        .clone();
    for worker in 0..content::REPETITIONS {
        let mut p = DriverProgress::new(&r, "first-use", worker, Some(&case));
        p.observe(&hello(&r, worker, false));
        let e = sample(&case, worker, worker);
        let result = p.observe(&e.to_string());
        if worker == 14 {
            assert!(result.unwrap().contains("15/15 samples"));
        } else {
            assert!(result.is_none());
        }
        drop(p);
        r.samples.push(serde_json::from_value(e).unwrap());
    }
    let case = "chain/plan-build/rq/k64";
    for worker in 0..content::WORKERS {
        let mut p = DriverProgress::new(&r, "diagnose", worker, None);
        p.observe(&hello(&r, worker, true));
        let e = json!({"event":"diagnostic","case":case,"worker":worker});
        let text = p.observe(&e.to_string()).unwrap();
        assert!(text.contains(&format!("{}/3 observations", worker + 1)));
        assert_eq!(text.contains("[RECORDED]"), worker == 2);
        drop(p);
        r.diagnostics.push(e);
    }
}
