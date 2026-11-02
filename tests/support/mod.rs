#![allow(dead_code)]
use fountain_eval::{
    build::Built,
    catalog,
    evidence::{Environment, Quadruple, Record},
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
pub fn record(family: &str, scheme: &str) -> Record {
    let configuration = fountain_eval::config::Configuration::parse("/fixture/fountain-eval.toml".into(),format!("engine = \"registry:fountain_engine@2.1.0\"\noperator = \"registry:fountain_operators@2.0.0#slab\"\nscheme = {scheme:?}\ntest = \"0.1.0\"\n")).unwrap();
    let request = configuration.request.clone();
    let recipe = catalog::plan(&request.test, &request.scheme, family).unwrap();
    let conditions = json!({"runtime":{"cpu":"synthetic-test-fixture","os":"test"},"build":{"profile":"release"},"scheme":{"name":scheme,"mode":"systematic"}});
    let samples: Vec<_> = recipe
        .cases
        .iter()
        .flat_map(|case| {
            let check = recipe.check(case).unwrap();
            (0..3).map(move |rep| Sample {
                case: case.clone(),
                repetition: rep,
                elapsed_ns: (rep as u64 + 1) * 1000,
                payload_bytes: check.payload_bytes,
                operations: check.operations,
                witness: if case.ends_with("/self-xor") || case.ends_with("/duplicate-source-xor") {
                    util::digest(&json!({"calls":256,"outcomes":[{"count":256,"actual":[0]}]}))
                } else {
                    "synthetic-fixture-witness".into()
                },
                observation: (case.ends_with("/self-xor")
                    || case.ends_with("/duplicate-source-xor"))
                .then(|| json!({"calls":256,"outcomes":[{"count":256,"actual":[0]}]})),
            })
        })
        .collect();
    Record {
        format_version: 1,
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
                else {json!({"event":"validation","case":c,"severity":"fatal","rule":check.rule,"status":"passed","witness":"synthetic-fixture-witness"})}
            })
            .collect(),
        summary: measurement::summarize(&samples).unwrap(),
        samples,
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
