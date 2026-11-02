mod support;
use fountain_eval::{
    catalog, config::Configuration, coverage, evidence, protocol::SourceReference,
};
use serde_json::json;
fn basic() -> String {
    "engine=\"path:../engine\"\noperator=\"path:../operator#slab\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n"
        .into()
}
fn parse(text: String) -> Result<Configuration, String> {
    Configuration::parse("/project/configs/eval.toml".into(), text)
}
#[test]
fn paths_defaults_and_scheme_source_are_explicit_and_frozen() {
    let config = parse(basic()).unwrap();
    assert_eq!(
        config.run.output,
        std::path::Path::new("/project/configs/runs")
    );
    assert_eq!(
        config.run.cache,
        std::path::Path::new("/project/configs/.fountain-eval")
    );
    assert_eq!(
        config.request.components.engine.source,
        SourceReference::Path {
            path: "/project/configs/../engine".into()
        }
    );
    let text = basic().replace(
        "scheme=\"rq\"",
        "scheme={name=\"r10\",source=\"path:../scheme\",mode=\"systematic\"}",
    );
    let config = parse(text.clone()).unwrap();
    assert_eq!(config.original, text);
    assert_eq!(config.request.scheme.name, "r10");
    assert_eq!(
        config.request.scheme.source.unwrap().source,
        SourceReference::Path {
            path: "/project/configs/../scheme".into()
        }
    );
}
#[test]
fn unknown_fields_overrides_and_partial_test_selection_are_errors() {
    for text in [
        basic() + "tests=[\"operator\"]\n",
        basic() + "skip=[\"codec\"]\n",
        basic() + "[run]\noutput_dir=\"other\"\n",
        basic().replace(
            "scheme=\"rq\"",
            "scheme={name=\"rq\",soruce=\"path:../scheme\"}",
        ),
        basic().replace("test=\"0.1.0\"", "test=\"operator-smoke@0.1.0\""),
        basic() + "[parameters]\nk=512\n",
        basic().replace(
            "scheme=\"rq\"",
            "scheme={name=\"rq\",source=\"path:../scheme#rq\"}",
        ),
    ] {
        assert!(parse(text.clone()).is_err(), "accepted {text}");
    }
}
#[test]
fn all_cases_are_planned_and_inapplicable_cases_have_reasons() {
    let c = parse(basic()).unwrap();
    let a = catalog::plan(&c.request.test, &c.request.scheme, "native-2").unwrap();
    let b = catalog::plan(&c.request.test, &c.request.scheme, "product-abi").unwrap();
    assert_eq!(a.identity, b.identity);
    assert_eq!(a.checks.len(), 26);
    assert_eq!(a.validation_cases().len(), 22);
    assert_eq!(b.validation_cases().len(), 16);
    assert_eq!(b.cases.len(), 4);
    assert_eq!(
        a.check("codec/r10/missing-8/decode")
            .unwrap()
            .skip
            .unwrap()
            .code,
        "scheme_not_selected"
    );
    assert_eq!(
        b.check("operator/1/direct").unwrap().skip.unwrap().code,
        "capability_unavailable"
    );
    assert!(
        b.check("operator/1/direct/self-xor")
            .unwrap()
            .measurement_skip
            .is_some()
    );
}
#[test]
fn skips_cannot_hide_missing_results_or_be_relabelled_as_success() {
    let mut r = support::record("product-abi", "rq");
    let report = coverage::report(&r);
    assert_eq!(report["registered_cases"], 26);
    assert_eq!(report["skipped_validation"], 10);
    assert_eq!(report["skipped_measurement"], 22);
    assert!(evidence::validate_samples(&r).is_ok());
    r.validation.pop();
    assert!(evidence::validate_samples(&r).is_err());
    let mut r = support::record("native-2", "rq");
    r.recipe
        .as_mut()
        .unwrap()
        .checks
        .get_mut("operator/1/direct")
        .unwrap()
        .skip = Some(fountain_eval::coverage::Skip {
        code: "fake".into(),
        reason: "hide failure".into(),
    });
    assert!(evidence::validate_samples(&r).is_err());
    r.status = "failed".into();
    r.validation.clear();
    r.samples.clear();
    let report = coverage::report(&r);
    assert!(
        report["cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["validation"]["status"] == "not_executed")
    );
}
#[test]
fn comparison_exposes_common_scope_and_refuses_different_schemes() {
    let left = support::record("native-2", "rq");
    let right = support::record("product-abi", "rq");
    let comparison = evidence::compare(&left, &right).unwrap();
    assert_eq!(comparison["rows"].as_array().unwrap().len(), 4);
    assert_eq!(comparison["coverage"]["right"]["skipped_measurement"], 22);
    assert!(evidence::compare(&left, &support::record("native-2", "r10")).is_err());
    let mut invalid = left.clone();
    invalid.validation.push(json!({"case":"codec/r10/missing-0/encode","severity":"fatal","rule":"codec-payload","status":"passed","witness":"fake"}));
    assert!(evidence::validate_samples(&invalid).is_err());
}
