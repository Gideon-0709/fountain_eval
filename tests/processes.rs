#![cfg(unix)]
mod support;
use fountain_eval::process;
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use support::Temp;
fn gone(pid: &str) -> bool {
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&out.stdout);
    !out.status.success() || state.trim().is_empty() || state.trim().starts_with('Z')
}
#[test]
fn timeout_and_crash_kill_child_groups_and_preserve_output() {
    for crash in [false, true] {
        let t = Temp::new();
        let mut command = Command::new("sh");
        command.arg("-c").arg(if crash {
            "sleep 30 & echo $!; echo diagnostic >&2; exit 7"
        } else {
            "sleep 30 & echo $!; echo diagnostic >&2; wait"
        });
        let start = Instant::now();
        let error =
            process::run(&mut command, &t.0, "driver", Duration::from_millis(100)).unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(error.contains(if crash { "exited" } else { "timeout" }));
        let pid = fs::read_to_string(t.0.join("driver.stdout")).unwrap();
        assert!(gone(pid.trim()), "child {pid} still running");
        assert!(
            fs::read_to_string(t.0.join("driver.stderr"))
                .unwrap()
                .contains("diagnostic")
        );
    }
    let t = Temp::new();
    assert!(
        process::run(
            Command::new("sh").args(["-c", "kill -TERM $$"]),
            &t.0,
            "crash",
            Duration::from_secs(5)
        )
        .unwrap_err()
        .contains("exited")
    );
}
// This helper runs only in a separate test process so SIGINT cannot poison other tests.
#[test]
#[ignore = "invoked by interruption_isolated_process"]
fn interruption_helper() {
    let Some(path) = std::env::var_os("FOUNTAIN_EVAL_TEST_INTERRUPT_DIR") else {
        return;
    };
    process::install_handler().unwrap();
    let result = process::run(
        Command::new("sh").args(["-c", "sleep 30 & echo $!; wait"]),
        std::path::Path::new(&path),
        "interrupt",
        Duration::from_secs(30),
    );
    assert_eq!(result.unwrap_err(), "interrupted");
}
#[test]
fn interruption_isolated_process() {
    let t = Temp::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "interruption_helper", "--nocapture"])
        .env("FOUNTAIN_EVAL_TEST_INTERRUPT_DIR", &t.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !t.0.join("interrupt.stdout").is_file()
        || fs::read_to_string(t.0.join("interrupt.stdout"))
            .unwrap_or_default()
            .is_empty()
    {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(child.wait().unwrap().success());
    let descendant = fs::read_to_string(t.0.join("interrupt.stdout")).unwrap();
    assert!(
        gone(descendant.trim()),
        "interrupted descendant remained alive"
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}
