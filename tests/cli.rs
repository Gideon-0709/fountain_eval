use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fountain-eval"))
        .args(args)
        .output()
        .unwrap()
}

fn arguments(command: &str) -> [&str; 7] {
    [
        command,
        "--engine",
        "registry:fountain_engine@2.1.0",
        "--operator",
        "registry:example_operator@0.1.0#slab",
        "--test",
        "example-unregistered@0.1.0",
    ]
}

#[test]
fn help_and_registered_catalog_are_available() {
    assert!(cli(&["--help"]).status.success());
    let output = cli(&["list"]);
    assert!(output.status.success());
    let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(catalog["interfaces"].as_array().unwrap().len(), 3);
    assert_eq!(catalog["tests"].as_array().unwrap().len(), 2);
}

#[test]
fn inspection_is_still_offline_and_does_not_claim_execution() {
    let inspect = cli(&arguments("inspect"));
    assert_eq!(inspect.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(report["status"], "inspection_only");
    assert_eq!(report["execution_available"], false);
}

#[test]
fn invalid_input_takes_precedence_over_execution_unavailability() {
    assert_eq!(cli(&["run"]).status.code(), Some(2));
    let mut args = arguments("run");
    args[2] = "registry:fountain_engine@latest";
    let malformed = cli(&args);
    assert_eq!(malformed.status.code(), Some(2));
    assert!(malformed.stdout.is_empty());
    args[0] = "inspect";
    args[2] = "path:Cargo.toml";
    args[4] = "path:Cargo.lock";
    let manifest_error = cli(&args);
    assert_eq!(manifest_error.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&manifest_error.stdout).unwrap();
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "local_manifest_error")
    );
}
