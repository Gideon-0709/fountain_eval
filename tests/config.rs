mod support;
use fountain_eval::{
    catalog, config::Configuration, coverage, evidence, protocol::SourceReference,
};
use serde_json::json;
fn basic() -> String {
    "engine0=\"path:../engine\"\noperator0=\"path:../operator#SlabDataOperator\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n"
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
        config.selections[0]
            .request
            .components
            .engine
            .as_ref()
            .unwrap()
            .source,
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
    assert_eq!(config.selections[0].request.scheme.name, "r10");
    assert_eq!(
        config.selections[0]
            .request
            .scheme
            .source
            .as_ref()
            .unwrap()
            .source,
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
            "engine0=\"path:../engine\"",
            "engine0={source=\"path:../engine\", feature=[]}",
        ),
        basic().replace(
            "engine0=\"path:../engine\"",
            "engine0={source=\"path:../engine\", features=[\"two names\"]}",
        ),
        basic().replace(
            "scheme=\"rq\"",
            "scheme={name=\"rq\",source=\"path:../scheme#rq\"}",
        ),
    ] {
        assert!(parse(text.clone()).is_err(), "accepted {text}");
    }
}
#[test]
fn component_build_options_follow_each_selection_and_round_trip() {
    let config = parse(r#"
engine0 = { source = "path:../engine", features = ["second", "first", "first"], default-features = true }
operator0 = { source = "path:../operator#ops::Slab", features = ["accelerated"] }
operator1 = "path:../operator#ops::Slab"
scheme = "rq"
test = "0.1.0"
"#.into()).unwrap();
    let pair = &config.selections[0].request.components;
    let engine = pair.engine.as_ref().unwrap();
    assert_eq!(engine.build.features, ["first", "second"]);
    assert!(engine.build.default_features);
    let operator = pair.operator.as_ref().unwrap();
    assert_eq!(operator.implementation.as_deref(), Some("ops::Slab"));
    assert_eq!(operator.build.features, ["accelerated"]);
    assert!(!operator.build.default_features);
    let single = config.selections[1]
        .request
        .components
        .operator
        .as_ref()
        .unwrap();
    assert_eq!(single.source, operator.source);
    assert_eq!(single.build, Default::default());
    let value = serde_json::to_value(&config).unwrap();
    let restored: Configuration = serde_json::from_value(value).unwrap();
    assert_eq!(restored.selections[0].request, config.selections[0].request);
}
#[test]
fn all_cases_are_planned_and_inapplicable_cases_have_reasons() {
    let c = parse(basic()).unwrap();
    let a = catalog::plan(
        &c.selections[0].request.test,
        &c.selections[0].request.scheme,
        "native-2",
        fountain_eval::protocol::Scope::Pair,
    )
    .unwrap();
    let b = catalog::plan(
        &c.selections[0].request.test,
        &c.selections[0].request.scheme,
        "product-abi",
        fountain_eval::protocol::Scope::Pair,
    )
    .unwrap();
    assert_eq!(a.identity, b.identity);
    assert_eq!(a.checks.len(), fountain_eval::content::cases().len());
    assert!(a.validation_cases().len() > b.validation_cases().len());
    assert!(b.cases.len() > 100);
    assert!(
        a.checks
            .values()
            .filter(|c| c.spec.scheme.as_deref() == Some("r10"))
            .all(|c| c.skip.as_ref().unwrap().code == "scheme_not_selected")
    );
    assert!(
        b.checks
            .values()
            .filter(|c| c.spec.category == "operation")
            .all(|c| c.skip.as_ref().unwrap().code == "capability_unavailable")
    );
}
#[test]
fn skips_cannot_hide_missing_results_or_be_relabelled_as_success() {
    let mut r = support::record("product-abi", "rq");
    let report = coverage::report(&r);
    assert_eq!(
        report["registered_cases"],
        r.recipe.as_ref().unwrap().checks.len()
    );
    assert!(report["skipped_validation"].as_u64().unwrap() > 0);
    assert!(report["skipped_measurement"].as_u64().unwrap() > 0);
    assert!(evidence::validate_samples(&r).is_ok());
    r.validation.pop();
    assert!(evidence::validate_samples(&r).is_err());
    let mut r = support::record("native-2", "rq");
    r.recipe
        .as_mut()
        .unwrap()
        .checks
        .get_mut("operation/xor-one/b16/direct/small")
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
    let comparison = support::compare(&left, &right).unwrap();
    assert!(comparison["rows"].as_array().unwrap().len() > 100);
    assert!(
        comparison["coverage"]["right"]["skipped_measurement"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(support::compare(&left, &support::record("native-2", "r10")).is_err());
    let mut invalid = left.clone();
    invalid.validation.push(json!({"case":"codec/r10/missing-0/encode","severity":"fatal","rule":"codec-payload","status":"passed","witness":"fake"}));
    assert!(evidence::validate_samples(&invalid).is_err());
}

#[test]
fn r10_success_cases_respect_the_registered_domain_on_every_interface() {
    let c = parse(basic().replace("scheme=\"rq\"", "scheme=\"r10\"")).unwrap();
    for family in ["native-2", "native-3", "product-abi"] {
        let plan = catalog::plan(
            &c.selections[0].request.test,
            &c.selections[0].request.scheme,
            family,
            fountain_eval::protocol::Scope::Pair,
        )
        .unwrap();
        assert_eq!(
            plan.check("correctness/scheme-padding/r10/k1/b128")
                .unwrap()
                .skip
                .unwrap()
                .code,
            "outside_api_domain"
        );
        assert!(
            plan.check("correctness/scheme-padding/r10/k4/b128")
                .unwrap()
                .skip
                .is_none()
        );
    }
}

#[test]
fn indexed_selections_are_numeric_sparse_and_allow_single_components() {
    use fountain_eval::protocol::Scope;
    let c=parse("operator10=\"path:../op#SimdDataOperator\"\nengine2=\"registry:fountain_engine@2.1.0\"\nengine0=\"path:../engine\"\noperator0=\"path:../operator#SlabDataOperator\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n".into()).unwrap();
    assert_eq!(
        c.selections.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![0, 2, 10]
    );
    for (selection, scope) in c
        .selections
        .iter()
        .zip([Scope::Pair, Scope::Engine, Scope::Operator])
    {
        assert_eq!(selection.request.components.scope().unwrap(), scope);
        let derived = c.selection(selection);
        assert_eq!(derived.original, c.original);
        assert_eq!(derived.component_id, selection.id);
    }
    for invalid in [
        "engine",
        "engine01",
        "operator-1",
        "operator+1",
        "engine4294967296",
        "operatorx",
        "other0",
    ] {
        assert!(
            parse(format!(
                "{invalid}=\"path:../x\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n"
            ))
            .is_err(),
            "{invalid}"
        );
    }
    assert!(parse("scheme=\"rq\"\ntest=\"0.1.0\"\n".into()).is_err());
    assert!(parse(basic() + "engine0=\"path:../other\"\n").is_err());
}
#[test]
fn single_components_keep_full_catalog_with_no_implicit_production_partner() {
    use fountain_eval::protocol::Scope;
    let c = parse(basic()).unwrap();
    let r = &c.selections[0].request;
    for family in ["native-2", "native-3", "product-abi"] {
        let engine = catalog::plan(&r.test, &r.scheme, family, Scope::Engine).unwrap();
        assert_eq!(engine.checks.len(), fountain_eval::content::cases().len());
        assert!(
            engine
                .checks
                .values()
                .filter(|c| c.skip.is_none())
                .all(|c| matches!(c.spec.workload.as_str(), "plan-build" | "plan-export"))
        );
        if family == "product-abi" {
            assert!(engine.cases.is_empty());
        } else {
            assert_eq!(engine.cases.len(), 6);
        }
        let operator = catalog::plan(&r.test, &r.scheme, family, Scope::Operator).unwrap();
        assert_eq!(operator.identity, engine.identity);
        assert!(
            operator
                .checks
                .values()
                .filter(|c| c.skip.is_none())
                .all(|c| c.spec.scheme.is_none())
        );
        assert!(operator.checks.values().any(|c| {
            c.skip
                .as_ref()
                .is_some_and(|s| s.code == "component_not_selected")
        }));
    }
}

#[test]
fn report_destination_is_relative_to_config_and_independent_of_evidence() {
    let c = parse(basic() + "[run]\nreport=\"../../readable reports\"\n").unwrap();
    assert_eq!(
        c.report_root(),
        std::path::Path::new("/project/configs/../../readable reports")
    );
    assert_eq!(c.run.output, std::path::Path::new("/project/configs/runs"));
    assert_eq!(
        parse(basic()).unwrap().report_root(),
        std::path::Path::new("/project/configs/reports")
    );
    assert!(parse(basic() + "[run]\nreport=\"\"\n").is_err());
    // Older sealed records omitted this optional setting; their normalized identity is unchanged.
    let old = parse(basic()).unwrap();
    let encoded = serde_json::to_value(&old).unwrap();
    assert!(encoded["run"].get("report").is_none());
    let decoded: Configuration = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.report_root(), old.report_root());
}
