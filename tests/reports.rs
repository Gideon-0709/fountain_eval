mod support;
use fountain_eval::{report, validation};
use serde_json::json;

#[test]
fn report_is_self_contained_grouped_and_bounds_every_table() {
    let mut r = support::record("native-2", "rq");
    r.correctness = validation::report(&r);
    let md = report::markdown(&r);
    for section in [
        "## Validation and warnings (V)",
        "## Operation tests (O)",
        "## Link tests (L)",
        "## Workflow tests (W)",
        "## Coverage and measurement context",
    ] {
        assert!(md.contains(section), "{section}");
    }
    let order = ["(V)", "(O)", "(L)", "(W)"].map(|s| md.find(s).unwrap());
    assert!(order.windows(2).all(|w| w[0] < w[1]));
    assert!(
        md.contains("| 512 | Build plan |")
            && md.contains("| O04 xor one |")
            && md.contains("source-equivalent Gbit/s")
            && md.contains("V11 · Errors and lifecycle")
            && md.contains("L12 · Decode replay")
            && md.contains("W08 · Component lifecycle")
    );
    assert!(md.lines().count() < 240 && !md.contains("<details>"));
    assert!(!md.contains("```json") && !md.contains("report.html"));
    let mut rows = 0;
    for line in md.lines() {
        if line.starts_with('|') {
            rows += 1;
            assert!(rows <= 14, "long table"); // header + separator + at most 12 data rows
            assert!(line.matches('|').count() <= 7, "wide table: {line}");
        } else {
            rows = 0;
        }
    }
}

#[test]
fn fatal_report_explains_defined_gf_contract_and_exact_difference() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.stage = "validate".into();
    r.summary.clear();
    r.samples.clear();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/dispatch-equivalence/b1", "phase":"validate", "rule":"execution-integrity",
        "message":"assertion `left == right` failed: correctness/dispatch-equivalence/b1 MulAdd { src: 0, scalar: 0, dst: 8 }\n  left: {0: [255], 8: [0]}\n right: {0: [255], 8: [255]}"
    }));
    let md = report::markdown(&r);
    assert!(md.contains("destination must stay unchanged"));
    assert!(md.contains("Symbol 8 expected [255], observed [0]"));
    assert!(md.contains("Not measured"));
    assert!(!md.contains("```json"));
}

#[test]
fn equivalence_failure_labels_both_paths_without_inventing_an_oracle() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/dispatch-equivalence/b1",
        "message":"assertion `left == right` failed: V05 direct versus execute: MulAdd { src: 0, scalar: 0, dst: 8 }\n  left: {8: [255]}\n right: {8: [0]}"
    }));
    let md = report::markdown(&r);
    assert!(md.contains("neither side is an independent scalar reference"));
    assert!(md.contains("Symbol 8, byte 0: direct = 255, execute = 0"));
    assert!(!md.contains("expected [0]") && !md.contains("observed [255]"));
}

#[test]
fn comparison_uses_category_sections_and_explains_warnings_without_json() {
    let r = support::record("native-2", "rq");
    let mut v = support::compare(&r, &r).unwrap();
    let boundary = v["rows"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["case"] == "operation/self-xor/b128/direct/small")
        .unwrap();
    boundary["right_warnings"] = json!([{"case": "operation/self-xor/b128/direct/small"}]);
    let md = report::comparison(&v);
    assert!(md.contains("## Operation tests"));
    assert!(md.contains("## Link tests"));
    assert!(md.contains("## Workflow tests"));
    assert!(md.contains("+0.00%"));
    assert!(md.contains("V06 · self xor · B=128 direct · warnings 0/1"));
    let round_trip = md
        .lines()
        .find(|line| line.starts_with("| W06 · round trip"))
        .unwrap();
    assert!(round_trip.contains(" ms |") && !round_trip.contains("Gbit/s"));
    assert!(!md.contains("```json"));
}

#[test]
fn failed_measurement_separates_partial_and_rejected_cases() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.stage = "measure".into();
    let plan = r.recipe.as_ref().unwrap();
    let total = plan.cases.len();
    let checks = plan.validation_cases().len();
    let skipped = plan
        .checks
        .values()
        .filter(|c| c.skip.is_some() || c.measurement_skip.is_some())
        .count();
    r.samples
        .retain(|s| s.case == plan.cases[0] || (s.case == plan.cases[1] && s.repetition == 0));
    let md = report::markdown(&r);
    let pending = total - 2;
    assert!(md.contains(&format!(
        "| C0 | {checks}/{checks} | 0/{total} | 1 / 1 | {pending} / {skipped} |"
    )));
    assert!(md.contains("Not measured"));
}

#[test]
fn unresolved_catalog_is_not_presented_as_an_applicable_plan() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.stage = "resolve".into();
    r.recipe = Some(fountain_eval::catalog::recipe(&r.request.test).unwrap());
    r.samples.clear();
    r.validation.clear();
    r.diagnostics.clear();
    let md = report::markdown(&r);
    assert!(md.contains("| C0 | Plan unavailable | Not measured |"));
    assert!(!md.contains("Coverage by test family"));
}

#[test]
fn stage_table_preserves_combined_interval_and_adds_to_diagnostic_total() {
    let mut r = support::record("native-2", "rq");
    for event in r
        .diagnostics
        .iter_mut()
        .filter(|event| event["case"] == "workflow/decode/rq/k512/b128/m52/source-first/s6513")
    {
        let worker = event["worker"].as_u64().unwrap();
        let mut intervals = json!({"ingestion-and-solve":6000});
        if worker != 0 {
            intervals["component-release"] = json!(3000);
        }
        event["observation"] = json!({
            "stages":{
                "total_ns":12000,
                "exclusive_ns":intervals,
                "unattributed_ns":if worker == 0 {6000} else {3000},
            },
            "link_coverage":{
                "L07":{"interval":"ingestion-and-solve","scope":"combined parent"},
                "L08":{"interval":"ingestion-and-solve","scope":"combined parent"},
                "L10":{"interval":"component-release","scope":"exclusive interval"},
            }
        });
    }
    let md = report::markdown(&r);
    assert!(md.contains("| ingestion and solve (L07, L08 · combined) | 6.000 | 50.0% |"));
    assert!(md.contains("| component release (L10) | 2.000 | 16.7% |"));
    assert!(md.contains("| Unattributed | 4.000 | 33.3% |"));
    assert!(md.contains("| Instrumented lifecycle total | 12.000 | 100.0% |"));
}

#[test]
fn warning_reports_distinct_timed_observations_and_actual_call_counts() {
    let mut r = support::record("native-2", "rq");
    r.correctness.warnings.push(json!({
        "case":"operation/self-xor/b128/direct/small",
        "expected":[0],
        "actual":[1],
        "measurement_summary":{
            "variants":[
                {"samples":3,"actual":{"calls":5,"outcomes":[{"actual":[0],"count":4},{"actual":[1],"count":1}]}},
                {"samples":2,"actual":{"calls":5,"outcomes":[{"actual":[2],"count":5}]}}
            ]
        }
    }));
    let md = report::markdown(&r);
    assert!(md.contains("12 calls · Matched the reference"));
    assert!(md.contains("10 calls · Expected &#91;0&#93;, observed &#91;2&#93;"));
    assert!(md.contains("3 calls with other observations"));
    assert!(md.contains("1 case · Expected &#91;0&#93;, observed &#91;1&#93;"));
    assert!(md.contains("B=128 direct small") && md.contains("ns/call"));
}

#[test]
fn unreadable_boundary_output_is_explained_without_dumping_the_state() {
    let mut r = support::record("native-2", "rq");
    let expected =
        json!({"sha256":"full-state-hash", "symbols":[{"id":1,"length":128,"prefix":[255,255]}]});
    r.correctness.warnings.push(json!({"case":"operation/overlapping-move/b128/direct/small", "expected":expected,"actual":{"status":"output_unreadable_after_boundary_call"},"details":{"dispatch":"direct","operations":[{"op":"move_range","src":0,"dst":1,"count":3}]},"measured_observations":[]}));
    let md = report::markdown(&r);
    assert!(md.contains(
        "Expected 1 readable output symbols; observed output unreadable after boundary call"
    ));
    assert!(!md.contains("full-state-hash"));
    assert!(md.contains("Not measured"));
    assert!(!md.contains("0/0 samples differ"));
    assert!(!md.contains("{\"symbols\""));
}

#[test]
fn history_failure_keeps_the_first_difference_visible_after_long_replay_context() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/mixed-sequence/copy-chain/b31/s6513/mixed",
        "message":format!("V04 copy-chain: width=31, seed=6513, route=mixed, step=73/144, dispatch=direct, action=Copy {{ src: 0, dst: 16 }}\nTrace SHA-256: trace; pre-state SHA-256: state.\nPrevious steps: {}\nFirst difference: symbol 0, byte 0; expected 7; actual 6; expected length 31; actual length 31.", "old operation; ".repeat(80))
    }));
    let md = report::markdown(&r);
    assert!(md.contains("step=73/144") && md.contains("First difference: symbol 0, byte 0"));
    assert!(md.contains("expected 7; actual 6") && md.contains("Trace SHA-256: trace"));
    assert!(md.contains("preceding operations") && !md.contains("old operation;"));
}
