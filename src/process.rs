use crate::util::Result;
use std::{
    fs::{self, File},
    path::Path,
    process::{Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static HANDLER: OnceLock<std::result::Result<(), String>> = OnceLock::new();
pub fn install_handler() -> Result<()> {
    HANDLER
        .get_or_init(|| {
            ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst))
                .map_err(|e| e.to_string())
        })
        .clone()
}
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}
pub fn controlled(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("CARGO_") && name != "CARGO_HOME"
            || matches!(
                name.as_ref(),
                "RUSTFLAGS"
                    | "RUSTDOCFLAGS"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "RUSTC"
                    | "RUSTDOC"
            )
        {
            command.env_remove(key);
        }
    }
    command
        .env("RUSTUP_TOOLCHAIN", "1.89.0")
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", "-C target-cpu=native")
        .env("CARGO_TERM_COLOR", "never");
}
/// Capture files incrementally and stop the complete child process group on failure.
pub fn run(
    command: &mut Command,
    directory: &Path,
    label: &str,
    timeout: Duration,
) -> Result<String> {
    if interrupted() {
        return Err("interrupted".into());
    }
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let stdout = directory.join(format!("{label}.stdout"));
    let stderr = directory.join(format!("{label}.stderr"));
    command
        .stdout(Stdio::from(
            File::create(&stdout).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            File::create(&stderr).map_err(|e| e.to_string())?,
        ))
        .stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("{label}: cannot start process: {e}"))?;
    let mut guard = ChildGroup(child);
    let child = &mut guard.0;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "{label}: process exited {status}; see {}",
                    stderr.display()
                ));
            }
            return fs::read_to_string(stdout).map_err(|e| e.to_string());
        }
        if interrupted() || start.elapsed() > timeout {
            return Err(if interrupted() {
                "interrupted".into()
            } else {
                format!("{label}: timeout")
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
struct ChildGroup(std::process::Child);
impl Drop for ChildGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let group = format!("-{}", self.0.id());
            if Command::new("kill")
                .args(["-TERM", &group])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
            {
                std::thread::sleep(Duration::from_millis(100));
                let _ = Command::new("kill")
                    .args(["-KILL", &group])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    controlled(&mut command);
    let output = command.output().map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
