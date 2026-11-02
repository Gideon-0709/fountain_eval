mod support;
use std::process::{Command, Output};
use support::Temp;
fn cli(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fountain-eval"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn config(engine: &str, operator: &str) -> String {
    format!("engine={engine:?}\noperator={operator:?}\nscheme=\"rq\"\ntest=\"0.1.0\"\n")
}
#[test]
fn help_and_full_catalog_are_available() {
    let t = Temp::new();
    assert!(cli(&t.0, &["--help"]).status.success());
    let output = cli(&t.0, &["list"]);
    let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(catalog["interfaces"].as_array().unwrap().len(), 3);
    assert_eq!(catalog["test_versions"].as_array().unwrap().len(), 1);
    assert_eq!(
        catalog["test_versions"][0]["checks"]
            .as_object()
            .unwrap()
            .len(),
        26
    );
}
#[test]
fn default_config_and_explicit_config_are_offline_and_equivalent() {
    let t = Temp::new();
    let path = t.write(
        "fountain-eval.toml",
        &config(
            "registry:fountain_engine@2.1.0",
            "registry:fountain_operators@2.0.0#slab",
        ),
    );
    let a = cli(&t.0, &["inspect"]);
    assert!(a.status.success());
    let b = cli(
        std::path::Path::new("/tmp"),
        &["inspect", "--config", path.to_str().unwrap()],
    );
    assert_eq!(a.stdout, b.stdout);
    let report: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(report["inspection"]["status"], "inspection_only");
    assert_eq!(report["inspection"]["execution_available"], false);
    assert!(!t.0.join("runs").exists());
    assert!(!t.0.join(".fountain-eval").exists());
}
#[test]
fn missing_config_invalid_sources_and_legacy_selectors_are_rejected() {
    let t = Temp::new();
    assert_eq!(cli(&t.0, &["run"]).status.code(), Some(2));
    assert_eq!(
        cli(&t.0, &["run", "--test", "operator-smoke@0.1.0"])
            .status
            .code(),
        Some(2)
    );
    t.write(
        "fountain-eval.toml",
        &config(
            "registry:fountain_engine@latest",
            "registry:fountain_operators@2.0.0#slab",
        ),
    );
    assert_eq!(cli(&t.0, &["inspect"]).status.code(), Some(2));
    t.write(
        "fountain-eval.toml",
        &config("path:missing-engine", "path:missing-operator"),
    );
    let result = cli(&t.0, &["inspect"]);
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["inspection"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "local_manifest_error")
    );
}
