#![allow(dead_code)]
use fountain_eval::{
    build::Built,
    catalog,
    evidence::{Environment, Quadruple, Record},
    measurement::{self, Sample},
    protocol::EvaluationRequest,
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
pub fn record(family: &str, test: &str) -> Record {
    let request = EvaluationRequest::parse(
        "registry:fountain_engine@2.1.0",
        "registry:fountain_operators@2.0.0#slab",
        test,
    )
    .unwrap();
    let recipe = catalog::recipe(&request.test).unwrap();
    let conditions = json!({"runtime":{"cpu":"synthetic-test-fixture","os":"test"},"build":{"profile":"release"}});
    let samples: Vec<_> = recipe
        .cases
        .iter()
        .flat_map(|case| {
            (0..3).map(move |rep| Sample {
                case: case.clone(),
                repetition: rep,
                elapsed_ns: (rep as u64 + 1) * 1000,
                payload_bytes: if case.starts_with("codec/") { 8192 } else { 0 },
                operations: if case.starts_with("operator/") { 14 } else { 0 },
                witness: "synthetic-fixture-witness".into(),
            })
        })
        .collect();
    Record {
        format_version: 1,
        run_id: "fixture".into(),
        status: "succeeded".into(),
        stage: "complete".into(),
        request,
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
            .cases
            .iter()
            .map(|c| json!({"event":"validation","case":c,"witness":"synthetic-fixture-witness"}))
            .collect(),
        summary: measurement::summarize(&samples).unwrap(),
        samples,
        recipe: Some(recipe),
        built: None,
        error: None,
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
