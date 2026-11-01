mod support;
use fountain_eval::{catalog, evidence, execution, measurement, util};
use serde_json::json;
use support::{Temp, record};
#[test]
fn metrics_are_decimal_gigabits_and_operations_per_second() {
    let codec = record("native-2", "codec-smoke@0.1.0");
    assert_eq!(codec.summary[0].source_payload_gbps, Some(32.768));
    assert_eq!(codec.summary[0].median_ns, 2000);
    let op = record("native-2", "operator-smoke@0.1.0");
    assert_eq!(op.summary[0].operations_per_second, Some(7_000_000.0));
    assert_eq!(op.summary[0].source_payload_gbps, None);
    let mut bad = codec.samples;
    bad[0].elapsed_ns = 0;
    assert!(measurement::summarize(&bad).is_err());
}
#[test]
fn incomplete_duplicate_corrupt_and_wrong_work_samples_fail() {
    let good = record("native-2", "codec-smoke@0.1.0");
    assert!(evidence::validate_samples(&good).is_ok());
    for mutation in 0..6 {
        let mut bad = good.clone();
        match mutation {
            0 => {
                bad.samples.pop();
            }
            1 => bad.samples.push(bad.samples[0].clone()),
            2 => bad.samples[0].witness = "wrong".into(),
            3 => bad.samples[0].payload_bytes = 8193,
            4 => bad.recipe.as_mut().unwrap().repetitions = 2,
            _ => bad.validation.push(bad.validation[0].clone()),
        };
        assert!(
            evidence::validate_samples(&bad).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn comparison_separates_observations_conditions_and_identity_collisions() {
    let left = record("native-2", "codec-smoke@0.1.0");
    let mut right = left.clone();
    right.run_id = "another-run".into();
    right.quadruple.as_mut().unwrap().e.observations = json!({"time":99});
    assert!(evidence::compare(&left, &right).is_ok());
    right.quadruple.as_mut().unwrap().i = catalog::interface("product-abi");
    right.quadruple.as_mut().unwrap().c = json!({"engine":"another-component"});
    let result = evidence::compare(&left, &right).unwrap();
    assert_eq!(
        result["attribution"],
        "component and interface path together"
    );
    for mutation in 0..6 {
        let mut bad = right.clone();
        match mutation {
            0 => bad.status = "failed".into(),
            1 => {
                bad.quadruple.as_mut().unwrap().e.conditions["runtime"]["cpu"] = json!("different")
            }
            2 => bad.quadruple.as_mut().unwrap().t.digest = "changed-content".into(),
            3 => bad.quadruple.as_mut().unwrap().i.digest = "changed-content".into(),
            4 => bad.quadruple.as_mut().unwrap().i.id = "unknown@0.1.0".into(),
            _ => {
                bad.validation[0]["witness"] = json!("wrong");
            }
        };
        assert!(
            evidence::compare(&left, &bad).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn sealed_results_do_not_need_a_build_cache_and_detect_corruption() {
    let t = Temp::new();
    let record = record("native-2", "codec-smoke@0.1.0");
    evidence::save(&t.0, &record).unwrap();
    evidence::seal(&t.0).unwrap();
    assert_eq!(evidence::load(&t.0).unwrap().run_id, record.run_id);
    std::fs::write(t.0.join("request.json"), "corrupt").unwrap();
    assert!(evidence::load(&t.0).is_err());
}
#[test]
fn artifact_mutation_and_unindexed_binary_fail() {
    let t = Temp::new();
    let mut built = support::built(&t.0);
    assert!(built.verify().is_err());
    t.write("driver", "binary");
    built.artifacts.insert(
        "driver".into(),
        util::file_hash(&t.0.join("driver")).unwrap(),
    );
    built.verify().unwrap();
    t.write("driver", "changed");
    assert!(built.verify().is_err());
}
#[test]
fn subprocess_protocol_requires_identity_completion_and_expected_events() {
    let t = Temp::new();
    let built = support::built(&t.0);
    let record = record("native-2", "codec-smoke@0.1.0");
    let events = vec![
        json!({"event":"hello","protocol":1,"build":built.key,"interface":built.interface.id,"test":"codec-smoke@0.1.0","operator":"slab","family":"native-2"}),
        json!({"event":"done","mode":"describe"}),
    ];
    execution::check_events(&events, &built, &record, "describe").unwrap();
    for mutation in 0..4 {
        let mut bad = events.clone();
        match mutation {
            0 => {
                bad.pop();
            }
            1 => bad[0]["build"] = json!("wrong"),
            2 => bad.insert(1, json!({"event":"sample"})),
            _ => bad[1]["mode"] = json!("measure"),
        };
        assert!(execution::check_events(&bad, &built, &record, "describe").is_err());
    }
}

#[test]
fn failed_and_prepared_stages_do_not_imply_measurement() {
    let mut r = record("native-2", "operator-smoke@0.1.0");
    r.status = "failed".into();
    r.stage = "build".into();
    r.quadruple = None;
    r.samples.clear();
    r.summary.clear();
    let states = evidence::stage_states(&r);
    assert_eq!(states["resolve"], "succeeded");
    assert_eq!(states["build"], "failed");
    assert_eq!(states["measure"], "not_executed");
    r.status = "prepared".into();
    r.stage = "prepared".into();
    let states = evidence::stage_states(&r);
    assert_eq!(states["probe"], "succeeded");
    assert_eq!(states["validate"], "not_executed");
}
