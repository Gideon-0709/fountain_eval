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
fn all_validation_events_are_quiet_and_warning_totals_remain_visible() {
    let r = record();
    let mut p = DriverProgress::new(&r, "validate", 0, None);
    assert!(p.observe(&hello(&r, 0, false)).is_none());
    let mut warnings = 0;
    let mut count = 0;
    for check in r
        .recipe
        .as_ref()
        .unwrap()
        .checks
        .values()
        .filter(|c| c.skip.is_none())
    {
        let warning = check.severity == fountain_eval::validation::Severity::Warning;
        warnings += usize::from(warning);
        count += 1;
        let event = json!({"event":"validation","case":check.spec.id,"severity":check.severity,"rule":check.rule,"status":if warning {"failed"} else {"passed"}});
        assert!(
            p.observe(&event.to_string()).is_none(),
            "No per-case validation output"
        );
    }
    let text = p.heartbeat();
    assert!(text.contains(&format!("{count}/{count} checks observed")));
    assert!(text.contains(&format!("{warnings} warnings")));
    assert!(!text.contains("correctness/") && !text.contains("operation/"));
}
#[test]
fn fatal_errors_are_immediate_concise_and_not_repeated() {
    let r = record();
    let case = "chain/plan-build/rq/k64";
    let mut p = DriverProgress::new(&r, "validate", 0, None);
    p.observe(&hello(&r, 0, false));
    let event = json!({"event":"validation","case":case,"severity":"fatal","rule":"execution-integrity","status":"failed","message":format!("arithmetic mismatch\n{}", "state dump".repeat(1000))});
    let text = p.observe(&event.to_string()).unwrap();
    assert!(text.contains("FATAL Validation: arithmetic mismatch"));
    assert!(text.len() < 250 && !text.contains("state dump") && !text.contains(case));
    assert!(p.observe(&event.to_string()).is_none());
    assert!(p.heartbeat().contains("error"));
}
#[test]
fn v_fatals_do_not_suppress_later_progress_or_repeat_diagnostics() {
    let r = record();
    let mut p = DriverProgress::new(&r, "validate", 0, None);
    p.observe(&hello(&r, 0, false));
    let mut count = 0;
    let mut fatal = 0;
    for case in r.recipe.as_ref().unwrap().validation_cases() {
        let c = &r.recipe.as_ref().unwrap().checks[&case];
        if c.spec.validation_family().is_none() {
            break;
        }
        let event = if matches!(c.spec.validation_family(), Some("V01" | "V11")) {
            fatal += 1;
            json!({"event":"validation","case":case,"severity":"fatal","status":"failed","rule":"execution-integrity","message":"independent contract failure"})
        } else {
            json!({"event":"validation","case":case,"severity":c.severity,"status":"passed","rule":c.rule})
        };
        let output = p.observe(&event.to_string());
        if count == 0 {
            assert!(output.unwrap().contains("continuing remaining V checks"));
        } else {
            assert!(output.is_none());
        }
        count += 1;
    }
    let text = p.heartbeat();
    assert!(text.contains(&format!("{count}/")), "{text}");
    assert!(
        text.contains(&format!("{fatal} fatal; continuing V checks")),
        "{text}"
    );
    assert!(!text.contains("waiting for process exit"));
}
#[test]
fn sampling_aggregates_across_workers_without_printing_cases_or_rates() {
    let mut r = record();
    let case = "chain/plan-build/rq/k64";
    for worker in 0..content::WORKERS {
        let mut p = DriverProgress::new(&r, "measure", worker, None);
        assert!(p.observe(&hello(&r, worker, false)).is_none());
        let mut events = Vec::new();
        for i in 0..content::SAMPLES_PER_WORKER {
            let event = sample(case, worker, worker * content::SAMPLES_PER_WORKER + i);
            assert!(p.observe(&event.to_string()).is_none());
            events.push(event);
        }
        let text = p.heartbeat();
        let total = r.recipe.as_ref().unwrap().cases.len() * content::REPETITIONS;
        assert!(text.contains(&format!("{}/{total} samples observed", (worker + 1) * 5)));
        assert!(
            text.contains("Encoding plans")
                && text.contains(&format!("worker {}/3", worker + 1))
                && text.contains('%')
        );
        assert!(!text.contains(case) && !text.contains("median") && !text.contains("Gbit/s"));
        drop(p);
        r.samples.extend(
            events
                .into_iter()
                .map(|v| serde_json::from_value(v).unwrap()),
        );
    }
}
#[test]
fn incomplete_recovery_remains_an_observation_not_a_throughput_claim() {
    let r = record();
    let mut p = DriverProgress::new(&r, "measure", 0, None);
    p.observe(&hello(&r, 0, false));
    let mut event = sample("chain/plan-build/rq/k64", 0, 0);
    event["outcome"] = json!("incomplete_recovery");
    assert!(p.observe(&event.to_string()).is_none());
    assert!(p.heartbeat().contains("1 incomplete outcomes"));
    assert!(!p.heartbeat().contains("Gbit/s"));
}
#[test]
fn invalid_identity_duplicates_and_unplanned_cases_stop_live_counts() {
    let r = record();
    let mut p = DriverProgress::new(&r, "measure", 0, None);
    assert!(p.observe("{}").unwrap().contains("ERROR"));
    assert!(p.observe(&hello(&r, 0, false)).is_none());
    for case in ["unregistered", "chain/plan-build/rq/k64"] {
        let mut p = DriverProgress::new(&r, "measure", 0, None);
        p.observe(&hello(&r, 0, false));
        let event = sample(case, 0, 0).to_string();
        if case == "unregistered" {
            assert!(p.observe(&event).unwrap().contains("ERROR"));
        } else {
            assert!(p.observe(&event).is_none());
            assert!(p.observe(&event).unwrap().contains("ERROR"));
        }
        assert!(!p.heartbeat().contains("samples observed"));
    }
}
#[test]
fn first_use_and_diagnostics_are_quiet_with_separate_aggregate_counts() {
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
        let event = sample(&case, worker, worker);
        assert!(p.observe(&event.to_string()).is_none());
        assert!(p.heartbeat().contains("first-use sampling"));
        assert!(!p.heartbeat().contains(&case));
        drop(p);
        r.samples.push(serde_json::from_value(event).unwrap());
    }
    for worker in 0..content::WORKERS {
        let mut p = DriverProgress::new(&r, "diagnose", worker, None);
        p.observe(&hello(&r, worker, true));
        let event = json!({"event":"diagnostic","case":"chain/plan-build/rq/k64","worker":worker});
        assert!(p.observe(&event.to_string()).is_none());
        assert!(
            p.heartbeat()
                .contains(&format!("Diagnostics: {}/", worker + 1))
        );
        assert!(p.heartbeat().contains("observations"));
        drop(p);
        r.diagnostics.push(event);
    }
}
