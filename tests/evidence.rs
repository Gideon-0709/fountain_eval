mod support;
use fountain_eval::{evidence, execution, measurement, util};
use serde_json::json;
use support::{Temp, record};
#[test]
fn metrics_are_decimal_gigabits_and_operations_per_second() {
    let codec = record("native-2", "rq");
    assert_eq!(
        codec
            .summary
            .iter()
            .find(|s| s.case.contains("encode-chain/rq/k64/b128"))
            .unwrap()
            .source_payload_gbps,
        Some(64.0 * 128.0 * 8.0 / 3000.0)
    );
    assert_eq!(codec.summary[0].median_ns, 3000);
    let op = record("native-2", "rq");
    let row = op
        .summary
        .iter()
        .find(|s| s.case == "operation/xor-one/b16/direct/small")
        .unwrap();
    assert_eq!(row.operations_per_second, Some(64.0 * 1e9 / 3000.0));
    assert_eq!(row.source_payload_gbps, None);
    let mut bad = codec.samples;
    bad[0].elapsed_ns = 0;
    assert!(measurement::summarize(&bad).is_err());
}
#[test]
fn incomplete_duplicate_corrupt_and_wrong_work_samples_fail() {
    let good = record("native-2", "rq");
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
    let left = record("native-2", "rq");
    let mut right = left.clone();
    right.run_id = "another-run".into();
    right.quadruple.as_mut().unwrap().e.observations = json!({"time":99});
    assert!(evidence::compare(&left, &right).is_ok());
    right = record("product-abi", "rq");
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
    let record = record("native-2", "rq");
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
    let record = record("native-2", "rq");
    let events = vec![
        json!({"event":"hello","protocol":1,"build":built.key,"interface":built.interface.id,"test":"0.1.0","operator":"slab","family":"native-2","scheme":"rq"}),
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
    let mut r = record("native-2", "rq");
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

#[test]
fn semantic_warnings_preserve_success_measurements_and_comparison() {
    let left = record("native-2", "rq");
    let mut right = left.clone();
    let case = "operation/self-xor/b16/direct/small";
    let event = right
        .validation
        .iter_mut()
        .find(|v| v["case"] == case)
        .unwrap();
    event["actual"] = json!([17]);
    event["witness"] = json!(util::digest(&event["actual"]));
    event["status"] = json!("failed");
    for sample in right.samples.iter_mut().filter(|s| s.case == case) {
        sample.observation = Some(json!({"calls":64,"outcomes":[{"count":64,"actual":[17]}]}));
        sample.witness = util::digest(sample.observation.as_ref().unwrap());
        sample.elapsed_ns /= 2;
    }
    evidence::validate_samples(&right).unwrap();
    let report = fountain_eval::validation::report(&right);
    assert_eq!(report.status, "passed_with_warnings");
    assert_eq!(report.warnings.len(), 1);
    assert!(report.fatal_failures.is_empty());
    let comparison = evidence::compare(&left, &right).unwrap();
    assert_eq!(comparison["status"], "comparable");
    let row = comparison["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == case)
        .unwrap();
    assert_eq!(row["left_over_right_time"], 2.0);
    assert_eq!(row["right_warnings"].as_array().unwrap().len(), 1);
    assert_eq!(
        row["right_operations_per_second"].as_f64().unwrap(),
        64.0 * 1e9 / 1500.0
    );
}

#[test]
fn measured_boundary_variation_is_reported_without_becoming_fatal() {
    let mut r = record("native-2", "rq");
    let sample = r
        .samples
        .iter_mut()
        .find(|s| s.case == "operation/self-xor/b16/direct/small")
        .unwrap();
    sample.observation =
        Some(json!({"calls":64,"outcomes":[{"count":1,"actual":[99]},{"count":63,"actual":[0]}]}));
    sample.witness = util::digest(sample.observation.as_ref().unwrap());
    evidence::validate_samples(&r).unwrap();
    let report = fountain_eval::validation::report(&r);
    assert_eq!(report.status, "passed_with_warnings");
    assert_eq!(report.warnings.len(), 1);
    assert!(evidence::compare(&r, &r).is_ok());
}

#[test]
fn fatal_failures_cannot_be_downgraded_and_warning_coverage_is_required() {
    let good = record("native-2", "rq");
    for mutation in 0..4 {
        let mut bad = good.clone();
        match mutation {
            0 => bad.validation[0]["status"] = json!("failed"),
            1 => bad.validation[0]["severity"] = json!("warning"),
            2 => {
                bad.validation.pop();
            }
            _ => {
                let warning = bad
                    .validation
                    .iter_mut()
                    .find(|v| v["severity"] == "warning")
                    .unwrap();
                warning["actual"] = json!([99]); // Unreported, incoherent observation.
            }
        }
        assert!(
            evidence::validate_samples(&bad).is_err(),
            "mutation {mutation}"
        );
        assert!(evidence::compare(&good, &bad).is_err());
    }
}

#[test]
fn different_internal_candidate_digests_are_not_comparable() {
    let left = record("native-2", "rq");
    let mut earlier = left.clone();
    earlier.quadruple.as_mut().unwrap().t.digest = "earlier-draft".into();
    assert_eq!(
        left.quadruple.as_ref().unwrap().t.id,
        earlier.quadruple.as_ref().unwrap().t.id
    );
    assert!(evidence::compare(&earlier, &left).is_err());
}

#[test]
fn boundary_batches_account_for_every_actual_call() {
    let good = record("native-2", "rq");
    for mutation in 0..4 {
        let mut bad = good.clone();
        let sample = bad
            .samples
            .iter_mut()
            .find(|s| s.observation.is_some())
            .unwrap();
        let observed = sample.observation.as_mut().unwrap();
        match mutation {
            0 => observed["calls"] = json!(63),
            1 => observed["outcomes"][0]["count"] = json!(63),
            2 => observed["outcomes"] = json!([]),
            _ => {
                observed["outcomes"] = json!([{"count":32,"actual":[0]},{"count":32,"actual":[0]}])
            }
        }
        sample.witness = util::digest(observed);
        assert!(
            evidence::validate_samples(&bad).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn diagnostics_must_be_complete_separate_and_additive() {
    let good = record("native-2", "rq");
    evidence::validate_diagnostics(&good).unwrap();
    for mutation in 0..5 {
        let mut bad = good.clone();
        match mutation {
            0 => {
                bad.diagnostics.pop();
            }
            1 => bad.diagnostics.push(bad.diagnostics[0].clone()),
            2 => bad.diagnostics[0]["observation"]["stages"]["total_ns"] = json!(9),
            3 => bad.diagnostics[0]["worker"] = json!(99),
            _ => bad.diagnostics[0]["resources"] = json!({}),
        }
        assert!(evidence::validate_diagnostics(&bad).is_err());
    }
}

#[test]
fn incomplete_recovery_has_no_successful_throughput() {
    let mut r = record("native-2", "rq");
    let case = r
        .samples
        .iter()
        .find(|s| s.case.starts_with("workflow/decode/"))
        .unwrap()
        .case
        .clone();
    for sample in r.samples.iter_mut().filter(|s| s.case == case) {
        sample.outcome = "incomplete_recovery".into();
    }
    evidence::validate_samples(&r).unwrap();
    let summaries = measurement::summarize(&r.samples).unwrap();
    let row = summaries.iter().find(|s| s.case == case).unwrap();
    assert_eq!(row.incomplete, 15);
    assert_eq!(row.source_payload_gbps, None);
}

#[test]
fn sample_worker_identity_and_report_escaping_are_checked() {
    let mut r = record("native-2", "rq");
    r.samples[0].worker = 99;
    assert!(evidence::validate_samples(&r).is_err());
    r.run_id = "<script>alert(1)</script>".into();
    let markdown = fountain_eval::report::markdown(&r);
    assert!(markdown.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!markdown.contains("<script>alert(1)</script>"));
}

#[test]
fn fatal_failure_details_are_visible_in_markdown() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.samples.clear();
    r.summary.clear();
    r.correctness.fatal_failures.push(serde_json::json!({"case":"correctness/arithmetic", "severity":"fatal", "status":"failed", "message":"MulAdd scalar: 0 <invalid>"}));
    let markdown = fountain_eval::report::markdown(&r);
    assert!(markdown.contains("**Fatal**"));
    assert!(markdown.contains("MulAdd scalar: 0 &lt;invalid&gt;"));
    assert!(!markdown.contains("```json"));
}
