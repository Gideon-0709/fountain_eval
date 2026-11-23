mod support;
use fountain_eval::findings::{self, Contract, Failure, FailureKind as Kind};
use serde_json::json;

#[test]
fn typed_contracts_survive_nested_scopes_with_explicit_context() {
    let error = std::panic::catch_unwind(|| {
        findings::scope(
            Contract::new("outer.contract", "Outer requirement"),
            json!({"case":"fixture","seed":7}),
            || {
                findings::scope(
                    Contract::new("inner.contract", "Inner requirement"),
                    json!({"step":3}),
                    || {
                        findings::require(
                            Contract::new("inner.bytes", "Bytes must agree"),
                            Kind::ValueMismatch,
                            false,
                            || "Any diagnostic words".into(),
                        );
                    },
                );
            },
        )
    })
    .unwrap_err()
    .downcast::<Failure>()
    .unwrap();
    assert_eq!(error.violation.contract_id, "inner.bytes");
    assert_eq!(error.violation.failure_kind, Kind::ValueMismatch);
    assert_eq!(error.violation.context["seed"], 7);
    assert_eq!(error.violation.context["step"], 3);
    assert!(error.violation.validate("fixture"));
    // A subsequent scope must not inherit a failed check's context.
    let fresh = std::panic::catch_unwind(|| {
        findings::scope(
            Contract::new("new.contract", "New requirement"),
            json!({"case":"fresh"}),
            || panic!("ordinary panic"),
        )
    })
    .unwrap_err()
    .downcast::<Failure>()
    .unwrap();
    assert!(fresh.violation.context.get("step").is_none());
    assert_eq!(fresh.violation.failure_kind, Kind::UnexpectedPanic);
    assert_eq!(fresh.violation.contract_id, "new.contract");
}

#[test]
fn boundary_kinds_are_emitted_from_structured_outcomes_and_checked() {
    let r = support::record("native-2", "rq");
    let c = &r.recipe.as_ref().unwrap().checks["operation/self-xor/b128/direct/small"].spec;
    for (actual, kind) in [
        (json!([1]), Kind::ReferenceDeviation),
        (json!({"status":"rejected_by_panic"}), Kind::CallRejected),
        (
            json!({"status":"output_unreadable_after_boundary_call"}),
            Kind::OutputUnreadable,
        ),
    ] {
        let v = findings::boundary_violation(c, &json!([0]), &actual).unwrap();
        assert_eq!(v.contract_id, "boundary.self-xor.reference");
        assert_eq!(v.failure_kind, kind);
        assert!(findings::verify_boundary(
            c,
            &json!([0]),
            &actual,
            &json!(v)
        ));
        let mut corrupt = v.clone();
        corrupt.contract_id = "different.contract".into();
        assert!(!findings::verify_boundary(
            c,
            &json!([0]),
            &actual,
            &json!(corrupt)
        ));
    }
    assert!(findings::boundary_violation(c, &json!([0]), &json!([0])).is_none());
}

#[test]
fn exact_byte_differences_are_recorded_without_parsing_assertion_text() {
    let failure = std::panic::catch_unwind(|| {
        findings::scope(
            Contract::new("fixture.bytes", "Bytes must agree"),
            json!({"case":"fixture"}),
            || {
                findings::bytes(
                    Contract::new("fixture.bytes", "Bytes must agree"),
                    &[1, 2, 3],
                    &[1, 9],
                    || "Message contains no byte positions".into(),
                );
            },
        )
    })
    .unwrap_err()
    .downcast::<Failure>()
    .unwrap();
    let d = failure.violation.difference.unwrap();
    assert_eq!(failure.violation.failure_kind, Kind::LengthMismatch);
    assert_eq!(d.location, "Output byte 1");
    assert_eq!((d.left, d.right), (json!(2), json!(9)));
    assert_eq!((d.left_length, d.right_length), (Some(3), Some(2)));
}

#[test]
fn failed_validation_rejects_missing_malformed_and_wrong_case_identities() {
    let mut r = support::record("native-2", "rq");
    r.validation[0]["status"] = json!("failed");
    assert!(
        fountain_eval::validation::verify(r.recipe.as_ref().unwrap(), &r.validation)
            .unwrap_err()
            .contains("structured")
    );
    r.validation[0]["finding"] =
        support::violation("wrong-case", "fixture.contract", Kind::ValueMismatch);
    assert!(
        fountain_eval::validation::verify(r.recipe.as_ref().unwrap(), &r.validation)
            .unwrap_err()
            .contains("invalid structured")
    );
    r.validation[0]["finding"]["failure_kind"] = json!("unknown_kind");
    assert!(
        fountain_eval::validation::verify(r.recipe.as_ref().unwrap(), &r.validation)
            .unwrap_err()
            .contains("structured")
    );
}
