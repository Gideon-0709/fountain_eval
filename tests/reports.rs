mod support;
use fountain_eval::{report, validation};
use serde_json::json;

#[test]
fn report_is_self_contained_grouped_and_bounds_every_table() {
    let mut r = support::record("native-2", "rq");
    r.correctness = validation::report(&r);
    let md = report::markdown(&r);
    for section in [
        "## 2. Correctness",
        "## 3. Individual",
        "## 4. Execution chains",
        "## 5. Integrated",
        "## 6. Coverage",
        "## 7. Execution",
    ] {
        assert!(md.contains(section), "{section}");
    }
    assert!(md.contains("chain/plan-build"));
    assert!(md.contains("operation/xor-one"));
    assert!(md.contains("Partition check: sum 4.0 ns; total 4.0 ns; residual 0.0 ns"));
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
    assert!(md.contains("| 8 | &#91;255&#93; | &#91;0&#93; |"));
    assert!(md.contains("symbol width 1 bytes"));
    assert!(md.contains("No accepted performance summary"));
    assert!(!md.contains("```json"));
}

#[test]
fn publishing_refuses_empty_reports_and_never_overwrites() {
    let t = support::Temp::new();
    let directory = report::reserve(&t.0, "test-run").unwrap();
    assert!(report::publish(&directory, "").is_err());
    assert!(!directory.join("report.md").exists());
    report::publish(&directory, "# Report\n\nComplete.\n").unwrap();
    assert!(report::publish(&directory, "replacement").is_err());
    assert!(report::reserve(&t.0, "test-run").is_err());
    assert!(report::reserve(&t.0, "../escape").is_err());
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
}

#[test]
fn comparison_uses_category_sections_and_explains_warnings_without_json() {
    let r = support::record("native-2", "rq");
    let v = fountain_eval::evidence::compare(&r, &r).unwrap();
    let md = report::comparison(&v);
    assert!(md.contains("## Individual operations"));
    assert!(md.contains("## Execution chains and encoding plans"));
    assert!(md.contains("## Integrated workflows"));
    assert!(md.contains("+0.00%"));
    assert!(!md.contains("```json"));
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
    assert!(!md.contains("{\"symbols\""));
}
