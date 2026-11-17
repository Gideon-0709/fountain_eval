mod support;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use support::Temp;

fn cli(t: &Temp, args: &[&str]) -> Output {
    cli_at(t, &t.0, args)
}

fn cli_at(t: &Temp, directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_feval"))
        .current_dir(directory)
        .env("XDG_STATE_HOME", t.0.join("state"))
        .args(args)
        .output()
        .unwrap()
}

fn init(t: &Temp) -> PathBuf {
    assert!(cli(t, &["init"]).status.success());
    t.0.join("feval_workspace").canonicalize().unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn config(engine: &str, operator: &str) -> String {
    format!("engine0={engine:?}\noperator0={operator:?}\nscheme=\"rq\"\ntest=\"0.1.0\"\n")
}

#[test]
fn help_and_full_catalog_are_available() {
    let t = Temp::new();
    let help = cli(&t, &["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    for name in ["init", "activate", "workspace", "clean"] {
        assert!(text.contains(name));
    }
    let help = cli(&t, &["help", "init"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("feval init"));
    let output = cli(&t, &["list"]);
    let catalog = json(&output);
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
fn init_selects_a_workspace_and_only_an_explicit_target_is_reset() {
    let t = Temp::new();
    let root = init(&t);
    let path = root.join("fountain-eval.toml");
    let starter = fs::read_to_string(&path).unwrap();
    assert_eq!(
        json(&cli(&t, &["inspect"]))["configuration"]["selections"][0]["request"]["components"]["engine"]
            ["kind"],
        "registry"
    );

    let edited = starter.replace("rq", "r10");
    fs::write(&path, &edited).unwrap();
    assert_eq!(cli(&t, &["init"]).status.code(), Some(2));
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
    assert_eq!(cli(&t, &["init", "missing"]).status.code(), Some(2));
    assert!(!t.0.join("missing").exists());

    let other = t.0.join("other");
    fs::create_dir(&other).unwrap();
    t.write("other/.hidden/input.txt", "replace me");
    let outside = t.write("outside.txt", "keep me");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, other.join("linked.txt")).unwrap();
    assert!(cli(&t, &["init", "other"]).status.success());
    assert!(!other.join(".hidden").exists());
    assert!(!other.join("linked.txt").exists());
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep me");
    assert!(cli(&t, &["activate", "feval_workspace"]).status.success());
    let active = cli_at(&t, &other, &["workspace"]);
    assert_eq!(
        String::from_utf8(active.stdout).unwrap().trim_end(),
        root.to_str().unwrap()
    );
    assert!(cli(&t, &["activate", "other"]).status.success());
    assert_eq!(
        json(&cli_at(&t, &root, &["inspect"]))["configuration"]["workspace"],
        other.canonicalize().unwrap().to_str().unwrap()
    );
}

#[test]
fn default_config_and_explicit_config_are_offline_and_equivalent() {
    let t = Temp::new();
    let root = init(&t);
    let path = root.join("fountain-eval.toml");
    fs::write(
        &path,
        config(
            "registry:fountain_engine@2.1.0",
            "registry:fountain_operators@2.0.0#SlabDataOperator",
        ),
    )
    .unwrap();
    let a = cli_at(&t, Path::new("/tmp"), &["inspect"]);
    assert!(a.status.success());
    let b = cli(&t, &["inspect", "--config", path.to_str().unwrap()]);
    assert_eq!(a.stdout, b.stdout);
    let report = json(&a);
    assert_eq!(
        report["inspections"][0]["inspection"]["status"],
        "inspection_only"
    );
    assert_eq!(
        report["inspections"][0]["inspection"]["execution_available"],
        false
    );
    assert_eq!(fs::read_dir(root.join("results")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(root.join(".fountain-eval/cache"))
            .unwrap()
            .count(),
        0
    );
    let outside = t.write("outside.toml", "");
    assert_eq!(
        cli(&t, &["inspect", "--config", outside.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn missing_workspace_invalid_sources_and_legacy_selectors_are_rejected() {
    let t = Temp::new();
    assert_eq!(cli(&t, &["run"]).status.code(), Some(2));
    assert_eq!(
        cli(&t, &["run", "--test", "operator-smoke@0.1.0"])
            .status
            .code(),
        Some(2)
    );
    let path = init(&t).join("fountain-eval.toml");
    fs::write(
        &path,
        config(
            "registry:fountain_engine@latest",
            "registry:fountain_operators@2.0.0#SlabDataOperator",
        ),
    )
    .unwrap();
    assert_eq!(cli(&t, &["inspect"]).status.code(), Some(2));
    fs::write(
        &path,
        config("path:missing-engine", "path:missing-operator"),
    )
    .unwrap();
    let output = cli(&t, &["inspect"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        json(&output)["inspections"][0]["inspection"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "local_manifest_error")
    );
}

#[test]
fn explicit_configuration_and_prepared_runs_use_frozen_input() {
    let t = Temp::new();
    let root = init(&t);
    fs::write(root.join("fountain-eval.toml"), "invalid default").unwrap();
    let alternate = t.write(
        "feval_workspace/alternate.toml",
        "engine7 = 'path:missing-engine'\nscheme = 'rq'\ntest = '0.1.0'\n",
    );
    let alternate = alternate.canonicalize().unwrap();
    let alternate = alternate.to_str().unwrap();
    assert_eq!(
        json(&cli(&t, &["inspect", "--config", alternate]))["inspections"][0]["component_id"],
        7
    );
    let prepared = cli(&t, &["prepare", "--config", alternate, "--json"]);
    assert_eq!(prepared.status.code(), Some(4));
    let body = json(&prepared);
    let pending = body["prepared"].as_str().unwrap();
    let other = t.0.join("other");
    fs::create_dir(&other).unwrap();
    assert!(cli(&t, &["init", "other"]).status.success());
    let moved = other
        .join(".fountain-eval/prepared")
        .join(Path::new(pending).file_name().unwrap());
    fs::rename(pending, &moved).unwrap();
    let rejected = cli(&t, &["run", "--prepared", moved.to_str().unwrap()]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("does not belong to this workspace")
    );
    fs::rename(moved, pending).unwrap();
    assert!(cli(&t, &["activate", "feval_workspace"]).status.success());
    assert_eq!(
        cli(&t, &["run", "--prepared", pending, "--config", alternate])
            .status
            .code(),
        Some(2)
    );
    for args in [
        ["run", "--prepared", pending, "--json"],
        ["run", "--config", alternate, "--json"],
    ] {
        let output = cli(&t, &args);
        assert_eq!(output.status.code(), Some(4));
        let result = json(&output);
        assert_eq!(result["batch"]["configuration"]["path"], alternate);
        assert_eq!(result["batch"]["entries"][0]["component_id"], 7);
    }
    assert_eq!(
        cli(&t, &["run", "--prepared", pending]).status.code(),
        Some(2)
    );
}

#[test]
fn run_report_keep_and_clean_have_separate_lifecycles() {
    let t = Temp::new();
    let root = init(&t);
    let config =
        "engine0=\"path:missing0\"\nengine1=\"path:missing1\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n";
    fs::write(root.join("fountain-eval.toml"), config).unwrap();
    let human = cli(&t, &["run"]);
    assert_eq!(human.status.code(), Some(4));
    let stdout = String::from_utf8(human.stdout).unwrap();
    let stderr = String::from_utf8(human.stderr).unwrap();
    assert!(
        stdout.contains("RESULT: COMPLETED WITH FAILURES")
            && stdout.contains("Generate report: feval report")
    );
    assert!(
        stderr.contains("C0 [1/2]") && stderr.contains("C1 [2/2]") && stderr.contains("C0 FAILED")
    );
    assert!(stderr.lines().count() < 40 && !stderr.contains('\x1b') && !stderr.contains('\r'));
    assert!(stderr.contains("[1/4] Preparation") && stderr.contains("Resolve and freeze sources"));
    assert!(!stderr.contains("[VALIDATE") && !stderr.contains("[MEASURE"));
    assert_eq!(fs::read_dir(root.join("reports")).unwrap().count(), 0);

    let machine = cli(&t, &["run", "--json"]);
    assert_eq!(machine.status.code(), Some(4));
    let value = json(&machine);
    assert_eq!(value["batch"]["entries"].as_array().unwrap().len(), 2);
    let id = value["batch"]["run_id"].as_str().unwrap();
    let raw = Path::new(value["result"].as_str().unwrap());
    let cache = root.join(".fountain-eval/cache").join(id);
    assert!(raw.is_dir() && cache.is_dir());
    let destination = root.join("reports").join(id);
    fs::write(&destination, "block report publishing").unwrap();
    assert_eq!(cli(&t, &["report", id]).status.code(), Some(2));
    assert!(raw.is_dir());
    fs::remove_file(&destination).unwrap();
    let published = cli(&t, &["report", id, "--keep", "--json"]);
    assert!(published.status.success());
    let report = destination.join("report.md");
    assert!(raw.is_dir() && cache.is_dir() && report.is_file());
    assert_eq!(
        json(&cli(&t, &["report", report.to_str().unwrap(), "--json"]))["components"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(cli(&t, &["report", id]).status.success());
    assert!(!raw.exists() && !cache.exists() && report.is_file());

    let prepared = cli(&t, &["prepare", "--json"]);
    assert_eq!(prepared.status.code(), Some(4));
    let value = json(&prepared);
    assert!(Path::new(value["prepared"].as_str().unwrap()).is_dir());
    assert!(cli(&t, &["clean"]).status.success());
    for dir in ["results", ".fountain-eval/prepared", ".fountain-eval/cache"] {
        assert_eq!(fs::read_dir(root.join(dir)).unwrap().count(), 0);
    }
    assert_eq!(
        fs::read_to_string(root.join("fountain-eval.toml")).unwrap(),
        config
    );
    assert!(report.is_file());
    assert!(cli_at(&t, &root, &["init", "."]).status.success());
    assert!(!report.exists());
    assert_ne!(
        fs::read_to_string(root.join("fountain-eval.toml")).unwrap(),
        config
    );
}
