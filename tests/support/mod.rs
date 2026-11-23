#![allow(dead_code)]
use fountain_eval::{
    artifact::Component,
    build::Built,
    catalog,
    evidence::{self, Environment, Quadruple, Record},
    measurement::{self, Sample},
    util,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fountain-eval-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn write(&self, path: &str, text: &str) -> PathBuf {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    pub fn resolver(&self) -> fountain_eval::source::Resolver {
        fountain_eval::source::Resolver::new(
            &self.0.join("cache"),
            &self.0.join("evidence/run/logs"),
        )
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub fn compare(left: &Record, right: &Record) -> util::Result<serde_json::Value> {
    evidence::compare_summaries(
        &Component::from_record(left)?,
        &Component::from_record(right)?,
    )
}
pub fn record(family: &str, scheme: &str) -> Record {
    let configuration = fountain_eval::config::Configuration::parse("/fixture/fountain-eval.toml".into(),format!("engine0 = \"registry:fountain_engine@2.1.0\"\noperator0 = \"registry:fountain_operators@2.0.0#SlabDataOperator\"\nscheme = {scheme:?}\ntest = \"0.1.0\"\n")).unwrap();
    let configuration = configuration.selection(&configuration.selections[0]);
    let request = configuration.request.clone();
    let recipe = catalog::plan(
        &request.test,
        &request.scheme,
        family,
        fountain_eval::protocol::Scope::Pair,
    )
    .unwrap();
    let conditions = json!({"runtime":{"cpu":"synthetic-test-fixture","os":"test"},"build":{"profile":"release"},"scheme":{"name":scheme,"mode":"systematic"}});
    let samples: Vec<_> = recipe
        .cases
        .iter()
        .flat_map(|case| {
            let check = recipe.check(case).unwrap();
            (0..fountain_eval::content::REPETITIONS).map(move |rep| Sample {
                case: case.clone(),
                repetition: rep,
                worker:if check.spec.fresh_process{rep}else{rep/5},
                outcome:"completed".into(),
                details:json!({}),
                elapsed_ns: ((rep % 5) as u64 + 1) * 1000,
                payload_bytes: check.payload_bytes,
                operations: if check.spec.workload.ends_with("-replay"){10}else{check.operations},
                witness:if check.spec.warning{util::digest(&json!({"calls":check.operations,"outcomes":[{"count":check.operations,"actual":[0]}]}))}else{"synthetic-fixture-witness".into()},
                observation:check.spec.warning.then(||json!({"calls":check.operations,"outcomes":[{"count":check.operations,"actual":[0]}]})),
            })
        })
        .collect();
    Record {
        format_version: 1,
        timing: Default::default(),
        run_id: "fixture".into(),
        status: "succeeded".into(),
        stage: "complete".into(),
        request,
        configuration,
        coverage: json!({}),
        quadruple: Some(Quadruple {
            c: json!({"engine":"fixture","operator":"fixture"}),
            i: catalog::interface(family),
            t: recipe.identity.clone(),
            e: Environment {
                digest: util::digest(&conditions),
                conditions,
                observations: json!({"time":1}),
            },
        }),
        validation: recipe
            .validation_cases()
            .iter()
            .map(|c| {
                let check=recipe.check(c).unwrap();
                if check.severity == fountain_eval::validation::Severity::Warning {json!({"event":"validation","case":c,"severity":"warning","rule":check.rule,"status":"passed","expected":[0],"actual":[0],"witness":util::digest(&json!([0]))})}
                else {json!({"event":"validation","case":c,"severity":"fatal","rule":check.rule,"status":"passed","witness":"synthetic-fixture-witness","details":{"calls":10}})}
            })
            .collect(),
        summary: measurement::summarize(&samples).unwrap(),
        samples,
        diagnostics:recipe.checks.iter().filter(|(_,c)|c.skip.is_none()&&c.measured&&!c.spec.fresh_process&&c.spec.category!="operation").flat_map(|(case,_)|(0..fountain_eval::content::WORKERS).map(move|worker|json!({"case":case,"worker":worker,"elapsed_ns":1,"resources":{"scope":"synthetic fixture"},"observation":{"stages":{"exclusive_ns":{"work":3},"unattributed_ns":1,"total_ns":4}}}))).collect(),
        recipe: Some(recipe),
        built: None,
        error: None,
        correctness: Default::default(),
    }
}
pub fn built(directory: &Path) -> Built {
    Built {
        key: "fixture-build".into(),
        directory: directory.into(),
        binary: directory.join("driver"),
        diagnostic_binary: directory.join("driver"),
        artifacts: BTreeMap::new(),
        interface: catalog::interface("native-2"),
        family: "native-2".into(),
        implementation: "slab".into(),
        components: json!({}),
        dependency_graph: json!({}),
        bindings: json!([]),
        product_library: None,
    }
}

pub fn violation(
    case: &str,
    contract: &str,
    kind: fountain_eval::findings::FailureKind,
) -> serde_json::Value {
    json!(fountain_eval::findings::Violation {
        contract_id: contract.into(),
        failure_kind: kind,
        requirement: "The fixture must satisfy its explicit contract.".into(),
        context: json!({"case":case}),
        difference: None
    })
}
pub fn attach_warning_findings(r: &mut Record) {
    let plan = r.recipe.as_ref().unwrap();
    for event in &mut r.validation {
        if event["severity"] == "warning" {
            let c = &plan.checks[event["case"].as_str().unwrap()].spec;
            event["finding"] = json!(fountain_eval::findings::boundary_violation(
                c,
                &event["expected"],
                &event["actual"]
            ));
        }
    }
    for sample in &mut r.samples {
        let Some(observation) = sample.observation.as_mut() else {
            continue;
        };
        let c = &plan.checks[&sample.case].spec;
        let event = r
            .validation
            .iter()
            .find(|e| e["case"] == sample.case)
            .unwrap();
        for outcome in observation["outcomes"].as_array_mut().unwrap() {
            outcome["finding"] = json!(fountain_eval::findings::boundary_violation(
                c,
                &event["expected"],
                &outcome["actual"]
            ));
        }
        sample.witness = util::digest(observation);
    }
}
