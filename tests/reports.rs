mod support;
use fountain_eval::findings::{Difference, FailureKind as Kind, Violation};
use fountain_eval::{report, validation};
use serde_json::{Value, json};

fn structured(
    case: &str,
    contract: &str,
    kind: Kind,
    context: Value,
    difference: Option<Difference>,
) -> Value {
    let requirement = if contract == "operator.mul_add.zero" {
        "Multiply-add with scalar 0 must preserve the destination. The destination must stay unchanged.".into()
    } else {
        format!("The explicit {contract} contract must hold.")
    };
    let mut base = json!({"case":case});
    base.as_object_mut()
        .unwrap()
        .extend(context.as_object().unwrap().clone());
    json!(Violation {
        contract_id: contract.into(),
        failure_kind: kind,
        requirement,
        context: base,
        difference
    })
}
fn delta(
    location: &str,
    left: Value,
    right: Value,
    labels: (&str, &str),
    lengths: (Option<usize>, Option<usize>),
) -> Option<Difference> {
    Some(Difference {
        location: location.into(),
        left_label: labels.0.into(),
        right_label: labels.1.into(),
        left,
        right,
        left_length: lengths.0,
        right_length: lengths.1,
    })
}
fn attach(
    r: &mut fountain_eval::evidence::Record,
    contract: &str,
    kind: Kind,
    context: Value,
    difference: Option<Difference>,
) {
    let f = r.correctness.fatal_failures.last_mut().unwrap();
    f["finding"] = structured(
        f["case"].as_str().unwrap(),
        contract,
        kind,
        context,
        difference,
    );
}

#[test]
fn report_is_self_contained_grouped_and_bounds_every_table() {
    let mut r = support::record("native-2", "rq");
    r.correctness = validation::report(&r);
    let md = report::markdown(&r);
    for section in [
        "## Summary",
        "### Fatal",
        "### Warning",
        "## Validation tests (V)",
        "## Operation tests (O)",
        "## Link tests (L)",
        "## Workflow tests (W)",
    ] {
        assert!(md.contains(section), "{section}");
    }
    assert!(!md.contains("## Fatal and warning findings"));
    let order = [
        "## Summary",
        "## Validation tests (V)",
        "### Summary",
        "### Fatal",
        "### Warning",
        "### Detailed results",
        "## Operation tests (O)",
        "## Link tests (L)",
        "## Workflow tests (W)",
    ]
    .map(|s| md.find(s).unwrap());
    assert!(order.windows(2).all(|w| w[0] < w[1]));
    assert!(
        md.contains("| 512 | Build plan |")
            && md.contains("| O04 xor one |")
            && md.contains("source-equivalent Gbit/s")
            && md.contains("V11 · Errors and lifecycle")
            && md.contains("L12 · Decode replay")
            && md.contains("W08 · Component lifecycle")
    );
    assert!(md.lines().count() < 360 && !md.contains("<details>"));
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
fn summary_contains_config_environment_versions_and_results_before_findings() {
    let mut r = support::record("native-2", "rq");
    r.correctness = validation::report(&r);
    let md = report::markdown(&r);
    let first = md.split("## Validation tests (V)").next().unwrap();
    for value in [
        "### Test configuration",
        "Scheme",
        "RQ",
        "0.1.0",
        "### Test environment",
        "synthetic-test-fixture",
        "### Test results",
        "PASS",
        "native-2@0.1.0",
    ] {
        assert!(first.contains(value), "{value}");
    }
    for label in [
        "| Sampling |",
        "| First-use sampling |",
        "| Environment identity |",
        "| Test content identity |",
    ] {
        assert!(!md.contains(label), "{label}");
    }
    for label in [
        "V check coverage",
        "Performance test coverage",
        "Exclusion reasons",
    ] {
        assert!(
            !first.contains(label),
            "Detailed coverage appeared in the summary"
        );
        let validation = md
            .split("## Validation tests (V)")
            .nth(1)
            .unwrap()
            .split("## Operation tests (O)")
            .next()
            .unwrap();
        assert!(validation.contains(label), "{label}");
    }
    let findings = md
        .split("## Validation tests (V)")
        .nth(1)
        .unwrap()
        .split("### Detailed results")
        .next()
        .unwrap();
    assert!(findings.contains("### Fatal\n\nNone detected."));
    assert!(findings.contains("### Warning\n\nNone detected."));
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
    attach(
        &mut r,
        "operator.mul_add.zero",
        Kind::ValueMismatch,
        json!({"operation":{"op":"mul_add","src":0,"scalar":0,"dst":8}}),
        delta(
            "Symbol 8, byte 0",
            json!(255),
            json!(0),
            ("Expected", "Observed"),
            (Some(1), Some(1)),
        ),
    );
    let md = report::markdown(&r);
    assert!(md.contains("destination must stay unchanged"));
    assert!(md.contains("| Symbol 8, byte 0 | 255 | 0 |"));
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
    attach(
        &mut r,
        "dispatch.equivalence",
        Kind::PathMismatch,
        json!({}),
        delta(
            "Symbol 8, byte 0",
            json!(255),
            json!(0),
            ("Direct", "Execute"),
            (Some(1), Some(1)),
        ),
    );
    let md = report::markdown(&r);
    assert!(md.contains("| First difference | Direct | Execute |"));
    assert!(md.contains("| Symbol 8, byte 0 | 255 | 0 |"));
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
    let checks = plan
        .checks
        .values()
        .filter(|c| c.skip.is_none() && c.spec.validation_family().is_some())
        .count();
    let skipped = plan
        .checks
        .values()
        .filter(|c| c.measured && (c.skip.is_some() || c.measurement_skip.is_some()))
        .count();
    r.samples
        .retain(|s| s.case == plan.cases[0] || (s.case == plan.cases[1] && s.repetition == 0));
    let md = report::markdown(&r);
    let pending = total - 2;
    assert!(md.contains(&format!("| C0 | {total} | 0 | {pending} | {skipped} |")));
    assert!(md.contains("| C0 | 1 | 1 |"));
    assert!(md.contains(&format!("| C0 | {checks} | {checks} | 0 |")));
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
    assert!(md.contains("| C0 | Plan unavailable | — | — | — |"));
    assert!(!md.contains("Coverage by test family"));
}

#[test]
fn performance_exclusions_omit_checks_that_have_no_timing() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.stage = "validate".into();
    r.samples.clear();
    let md = report::markdown(&r);
    let coverage = md
        .split("## Validation tests (V)")
        .nth(1)
        .unwrap()
        .split("## Operation tests (O)")
        .next()
        .unwrap();
    assert!(coverage.contains("| C0 | 1416 | 0 | 1416 | 264 |"));
    assert!(coverage.contains("| C0 | 1074 | 1074 | 0 | 45 |"));
    assert!(coverage.contains("RQ is selected. R10 cases are excluded. | 45 | 216 |"));
    assert!(coverage.contains("no alias-operation entry point. | 0 | 48 |"));
    assert!(!coverage.contains("Incomplete sample sets"));
    assert!(!coverage.contains("Partial / rejected"));
    assert!(!coverage.contains("fused work"));
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
    for w in &mut r.correctness.warnings {
        let c = &r.recipe.as_ref().unwrap().checks[w["case"].as_str().unwrap()].spec;
        w["finding"] = json!(fountain_eval::findings::boundary_violation(
            c,
            &w["expected"],
            &w["actual"]
        ));
        for variant in w["measurement_summary"]["variants"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            for outcome in variant["actual"]["outcomes"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                outcome["finding"] = json!(fountain_eval::findings::boundary_violation(
                    c,
                    &json!([0]),
                    &outcome["actual"]
                ));
            }
        }
    }
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
    for w in &mut r.correctness.warnings {
        let c = &r.recipe.as_ref().unwrap().checks[w["case"].as_str().unwrap()].spec;
        w["finding"] = json!(fountain_eval::findings::boundary_violation(
            c,
            &w["expected"],
            &w["actual"]
        ));
        for variant in w["measurement_summary"]["variants"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            for outcome in variant["actual"]["outcomes"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                outcome["finding"] = json!(fountain_eval::findings::boundary_violation(
                    c,
                    &json!([0]),
                    &outcome["actual"]
                ));
            }
        }
    }
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
    attach(
        &mut r,
        "operator.copy",
        Kind::ValueMismatch,
        json!({"step":73,"total_steps":144,"operation":{"op":"copy","src":0,"dst":16}}),
        delta(
            "Symbol 0, byte 0",
            json!(7),
            json!(6),
            ("Expected", "Observed"),
            (Some(31), Some(31)),
        ),
    );
    let md = report::markdown(&r);
    assert!(md.contains("Step 73/144") && md.contains("| Symbol 0, byte 0 | 7 | 6 |"));
    assert!(!md.contains("Trace SHA-256") && !md.contains("old operation;"));
    assert!(md.contains("Full diagnostic and replay context"));
}

#[test]
fn mixed_sequence_failure_leads_with_zero_scalar_contract_and_exact_bytes() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/mixed-sequence/seeded/b1/s20488/direct",
        "message":"V04 seeded: width=1, seed=20488, route=direct, step=27/256, dispatch=direct, input_seed=20515, action=MulAdd { src: 11, scalar: 0, dst: 14 }\nTrace SHA-256: trace-hash; pre-state SHA-256: before-hash.\nPrevious steps: replay\nFirst difference: symbol 14, byte 0; expected 130; actual 0; expected length 1; actual length 1."
    }));
    attach(
        &mut r,
        "operator.mul_add.zero",
        Kind::ValueMismatch,
        json!({"step":27,"total_steps":256,"dispatch":"direct","input_seed":20515,"operation":{"op":"mul_add","src":11,"scalar":0,"dst":14}}),
        delta(
            "Symbol 14, byte 0",
            json!(130),
            json!(0),
            ("Expected", "Observed"),
            (Some(1), Some(1)),
        ),
    );
    let md = report::markdown(&r);
    let finding = md.find("**Fatal**").unwrap();
    let summary = md.find("Multiply-add with scalar 0").unwrap();
    let difference = md.find("| Symbol 14, byte 0 | 130 | 0 |").unwrap();
    let context = md.find("| Operation | MulAdd").unwrap();
    assert!(finding < summary && summary < difference && difference < context);
    assert!(md.contains("Step 27/256") && md.contains("B=1 (bytes/symbol) · direct"));
    assert!(md.contains("20488 · step input seed 20515"));
    assert!(!md.contains("trace-hash") && !md.contains("before-hash"));
}

#[test]
fn duplicate_input_failure_distinguishes_output_offset_from_last_input_symbol() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/receive-contract/rq/s20488",
        "message":"correctness/receive-contract/rq/s20488: V10 missing-with-duplicates input 66, symbol 147; byte 0, expected Some(140), actual Some(190); lengths 8192 and 8192"
    }));
    attach(
        &mut r,
        "codec.recovered-payload",
        Kind::ValueMismatch,
        json!({"scenario":"missing-with-duplicates","input_count":66,"input_symbol":147}),
        delta(
            "Output byte 0",
            json!(140),
            json!(190),
            ("Expected", "Observed"),
            (Some(8192), Some(8192)),
        ),
    );
    let md = report::markdown(&r);
    assert!(md.contains("codec.recovered-payload") && md.contains("missing with duplicates"));
    assert!(md.contains("| Output byte 0 | 140 | 190 |"));
    assert!(md.contains("After 66 received symbols · last input symbol ID 147"));
    assert!(!md.contains("Some(") && !md.contains("Symbol 147, byte 0"));
}

#[test]
fn fatal_output_lengths_and_missing_bytes_are_explicit() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.correctness.fatal_failures.push(json!({
        "case":"correctness/receive-contract/rq/s20488",
        "message":"correctness/receive-contract/rq/s20488: V10 missing-with-duplicates input 66, symbol 147; byte 8191, expected Some(140), actual None; lengths 8192 and 8191"
    }));
    attach(
        &mut r,
        "codec.recovered-payload",
        Kind::LengthMismatch,
        json!({"scenario":"missing-with-duplicates","input_count":66,"input_symbol":147}),
        delta(
            "Output byte 8191",
            json!(140),
            Value::Null,
            ("Expected", "Observed"),
            (Some(8192), Some(8191)),
        ),
    );
    let md = report::markdown(&r);
    assert!(md.contains("| Output byte 8191 | 140 | missing |"));
    assert!(md.contains("| Output length (bytes) | 8192 | 8191 |"));
}

#[test]
fn repeated_fatals_form_one_problem_before_the_complete_family_tables() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    r.stage = "validate".into();
    r.samples.clear();
    let ids: Vec<_> = r
        .recipe
        .as_ref()
        .unwrap()
        .checks
        .values()
        .filter(|c| c.skip.is_none() && c.spec.validation_family() == Some("V04"))
        .take(25)
        .map(|c| c.spec.id.clone())
        .collect();
    r.correctness = validation::report(&r);
    r.correctness.fatal_failures = ids.iter().map(|id| json!({"case":id,"severity":"fatal","status":"failed",
        "message":"V04 seeded: width=1, seed=20488, route=direct, step=27/256, action=MulAdd { src: 11, scalar: 0, dst: 14 }\nFirst difference: symbol 14, byte 0; expected 130; actual 0; expected length 1; actual length 1."})).collect();
    for f in &mut r.correctness.fatal_failures {
        f["finding"] = structured(
            f["case"].as_str().unwrap(),
            "operator.mul_add.zero",
            Kind::ValueMismatch,
            json!({"step":27,"total_steps":256}),
            delta(
                "Symbol 14, byte 0",
                json!(130),
                json!(0),
                ("Expected", "Observed"),
                (Some(1), Some(1)),
            ),
        );
    }
    let md = report::markdown(&r);
    let findings = md
        .split("\n### Fatal\n")
        .nth(1)
        .unwrap()
        .split("\n### Warning\n")
        .next()
        .unwrap();
    assert!(findings.contains("| C0 | V04 | 25 |"));
    assert_eq!(findings.matches("**Fatal**").count(), 1);
    assert!(findings.contains(&ids[0]));
    for id in ids.iter().skip(1) {
        assert!(!findings.contains(id), "Repeated case expanded: {id}");
    }
    assert!(findings.contains("Step 27/256"));
    let detail = md.split("### Detailed results").nth(1).unwrap();
    for number in 1..=11 {
        assert!(detail.contains(&format!("V{number:02} ·")));
    }
    assert!(md.lines().count() < 240);
}

#[test]
fn one_problem_can_span_components_and_validation_families() {
    use fountain_eval::{artifact, batch, config::Configuration};
    let mut left = support::record("native-2", "rq");
    let mut right = left.clone();
    let configuration = Configuration::parse(
        "/fixture/fountain-eval.toml".into(),
        format!("{}\nengine1 = \"registry:fountain_engine@2.1.0\"\noperator1 = \"registry:fountain_operators@2.0.0#SimdDataOperator\"\n", left.configuration.original),
    ).unwrap();
    for (index, record) in [&mut left, &mut right].into_iter().enumerate() {
        record.configuration = configuration.selection(&configuration.selections[index]);
        record.request = record.configuration.request.clone();
        record.status = "failed".into();
        record.stage = "validate".into();
        record.samples.clear();
        record.summary.clear();
        record.diagnostics.clear();
        for event in &mut record.validation {
            let case = event["case"].as_str().unwrap().to_owned();
            if case == "correctness/operator-reference/b1"
                || case == "correctness/mixed-sequence/seeded/b1/s6513/direct"
            {
                event["status"] = json!("failed");
                event["message"] = json!(
                    "Different wording may be used here without changing the structured identity."
                );
                event["finding"] = structured(
                    &case,
                    "operator.mul_add.zero",
                    Kind::ValueMismatch,
                    json!({}),
                    delta(
                        "Symbol 8, byte 0",
                        json!(255),
                        json!(0),
                        ("Expected", "Observed"),
                        (Some(1), Some(1)),
                    ),
                );
            }
            if case == "operation/self-xor/b128/direct/small" {
                event["status"] = json!("failed");
                event["actual"] = json!([1]);
                event["witness"] = json!(fountain_eval::util::digest(&json!([1])));
            }
        }
        support::attach_warning_findings(record);
        record.correctness = validation::report(record);
        assert_eq!(record.correctness.fatal_failures.len(), 2);
    }
    let components = [&left, &right]
        .into_iter()
        .map(artifact::Component::from_record)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let entries = [&left, &right]
        .into_iter()
        .map(|r| batch::Entry {
            component_id: r.configuration.component_id,
            request: r.request.clone(),
            status: r.status.clone(),
            directory: None,
            elapsed_ns: Some(1),
            phases_ns: Default::default(),
            validation_observations: r.validation.len(),
            warnings: r.correctness.warnings.len(),
            samples: 0,
            diagnostics: 0,
            skipped: 0,
            error: None,
        })
        .collect();
    let saved = artifact::Snapshot {
        batch: batch::Record {
            format_version: 1,
            run_id: "fixture-batch".into(),
            action: "run".into(),
            status: "completed_with_failures".into(),
            interrupted: false,
            started_unix_ms: 0,
            elapsed_ns: 1,
            configuration,
            entries,
        },
        components,
        source_seal: "fixture".into(),
        prepared_from: None,
    };
    let md = report::snapshot(&saved);
    let findings = md
        .split("\n### Fatal\n")
        .nth(1)
        .unwrap()
        .split("### Detailed results")
        .next()
        .unwrap();
    assert_eq!(findings.matches("**Fatal**").count(), 1);
    assert!(findings.contains("| C0 | V02, V04 | 2 |"));
    assert!(findings.contains("| C1 | V02, V04 | 2 |"));
    assert_eq!(
        findings
            .matches("#### Warning 1 · Boundary reference deviation")
            .count(),
        1
    );
    assert!(findings.contains("| C0 | 1 | 1 case · Expected"));
    assert!(findings.contains("| C1 | 1 | 1 case · Expected"));
    assert!(md.contains("V11 · Errors and lifecycle"));
}

#[test]
fn different_contract_failures_are_not_merged_by_family_alone() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    for message in [
        "assertion `left == right` failed: V05 direct versus execute\n  left: {8: [255]}\n right: {8: [0]}",
        "assertion `left == right` failed: V05 batch versus elementary direct calls\n  left: {8: [255]}\n right: {8: [0]}",
        "A separate undocumented panic",
    ] {
        r.correctness
            .fatal_failures
            .push(json!({"case":"correctness/dispatch-equivalence/b1", "message":message}));
    }
    let contracts = [
        (
            "dispatch.equivalence",
            Kind::PathMismatch,
            Some(("Direct", "Execute")),
        ),
        (
            "composition.equivalence",
            Kind::PathMismatch,
            Some(("Batch", "Elementary calls")),
        ),
        ("case.execution", Kind::UnexpectedPanic, None),
    ];
    for (f, (contract, kind, labels)) in r.correctness.fatal_failures.iter_mut().zip(contracts) {
        f["finding"] = structured(
            f["case"].as_str().unwrap(),
            contract,
            kind,
            json!({}),
            labels.and_then(|labels| {
                delta(
                    "Symbol 8, byte 0",
                    json!(255),
                    json!(0),
                    labels,
                    (Some(1), Some(1)),
                )
            }),
        );
    }
    let md = report::markdown(&r);
    assert_eq!(md.matches("**Fatal**").count(), 3);
    assert!(md.contains("| First difference | Direct | Execute |"));
    assert!(md.contains("| First difference | Batch | Elementary calls |"));
    assert!(md.contains("A separate undocumented panic"));
}

#[test]
fn grouping_uses_only_explicit_identity_and_not_message_words() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    for (case, message, kind) in [
        (
            "correctness/operator-reference/b1",
            "completely unrelated wording",
            Kind::ValueMismatch,
        ),
        (
            "correctness/operator-reference/b15",
            "Vector with ID 999 does not exist",
            Kind::ValueMismatch,
        ),
        (
            "correctness/operator-reference/b16",
            "completely unrelated wording",
            Kind::UnexpectedPanic,
        ),
    ] {
        r.correctness
            .fatal_failures
            .push(json!({"case":case,"message":message,
            "finding":structured(case,"operator.explicit-contract",kind,json!({}),None)}));
    }
    let md = report::markdown(&r);
    assert_eq!(md.matches("**Fatal**").count(), 2);
    assert!(md.contains("| C0 | V02 | 2 |"));
    assert!(md.contains("| C0 | V02 | 1 |"));
    assert!(!md.contains("A required symbol was missing"));
}

#[test]
fn legacy_messages_are_unclassified_and_never_used_as_grouping_keys() {
    let mut r = support::record("native-2", "rq");
    r.status = "failed".into();
    for case in [
        "correctness/operator-reference/b1",
        "correctness/operator-reference/b15",
    ] {
        r.correctness
            .fatal_failures
            .push(json!({"case":case,"message":"Identical MulAdd scalar: 0 diagnostic"}));
    }
    let md = report::markdown(&r);
    assert_eq!(md.matches("**Unclassified diagnostic**").count(), 2);
    assert!(!md.contains("operator.mul_add.zero"));
}

#[test]
fn one_boundary_with_different_violation_kinds_is_reported_separately() {
    let mut r = support::record("native-2", "rq");
    let c = &r.recipe.as_ref().unwrap().checks["operation/self-xor/b128/direct/small"].spec;
    let outcomes = [json!([1]), json!({"status":"rejected_by_panic"})];
    let finding =
        fountain_eval::findings::boundary_violation(c, &json!([0]), &outcomes[0]).unwrap();
    let additional =
        fountain_eval::findings::boundary_violation(c, &json!([0]), &outcomes[1]).unwrap();
    r.correctness.warnings.push(json!({"case":c.id,"expected":[0],"actual":outcomes[0],"finding":finding,"findings":[finding,additional]}));
    let md = report::markdown(&r);
    assert_eq!(md.matches("#### Warning ").count(), 2);
    assert!(md.contains("Call rejected") && md.contains("Boundary reference deviation"));
}
