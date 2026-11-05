mod support;
use std::process::{Command, Output};
use support::Temp;
fn cli(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_feval"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn config(engine: &str, operator: &str) -> String {
    format!("engine0={engine:?}\noperator0={operator:?}\nscheme=\"rq\"\ntest=\"0.1.0\"\n")
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
        fountain_eval::content::cases().len()
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
    assert_eq!(
        report["inspections"][0]["inspection"]["status"],
        "inspection_only"
    );
    assert_eq!(
        report["inspections"][0]["inspection"]["execution_available"],
        false
    );
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
        report["inspections"][0]["inspection"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "local_manifest_error")
    );
}

#[test]
fn human_progress_is_default_and_json_stdout_remains_machine_readable() {
    let t = Temp::new();
    t.write(
        "fountain-eval.toml",
        "engine0=\"path:missing0\"\nengine1=\"path:missing1\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n",
    );
    let human = cli(&t.0, &["run"]);
    assert_eq!(human.status.code(), Some(4));
    let stdout = String::from_utf8(human.stdout).unwrap();
    let stderr = String::from_utf8(human.stderr).unwrap();
    assert!(stdout.contains("RESULT: COMPLETED WITH FAILURES"));
    assert!(
        stdout.contains("Report:")
            && stdout.contains("Temporary evidence and build cache: cleaned")
    );
    assert!(stderr.contains("C0 [1/2]") && stderr.contains("C1 [2/2]"));
    assert!(stderr.contains("C0 FAILED") && stderr.contains("REPORT  "));
    assert!(
        stderr.lines().count() < 40,
        "Two early failures need only a compact stage summary"
    );
    assert!(stderr.contains("[1/4] Preparation") && stderr.contains("Resolve and freeze sources"));
    assert!(!stderr.contains('\x1b') && !stderr.contains('\r'));
    assert!(!stderr.contains("[VALIDATE") && !stderr.contains("[MEASURE"));
    let machine = cli(&t.0, &["run", "--json"]);
    assert_eq!(machine.status.code(), Some(4));
    let value: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(value["batch"]["entries"].as_array().unwrap().len(), 2);
    assert!(value.get("run_dir").is_none());
    let report = std::path::Path::new(value["report"].as_str().unwrap());
    assert!(report.is_file());
    assert!(!t.0.join("runs").exists());
    assert!(!t.0.join(".fountain-eval").exists());
    let index = cli(&t.0, &["report", report.to_str().unwrap(), "--json"]);
    assert!(index.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&index.stdout).unwrap()["components"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        String::from_utf8(machine.stderr)
            .unwrap()
            .contains("C1 [2/2]")
    );
    let prepared = cli(&t.0, &["prepare", "--json"]);
    assert_eq!(prepared.status.code(), Some(4));
    let value: serde_json::Value = serde_json::from_slice(&prepared.stdout).unwrap();
    assert_eq!(value["batch"]["action"], "prepare");
    assert!(value.get("report").is_none());
    assert!(std::path::Path::new(value["prepared"].as_str().unwrap()).is_dir());
}
