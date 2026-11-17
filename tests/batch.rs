mod support;
use fountain_eval::{artifact, batch, config::Configuration};
use support::Temp;

fn failures(t: &Temp, prepare: bool) -> (std::path::PathBuf, batch::Record) {
    let path = t.write(
        "eval.toml",
        "engine0=\"path:missing0\"\noperator2=\"path:missing2\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n",
    );
    batch::run(Configuration::load(&path).unwrap(), prepare).unwrap()
}
#[test]
fn independent_failures_continue_and_explicit_reports_retain_each_c() {
    let t = Temp::new();
    let (raw, r) = failures(&t, false);
    assert_eq!(r.status, "completed_with_failures");
    assert_eq!(batch::exit_code(&r), 4);
    assert_eq!(r.entries.len(), 2);
    assert!(!t.0.join("reports").exists());
    let path = artifact::generate(&raw, &t.0.join("reports"), false).unwrap();
    let (_, saved) = artifact::load(&path).unwrap();
    assert_eq!(saved.components.len(), 2);
    for c in &saved.components {
        assert_eq!(c.status, "failed");
        assert!(c.quadruple.is_none());
        assert!(c.summary.is_empty());
    }
    let report = std::fs::read_to_string(&path).unwrap();
    assert!(report.contains("C0") && report.contains("C2") && report.contains("Key findings"));
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    assert!(!r.configuration.run.output.join(&r.run_id).exists());
    assert!(!r.configuration.run.cache.exists());
    assert!(artifact::load(&path).is_ok());
    std::fs::write(&path, report.replace("Key findings", "Changed findings")).unwrap();
    assert!(artifact::load(&path).is_err());
}
#[test]
fn prepared_failures_are_not_retried_and_their_provenance_is_retained() {
    let t = Temp::new();
    let (first, _) = failures(&t, true);
    let (second, r) = batch::from_prepared(&first).unwrap();
    assert_ne!(first, second);
    assert!(
        r.entries
            .iter()
            .all(|e| e.status == "not_executed" && e.elapsed_ns.is_none() && e.directory.is_none())
    );
    assert!(first.is_dir());
    assert!(
        batch::from_prepared(&first)
            .unwrap_err()
            .contains("consumed")
    );
    let report = artifact::generate(&second, &t.0.join("reports"), false).unwrap();
    let (_, saved) = artifact::load(&report).unwrap();
    let origin = saved.prepared_from.unwrap();
    assert_eq!(origin["entries"][0]["status"], "failed");
    assert!(origin["entries"][0]["error"].is_string());
    assert!(!first.exists());
    assert!(!t.0.join(".fountain-eval").exists());
    assert!(artifact::load(&report).is_ok());
}
#[test]
fn report_rejects_component_identity_and_count_changes() {
    let t = Temp::new();
    let (raw, _) = failures(&t, false);
    let path = artifact::generate(&raw, &t.0.join("reports"), false).unwrap();
    let (text, saved) = artifact::load(&path).unwrap();
    for field in ["samples", "skipped", "component_id", "elapsed_ns"] {
        let mut v = serde_json::to_value(&saved).unwrap();
        v["batch"]["entries"][0][field] = if field == "elapsed_ns" {
            0.into()
        } else {
            999.into()
        };
        let altered = serde_json::from_value(v).unwrap();
        assert!(artifact::publish(&t.0.join(field), &text, &altered, false).is_err());
    }
}

#[cfg(unix)]
#[test]
fn interrupt_stops_the_batch_and_leaves_later_c_unexecuted() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    let t = Temp::new();
    let feval = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_feval"));
        command
            .current_dir(&t.0)
            .env("XDG_STATE_HOME", t.0.join("state"));
        command
    };
    assert!(feval().arg("init").output().unwrap().status.success());
    let config = t.write(
        "feval_workspace/fountain-eval.toml",
        "engine0=\"git+https://example.invalid/engine.git?package=fountain_engine&rev=1111111111111111111111111111111111111111\"\nengine1=\"path:missing1\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n",
    );
    let git = t.write("bin/git", "#!/bin/sh\nexec sleep 60\n");
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = feval()
        .args(["run", "--config", config.to_str().unwrap()])
        .env(
            "PATH",
            format!(
                "{}:{}",
                t.0.join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = loop {
        assert!(Instant::now() < deadline, "child did not enter its first C");
        assert!(child.try_wait().unwrap().is_none());
        let found = std::fs::read_dir(t.0.join("feval_workspace/results"))
            .ok()
            .and_then(|mut paths| paths.next())
            .and_then(Result::ok)
            .map(|p| p.path());
        if let Some(root) = found {
            let ready = std::fs::read_dir(root.join("C0"))
                .ok()
                .and_then(|mut paths| paths.next())
                .and_then(Result::ok)
                .is_some_and(|p| p.path().join("result.json").exists());
            if ready {
                break root;
            }
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        String::from_utf8_lossy(&feval().arg("clean").output().unwrap().stderr)
            .contains("workspace is in use")
    );
    assert!(
        String::from_utf8_lossy(
            &feval()
                .args(["init", "feval_workspace"])
                .output()
                .unwrap()
                .stderr
        )
        .contains("workspace is in use")
    );
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.exists());
    let id = root.file_name().unwrap().to_str().unwrap();
    assert!(
        feval()
            .args(["report", id])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!root.exists());
    let report =
        t.0.join("feval_workspace/reports")
            .join(id)
            .join("report.md");
    let (_, saved) = artifact::load(&report).unwrap();
    let record = saved.batch;
    assert_eq!(record.status, "interrupted");
    assert_eq!(record.entries[0].status, "interrupted");
    assert_eq!(record.entries[1].status, "not_executed");
    assert!(record.entries[1].directory.is_none());
    assert!(record.entries[1].elapsed_ns.is_none());
}

#[test]
fn one_batch_never_cleans_another_preparation_and_busy_preparation_is_rejected() {
    let t = Temp::new();
    let (pending, r) = failures(&t, true);
    assert!(!t.0.join("reports").exists());
    let cache = r.configuration.run.cache.join(&r.run_id);
    assert!(cache.is_dir());
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pending.join("lease.tmp"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(
        batch::from_prepared(&pending)
            .unwrap_err()
            .contains("in use")
    );
    let (raw, _) = failures(&t, false);
    let report = artifact::generate(&raw, &t.0.join("reports"), false).unwrap();
    assert!(report.is_file() && pending.is_dir() && cache.is_dir());
    drop(lock);
    let (raw, _) = batch::from_prepared(&pending).unwrap();
    let report = artifact::generate(&raw, &t.0.join("reports"), false).unwrap();
    assert!(report.is_file() && !pending.exists() && !cache.exists());
    assert!(!t.0.join("runs").exists());
    assert!(!t.0.join(".fountain-eval").exists());
}
