use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use fountain_eval::{
    protocol::{EvaluationRequest, Inspection},
    runner::inspect,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Inputs(PathBuf);

impl Inputs {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fountain-eval-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn manifest(&self, directory: &str, contents: &str) -> String {
        let directory = self.0.join(directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Cargo.toml");
        fs::write(&path, contents).unwrap();
        format!("path:{}", path.display())
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn request(engine: &str, operator: &str) -> EvaluationRequest {
    EvaluationRequest::parse(engine, operator, "example-unregistered@0.1.0").unwrap()
}

fn has_code(report: &Inspection, code: &str) -> bool {
    report.diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn detects_incompatible_engine_without_editing_or_building_sources() {
    let inputs = Inputs::new();
    let engine_manifest = "[package]\nname = 'fountain_engine'\nversion = '3.0.0-dev'\n";
    let operator_manifest = "[package]\nname = 'test_operator'\nversion = '0.1.0'\n[dependencies]\nengine_alias = { package = 'fountain_engine', version = '=2.1.0' }\n";
    let engine = inputs.manifest("engine", engine_manifest);
    let operator = inputs.manifest("operator", operator_manifest);
    let report = inspect(request(&engine, &operator));
    assert!(report.has_errors());
    assert!(has_code(&report, "engine_version_conflict"));
    assert!(!report.execution_available);
    for (name, expected) in [("engine", engine_manifest), ("operator", operator_manifest)] {
        assert_eq!(
            fs::read_to_string(inputs.0.join(name).join("Cargo.toml")).unwrap(),
            expected
        );
        assert_eq!(fs::read_dir(inputs.0.join(name)).unwrap().count(), 1);
    }
    assert_eq!(fs::read_dir(&inputs.0).unwrap().count(), 2);
}

#[test]
fn matching_declared_versions_do_not_claim_evaluation_success() {
    let inputs = Inputs::new();
    let operator = inputs.manifest(
        "operator",
        "[package]\nname = 'op'\nversion = '0.1.0'\n[dependencies]\nfountain_engine = '=2.1.0'\n",
    );
    let report = inspect(request("registry:fountain_engine@2.1.0", &operator));
    assert!(!report.has_errors());
    assert!(!report.execution_available);
    assert!(has_code(&report, "resolution_deferred"));
    assert!(has_code(&report, "execution_not_attempted"));
    assert!(report.local_declarations.engine.is_none());
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["status"], "inspection_only");
    for absent in [
        "samples",
        "throughput",
        "validation",
        "identity",
        "environment",
    ] {
        assert!(json.get(absent).is_none());
    }
}

#[test]
fn inherited_and_conditional_constraints_remain_unresolved() {
    let inputs = Inputs::new();
    let engine = inputs.manifest(
        "engine",
        "[package]\nname = 'fountain_engine'\nversion.workspace = true\n",
    );
    let operator = inputs.manifest("operator", "[package]\nname = 'op'\nversion = '0.1.0'\n[dependencies]\nfountain_engine = { version = '=2.1.0', optional = true }\n[target.'cfg(unix)'.dependencies]\nfountain_engine = '=2.1.0'\n");
    let report = inspect(request(&engine, &operator));
    assert!(!report.has_errors());
    assert!(has_code(&report, "version_inheritance_deferred"));
    assert!(report.local_declarations.engine.unwrap().version.is_none());
    assert!(
        report
            .local_declarations
            .operator
            .unwrap()
            .engine_requirement
            .is_none()
    );
    assert!(!report.execution_available);
}

#[test]
fn bad_local_manifests_and_virtual_roots_are_errors() {
    let inputs = Inputs::new();
    for (name, text) in [
        ("virtual", "[workspace]\nmembers = []\n"),
        ("broken", "[package"),
        (
            "version",
            "[package]\nname = 'engine'\nversion = 'latest'\n",
        ),
    ] {
        let engine = inputs.manifest(name, text);
        let report = inspect(request(&engine, "registry:operator@1.0.0"));
        assert!(report.has_errors());
        assert!(has_code(&report, "local_manifest_error"));
    }
    let missing = format!("path:{}/missing/Cargo.toml", inputs.0.display());
    assert!(inspect(request(&missing, "registry:operator@1.0.0")).has_errors());
}

#[test]
fn invalid_direct_requirement_is_rejected_when_engine_version_is_known() {
    let inputs = Inputs::new();
    let operator = inputs.manifest("operator", "[package]\nname = 'op'\nversion = '0.1.0'\n[dependencies]\nfountain_engine = 'not-semver'\n");
    let report = inspect(request("registry:fountain_engine@2.1.0", &operator));
    assert!(report.has_errors());
    assert!(has_code(&report, "invalid_engine_requirement"));
}
