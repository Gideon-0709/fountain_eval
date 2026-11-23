mod support;
use fountain_eval::{evidence::Record, validation};
use serde_json::json;

fn failed_v() -> Record {
    let mut r = support::record("native-2", "rq");
    let plan = r.recipe.as_ref().unwrap();
    r.validation.retain(|event| {
        plan.checks[event["case"].as_str().unwrap()]
            .spec
            .validation_family()
            .is_some()
    });
    // Fail at both ends of V, with independent passing cases between them.
    for index in [0, r.validation.len() - 1] {
        r.validation[index]["status"] = json!("failed");
        r.validation[index]["message"] = json!("independent contract failure");
        r.validation[index]["finding"] = support::violation(
            r.validation[index]["case"].as_str().unwrap(),
            "fixture.independent",
            fountain_eval::findings::FailureKind::ValueMismatch,
        );
    }
    r.status = "failed".into();
    r.stage = "validate".into();
    r.samples.clear();
    r.diagnostics.clear();
    r.summary.clear();
    r
}

#[test]
fn validation_order_is_v01_through_v11_before_performance_prechecks() {
    let r = support::record("native-2", "rq");
    let plan = r.recipe.as_ref().unwrap();
    let cases = plan.validation_cases();
    let v: Vec<_> = cases
        .iter()
        .filter_map(|id| plan.checks[id].spec.validation_family())
        .collect();
    assert_eq!(v.first(), Some(&"V01"));
    assert_eq!(v.last(), Some(&"V11"));
    assert!(v.windows(2).all(|p| p[0] <= p[1]));
    assert!(
        cases[..v.len()]
            .iter()
            .all(|id| plan.checks[id].spec.validation_family().is_some())
    );
    assert!(
        cases[v.len()..]
            .iter()
            .all(|id| plan.checks[id].spec.validation_family().is_none())
    );
    assert!(cases[..v.len()].iter().any(|id| {
        let c = &plan.checks[id];
        c.spec.validation_family() == Some("V06") && c.measured && c.spec.category == "operation"
    }));
    validation::verify(plan, &r.validation).unwrap();
}

#[test]
fn complete_v_retains_all_fatal_findings_and_blocks_performance() {
    let r = failed_v();
    let error = validation::verify(r.recipe.as_ref().unwrap(), &r.validation).unwrap_err();
    assert!(
        error.starts_with("2 fatal correctness checks failed; all "),
        "{error}"
    );
    assert!(error.contains("O/L/W were not executed"));
    let report = validation::report(&r);
    assert_eq!(report.fatal_failures.len(), 2);
    assert_eq!(report.status, "failed");
    let coverage = fountain_eval::coverage::report(&r);
    let plan = r.recipe.as_ref().unwrap();
    for row in coverage["cases"].as_array().unwrap() {
        let c = &plan.checks[row["case"].as_str().unwrap()];
        if c.skip.is_none() {
            assert_eq!(
                row["validation"]["status"] == "not_executed",
                c.spec.validation_family().is_none()
            );
            if c.measured && c.measurement_skip.is_none() {
                assert_eq!(row["measurement"]["status"], "not_executed");
            }
        }
    }
    let md = fountain_eval::report::markdown(&r);
    let count = r.validation.len();
    assert!(md.contains(&format!("| C0 | {count} | {count} | 0 |")));
}

#[test]
fn missing_v_evidence_after_a_fatal_cannot_claim_v_completion() {
    let mut r = failed_v();
    r.validation.pop();
    let error = validation::verify(r.recipe.as_ref().unwrap(), &r.validation).unwrap_err();
    assert!(
        error.contains("incomplete or out-of-order V evidence"),
        "{error}"
    );
}

#[test]
fn performance_prechecks_after_v_fatal_are_rejected() {
    let mut r = failed_v();
    let good = support::record("native-2", "rq");
    r.validation.push(
        good.validation
            .iter()
            .find(|e| {
                good.recipe.as_ref().unwrap().checks[e["case"].as_str().unwrap()]
                    .spec
                    .validation_family()
                    .is_none()
            })
            .unwrap()
            .clone(),
    );
    assert!(
        validation::verify(r.recipe.as_ref().unwrap(), &r.validation)
            .unwrap_err()
            .contains("O/L/W executed after V fatal")
    );
}

#[test]
fn warning_case_scope_corruption_remains_fatal_and_other_v_cases_continue() {
    let mut r = failed_v();
    let warning = r
        .validation
        .iter_mut()
        .find(|e| e["severity"] == "warning")
        .unwrap();
    *warning = json!({"event":"validation","case":warning["case"],"severity":"fatal","rule":"execution-integrity","status":"failed","message":"protected symbol changed","witness":"failure-witness"});
    warning["finding"] = support::violation(
        warning["case"].as_str().unwrap(),
        "fixture.scope",
        fountain_eval::findings::FailureKind::ScopeCorruption,
    );
    let error = validation::verify(r.recipe.as_ref().unwrap(), &r.validation).unwrap_err();
    assert!(
        error.starts_with("3 fatal correctness checks failed; all "),
        "{error}"
    );
    assert_eq!(validation::report(&r).fatal_failures.len(), 3);
}

#[test]
fn warnings_alone_do_not_block_performance_but_family_reordering_is_rejected() {
    let mut r = support::record("native-2", "rq");
    let warning = r
        .validation
        .iter_mut()
        .find(|e| e["severity"] == "warning")
        .unwrap();
    warning["actual"] = json!([1]);
    warning["status"] = json!("failed");
    warning["witness"] = json!(fountain_eval::util::digest(&json!([1])));
    support::attach_warning_findings(&mut r);
    validation::verify(r.recipe.as_ref().unwrap(), &r.validation).unwrap();
    let last_v = r
        .validation
        .iter()
        .rposition(|e| {
            r.recipe.as_ref().unwrap().checks[e["case"].as_str().unwrap()]
                .spec
                .validation_family()
                .is_some()
        })
        .unwrap();
    r.validation.swap(0, last_v);
    assert!(
        validation::verify(r.recipe.as_ref().unwrap(), &r.validation)
            .unwrap_err()
            .contains("out-of-order")
    );
}
