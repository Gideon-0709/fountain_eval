mod support;
use fountain_eval::{batch, config::Configuration, evidence, util};
use support::Temp;

fn failures(t: &Temp, prepare: bool) -> (std::path::PathBuf, batch::Record) {
    let path = t.write(
        "eval.toml",
        "engine0=\"path:missing0\"\noperator2=\"path:missing2\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n",
    );
    batch::run(Configuration::load(&path).unwrap(), prepare).unwrap()
}
#[test]
fn independent_failures_continue_and_automatic_reports_retain_each_c() {
    let t = Temp::new();
    let (path, r) = failures(&t, false);
    assert_eq!(r.status, "completed_with_failures");
    assert_eq!(batch::exit_code(&r), 4);
    assert_eq!(r.entries.len(), 2);
    for e in &r.entries {
        assert_eq!(e.status, "failed");
        assert!(e.elapsed_ns.unwrap() > 0);
        let child = path.join(e.directory.as_ref().unwrap());
        assert!(child.join("report.html").is_file());
        let record = evidence::load(&child).unwrap();
        assert!(record.quadruple.is_none());
        assert!(record.samples.is_empty());
    }
    let report = std::fs::read_to_string(path.join("report.html")).unwrap();
    assert!(report.contains("C0") && report.contains("C2") && report.contains("Detailed results"));
    assert!(path.join("runs.csv").is_file());
    assert_eq!(batch::load(&path).unwrap().entries.len(), 2);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fountain-eval"))
        .args(["report", path.to_str().unwrap(), "--format", "markdown"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // Integrity checks do not depend on surviving build caches.
    std::fs::remove_dir_all(t.0.join(".fountain-eval")).unwrap();
    assert!(batch::load(&path).is_ok());
    std::fs::write(path.join("runs.csv"), "corrupt").unwrap();
    assert!(batch::load(&path).is_err());
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
    let origin: serde_json::Value = util::read_json(&second.join("prepared-from.json")).unwrap();
    assert_eq!(origin["entries"][0]["status"], "failed");
    assert!(origin["entries"][0]["error"].is_string());
    assert!(batch::load(&first).is_ok());
    assert!(batch::load(&second).is_ok());
}
#[test]
fn resealed_batch_cannot_change_child_identity_counts_or_duration() {
    let t = Temp::new();
    let (path, r) = failures(&t, false);
    for field in ["samples", "skipped", "component_id", "elapsed_ns"] {
        let mut value = serde_json::to_value(&r).unwrap();
        value["entries"][0][field] = if field == "elapsed_ns" {
            0.into()
        } else {
            999.into()
        };
        util::write_json(&path.join("batch.json"), &value).unwrap();
        evidence::seal(&path).unwrap();
        assert!(batch::load(&path).is_err(), "accepted altered {field}");
    }
}

#[cfg(unix)]
#[test]
fn interrupt_stops_the_batch_and_leaves_later_c_unexecuted() {
    use std::{
        fs::OpenOptions,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    let t = Temp::new();
    let config = t.write(
        "eval.toml",
        "engine0=\"path:missing0\"\nengine1=\"path:missing1\"\nscheme=\"rq\"\ntest=\"0.1.0\"\n",
    );
    std::fs::create_dir(t.0.join(".fountain-eval")).unwrap();
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(t.0.join(".fountain-eval/execution.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fountain-eval"))
        .args(["run", "--config", config.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = loop {
        assert!(Instant::now() < deadline, "child did not enter its first C");
        assert!(child.try_wait().unwrap().is_none());
        let found = std::fs::read_dir(t.0.join("runs"))
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
    let record = batch::load(&root).unwrap();
    assert_eq!(record.status, "interrupted");
    assert_eq!(record.entries[0].status, "interrupted");
    assert_eq!(record.entries[1].status, "not_executed");
    assert!(record.entries[1].directory.is_none());
    assert!(record.entries[1].elapsed_ns.is_none());
}
