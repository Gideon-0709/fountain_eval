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
            Duration::from_secs(5),
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
    let mut observed = 0;
    let result = process::run_observed(
        Command::new("sh").args(["-c", "sleep 30 & echo $!; wait"]),
        std::path::Path::new(&path),
        "interrupt",
        Duration::from_secs(30),
        Some(&mut |_| observed += 1),
    );
    assert_eq!(result.unwrap_err(), "interrupted");
    assert_eq!(observed, 1);
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

#[test]
fn observer_receives_complete_lines_before_exit_and_keeps_raw_logs() {
    let t = Temp::new();
    let mut command = Command::new("sh");
    command.current_dir(&t.0).args(["-c", "printf 'rea'; sleep 0.04; printf 'dy\\n'; while [ ! -f continue ]; do sleep 0.01; done; printf 'done\\n'; printf 'diagnostic\\n' >&2"]);
    let mut lines = Vec::new();
    let mut observe = |line: &str| {
        lines.push(line.to_owned());
        if line == "ready" {
            fs::write(t.0.join("continue"), "observed before child exit").unwrap();
        }
    };
    let raw = process::run_observed(
        &mut command,
        &t.0,
        "stream",
        Duration::from_secs(5),
        Some(&mut observe),
    )
    .unwrap();
    assert_eq!(lines, vec!["ready", "done"]);
    assert_eq!(raw, "ready\ndone\n");
    assert_eq!(fs::read_to_string(t.0.join("stream.stdout")).unwrap(), raw);
    assert_eq!(
        fs::read_to_string(t.0.join("stream.stderr")).unwrap(),
        "diagnostic\n"
    );
}

#[test]
fn observer_drains_failure_and_large_completed_output() {
    let t = Temp::new();
    let mut lines = Vec::new();
    let error = process::run_observed(
        Command::new("sh").args(["-c", "printf 'fatal\\n'; exit 7"]),
        &t.0,
        "failed",
        Duration::from_secs(5),
        Some(&mut |s| lines.push(s.to_owned())),
    )
    .unwrap_err();
    assert!(error.contains("exited"));
    assert_eq!(lines, vec!["fatal"]);
    let raw = t.write("large.txt", &("x".repeat(2000) + "\n").repeat(1000));
    let mut count = 0;
    process::run_observed(
        Command::new("cat").arg(raw),
        &t.0,
        "large",
        Duration::from_secs(5),
        Some(&mut |s| {
            assert_eq!(s.len(), 2000);
            count += 1;
        }),
    )
    .unwrap();
    assert_eq!(count, 1000);
}

#[test]
fn continuous_observed_output_does_not_starve_timeout() {
    let t = Temp::new();
    let start = Instant::now();
    let mut observed = 0;
    let error = process::run_observed(
        Command::new("sh").args(["-c", "while :; do printf 'active\\n'; done"]),
        &t.0,
        "noisy",
        Duration::from_millis(100),
        Some(&mut |_| observed += 1),
    )
    .unwrap_err();
    assert!(error.contains("timeout"));
    assert!(observed > 0);
    assert!(start.elapsed() < Duration::from_secs(5));
}
