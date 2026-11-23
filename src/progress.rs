//! Stage-level CLI progress. Native drivers and measured regions are unchanged.
use crate::{
    batch, content,
    evidence::Record,
    protocol::{ComponentReference, SourceReference},
    validation::Severity,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::{IsTerminal, Write as _},
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static ACTIVITY: OnceLock<Mutex<Instant>> = OnceLock::new();
const INTERVAL: Duration = Duration::from_secs(30);
/// The CLI enables progress; library execution is silent unless explicitly enabled.
pub fn set_enabled(value: bool) {
    ENABLED.store(value, Ordering::Relaxed);
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn concise(text: &str) -> String {
    let first = text.lines().next().unwrap_or("See the report for details");
    let mut text: String = first.chars().take(180).collect();
    if first.chars().count() > 180 {
        text.push_str("...");
    }
    clean(&text)
}
struct Display {
    interactive: bool,
    color: bool,
    live: bool,
    group: String,
    phase_start: Instant,
    prepare_only: bool,
}
fn display() -> &'static Mutex<Display> {
    static DISPLAY: OnceLock<Mutex<Display>> = OnceLock::new();
    DISPLAY.get_or_init(|| {
        let interactive = std::io::stderr().is_terminal()
            && std::env::var_os("CI").is_none()
            && std::env::var("TERM").as_deref() != Ok("dumb");
        Mutex::new(Display {
            interactive,
            color: interactive && std::env::var_os("NO_COLOR").is_none(),
            live: false,
            group: String::new(),
            phase_start: Instant::now(),
            prepare_only: false,
        })
    })
}
fn emit(text: &str, color: &str, live: bool) {
    if !enabled() {
        return;
    }
    let Ok(mut ui) = display().lock() else { return };
    let text = clean(text);
    let mut stderr = std::io::stderr().lock();
    if ui.live {
        let _ = write!(stderr, "\r\x1b[2K");
    }
    ui.live = live && ui.interactive;
    if ui.color {
        let _ = write!(stderr, "\x1b[{color}m");
    }
    if ui.live {
        // Clip at the right edge even when the user resizes a narrow terminal.
        let _ = write!(stderr, "\x1b[?7l{text}\x1b[?7h");
    } else {
        let _ = write!(stderr, "{text}");
    }
    if ui.color {
        let _ = write!(stderr, "\x1b[0m");
    }
    if !ui.live {
        let _ = writeln!(stderr);
    }
    let _ = stderr.flush();
}
pub fn line(text: impl AsRef<str>) {
    emit(text.as_ref(), "0", false);
}
fn heading(text: &str) {
    line("");
    emit(&format!("━━ {text} ━━"), "1;36", false);
}
pub fn live(text: &str) {
    emit(text, "36", true);
}
pub fn preparation_step(name: &str) {
    let step = match name {
        "resolve" => "Resolve and freeze sources",
        "build" => "Build native drivers",
        "probe" => "Check driver capabilities",
        _ => return,
    };
    line(format!("    › {step}"));
}
fn reset_activity() {
    if let Ok(mut last) = ACTIVITY.get_or_init(|| Mutex::new(Instant::now())).lock() {
        *last = Instant::now();
    }
}
fn interval() -> Duration {
    if display().lock().is_ok_and(|s| s.interactive) {
        Duration::from_secs(1)
    } else {
        INTERVAL
    }
}
fn due(last: &mut Instant, now: Instant, interval: Duration) -> bool {
    if now.duration_since(*last) < interval {
        return false;
    }
    *last = now;
    true
}
/// One shared interval spans worker/fresh-process boundaries and lock waits.
pub fn heartbeat_due() -> bool {
    enabled()
        && ACTIVITY
            .get_or_init(|| Mutex::new(Instant::now()))
            .lock()
            .is_ok_and(|mut last| due(&mut last, Instant::now(), interval()))
}
pub(crate) fn source(reference: Option<&ComponentReference>) -> String {
    let Some(reference) = reference else {
        return "not selected".into();
    };
    let text = match &reference.source {
        SourceReference::Path { path } => {
            let path = if path.file_name().is_some_and(|p| p == "Cargo.toml") {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            format!(
                "{} (local)",
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
            )
        }
        SourceReference::Registry {
            package, version, ..
        } => format!("{package}@{version}"),
        SourceReference::Git {
            package, revision, ..
        } => format!("{package}@{}", &revision[..revision.len().min(12)]),
    };
    match &reference.implementation {
        Some(implementation) => format!("{text} / {implementation}"),
        None => text,
    }
}
fn status(raw: &str, warnings: usize) -> String {
    match raw {
        "succeeded" if warnings > 0 => "PASSED WITH WARNINGS".into(),
        "succeeded" => "PASSED".into(),
        "completed_with_unsupported" => "COMPLETED WITH UNSUPPORTED COMPONENTS".into(),
        _ => raw.replace('_', " ").to_uppercase(),
    }
}
pub fn batch_started(r: &batch::Record) {
    if let Ok(mut ui) = display().lock() {
        ui.prepare_only = r.action == "prepare";
    }
    if let Some(e) = r.entries.first() {
        heading(&format!("Fountain evaluation · {}", r.action));
        line(format!(
            "  Suite {}    Scheme {}    Components {}",
            e.request.test.version,
            e.request.scheme.name.to_uppercase(),
            r.entries.len()
        ));
    }
}
pub fn component_started(index: usize, total: usize, entry: &batch::Entry) {
    reset_activity();
    heading(&format!(
        "C{} [{}/{}] · Component",
        entry.component_id,
        index + 1,
        total
    ));
    line(format!(
        "  Engine:   {}",
        source(entry.request.components.engine.as_ref())
    ));
    line(format!(
        "  Operator: {}",
        source(entry.request.components.operator.as_ref())
    ));
}
pub fn stage(name: &str) -> &'static str {
    match name {
        "validate" => "Validation",
        "measure" | "first-use" => "Performance",
        "diagnose" | "complete" => "Diagnostics",
        _ => "Preparation",
    }
}
pub fn phase_started(name: &str) {
    reset_activity();
    let label = stage(name);
    let (index, action) = match label {
        "Validation" => (2, "validate outputs and boundary behavior"),
        "Performance" => (3, "measure operation, link and workflow tests"),
        "Diagnostics" => (4, "trace codec stages and resource use"),
        _ => (1, "resolve, build and probe"),
    };
    let total = display().lock().map_or(4, |mut ui| {
        ui.group.clear();
        ui.phase_start = Instant::now();
        if ui.prepare_only { 1 } else { 4 }
    });
    emit(
        &format!("  [{index}/{total}] {label} — {action}"),
        "1;34",
        false,
    );
}
fn v_failed(r: &Record) -> bool {
    r.validation.iter().any(|e| {
        e["severity"] == "fatal"
            && e["status"] == "failed"
            && r.recipe
                .as_ref()
                .and_then(|p| p.checks.get(e["case"].as_str().unwrap_or("")))
                .is_some_and(|c| c.spec.validation_family().is_some())
    })
}
fn totals(r: &Record, name: &str) -> (usize, usize, &'static str) {
    let Some(p) = &r.recipe else {
        return (0, 0, "");
    };
    match name {
        "validate" if v_failed(r) => (
            r.validation
                .iter()
                .filter(|e| {
                    p.checks
                        .get(e["case"].as_str().unwrap_or(""))
                        .is_some_and(|c| c.spec.validation_family().is_some())
                })
                .count(),
            p.checks
                .values()
                .filter(|c| c.skip.is_none() && c.spec.validation_family().is_some())
                .count(),
            "V checks",
        ),
        "validate" => (r.validation.len(), p.validation_cases().len(), "checks"),
        "measure" | "first-use" => (r.samples.len(), p.cases.len() * p.repetitions, "samples"),
        "diagnose" | "complete" => (
            r.diagnostics.len(),
            p.checks
                .values()
                .filter(|c| {
                    c.skip.is_none()
                        && c.measured
                        && !c.spec.fresh_process
                        && c.spec.category != "operation"
                })
                .count()
                * content::WORKERS,
            "observations",
        ),
        _ => (0, 0, ""),
    }
}
pub fn phase_finished(r: &Record, name: &str, success: bool) {
    let seconds = r
        .timing
        .phases_ns
        .iter()
        .filter(|(key, _)| stage(key) == stage(name))
        .map(|(_, ns)| *ns as f64 / 1e9)
        .sum::<f64>();
    let (done, total, unit) = totals(r, name);
    let mut text = format!(
        "  {}  {}",
        if success { "DONE" } else { "STOP" },
        stage(name)
    );
    if !unit.is_empty() {
        let _ = write!(text, ": {done}/{total} {unit}");
    }
    if name == "validate" && !r.correctness.warnings.is_empty() {
        let _ = write!(text, " | {} warnings", r.correctness.warnings.len());
    }
    if name == "validate" && v_failed(r) {
        let _ = write!(
            text,
            " | {} fatal | O/L/W blocked",
            r.correctness.fatal_failures.len()
        );
    }
    if name == "measure" {
        let incomplete = r
            .samples
            .iter()
            .filter(|s| s.outcome != "completed")
            .count();
        if incomplete > 0 {
            let _ = write!(text, " | {incomplete} incomplete outcomes");
        }
    }
    let _ = write!(text, " | {seconds:.1} s");
    emit(&text, if success { "32" } else { "1;31" }, false);
    if name == "validate" && success && !r.correctness.warnings.is_empty() {
        line("  WARN  Boundary deviations recorded; performance continues. Details in report.");
    }
}
pub fn plan(r: &Record) {
    let Some(plan) = &r.recipe else {
        return;
    };
    let v = plan
        .checks
        .values()
        .filter(|c| c.skip.is_none() && c.spec.validation_family().is_some())
        .count();
    line(format!(
        "  PLAN  {v} V checks | {} performance prechecks | {} measured cases",
        plan.validation_cases().len() - v,
        plan.cases.len()
    ));
    let mut reasons = BTreeMap::<String, usize>::new();
    let mut measurement_only = 0;
    for check in plan.checks.values() {
        if let Some(skip) = &check.skip {
            *reasons.entry(skip.code.replace('_', " ")).or_default() += 1;
        } else if check.measurement_skip.is_some() {
            measurement_only += 1;
        }
    }
    if !reasons.is_empty() {
        line(format!(
            "  SKIP  {} cases ({})",
            reasons.values().sum::<usize>(),
            reasons
                .iter()
                .map(|(reason, count)| format!("{reason}: {count}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if measurement_only > 0 {
        line(format!(
            "  SKIP  {measurement_only} additional measurement exclusions; details in report"
        ));
    }
}
pub fn component_finished(r: &Record) {
    let success = matches!(r.status.as_str(), "succeeded" | "prepared");
    phase_finished(r, &r.stage, success);
    let mut text = format!(
        "C{} {} | {:.1} s",
        r.configuration.component_id,
        status(&r.status, r.correctness.warnings.len()),
        r.timing.elapsed_ns as f64 / 1e9
    );
    if !r.correctness.warnings.is_empty() {
        let _ = write!(text, " | {} warnings", r.correctness.warnings.len());
    }
    if !r.correctness.fatal_failures.is_empty() {
        let _ = write!(
            text,
            " | fatal failures: {}",
            r.correctness.fatal_failures.len()
        );
    }
    if !success {
        let (done, total, _) = totals(r, "validate");
        let missing = total.saturating_sub(done);
        if missing > 0 {
            let _ = write!(text, " | {missing} checks not executed");
        }
    }
    emit(
        &text,
        if !success {
            "1;31"
        } else if r.correctness.warnings.is_empty() {
            "1;32"
        } else {
            "1;33"
        },
        false,
    );
    if let Some(error) = &r.error {
        line(format!(
            "  Reason: {}",
            concise(error.split("; see ").next().unwrap_or(error))
        ));
    }
}
pub fn summary(r: &batch::Record, path: &Path) -> String {
    let mut text = format!(
        "\n━━ Evaluation summary ━━\nRESULT: {} | {:.1} s\n",
        status(&r.status, r.entries.iter().map(|e| e.warnings).sum()),
        r.elapsed_ns as f64 / 1e9
    );
    for e in &r.entries {
        let _ = writeln!(
            text,
            "  C{}  {} | {} warnings | {} skipped",
            e.component_id,
            status(&e.status, e.warnings),
            e.warnings,
            e.skipped
        );
    }
    if r.action == "prepare" {
        let _ = writeln!(
            text,
            "\nPrepared workspace: {}\nContinue: feval run --prepared '{}'",
            clean(&path.display().to_string()),
            clean(&r.run_id)
        );
    } else {
        let _ = writeln!(
            text,
            "\nResult: {}\nGenerate report: feval report {}",
            clean(&path.display().to_string()),
            clean(&r.run_id)
        );
    }
    text
}

/// Aggregate one invocation's events; final protocol/evidence validation is authoritative.
pub struct DriverProgress<'a> {
    record: &'a Record,
    mode: &'a str,
    worker: usize,
    eligible: BTreeSet<String>,
    seen: BTreeSet<(String, usize)>,
    received: usize,
    warnings: usize,
    fatals: usize,
    incomplete: usize,
    hello: bool,
    invalid: bool,
    category: Option<&'static str>,
}
impl<'a> DriverProgress<'a> {
    pub fn new(record: &'a Record, mode: &'a str, worker: usize, selected: Option<&str>) -> Self {
        let eligible = record
            .recipe
            .as_ref()
            .map(|p| {
                p.checks
                    .iter()
                    .filter(|(id, c)| {
                        c.skip.is_none()
                            && match mode {
                                "validate" => true,
                                "measure" => {
                                    c.measured
                                        && c.measurement_skip.is_none()
                                        && !c.spec.fresh_process
                                }
                                "first-use" => {
                                    c.measured
                                        && c.measurement_skip.is_none()
                                        && c.spec.fresh_process
                                        && selected == Some(id.as_str())
                                }
                                "diagnose" => {
                                    c.measured
                                        && c.measurement_skip.is_none()
                                        && !c.spec.fresh_process
                                        && c.spec.category != "operation"
                                }
                                _ => false,
                            }
                    })
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            record,
            mode,
            worker,
            eligible,
            seen: BTreeSet::new(),
            received: 0,
            warnings: 0,
            fatals: 0,
            incomplete: 0,
            hello: false,
            invalid: false,
            category: None,
        }
    }
    fn location(&self) -> String {
        let section = self.category.unwrap_or("Starting driver");
        match self.mode {
            "first-use" => format!("{section} · fresh processes"),
            "measure" | "diagnose" => format!(
                "{section} · worker {}/{}",
                self.worker + 1,
                content::WORKERS
            ),
            _ => section.to_owned(),
        }
    }
    fn section(&mut self, category: &'static str) {
        if self.category == Some(category) {
            return;
        }
        self.category = Some(category);
        if !enabled() {
            return;
        }
        let label = self.location();
        let changed = display().lock().is_ok_and(|mut ui| {
            if ui.group == label {
                false
            } else {
                ui.group = label.clone();
                true
            }
        });
        if changed {
            line(format!("    › {label}"));
        }
    }
    pub fn heartbeat(&self) -> String {
        let label = stage(self.mode);
        if self.invalid {
            return format!("  LIVE  {label}: driver reported an error; waiting for process exit");
        }
        if self.mode == "describe" {
            return "  LIVE  Preparation: checking native capabilities".into();
        }
        let (previous, mut total, unit) = totals(self.record, self.mode);
        if self.mode == "validate" && self.fatals > 0 {
            total = self.record.recipe.as_ref().map_or(0, |p| {
                p.checks
                    .values()
                    .filter(|c| c.skip.is_none() && c.spec.validation_family().is_some())
                    .count()
            });
        }
        let mut text = format!(
            "  LIVE  {label}: {}/{total} {unit} observed",
            previous + self.received
        );
        if self.warnings > 0 {
            let _ = write!(text, " | {} warnings", self.warnings);
        }
        if self.fatals > 0 {
            let _ = write!(text, " | {} fatal; continuing V checks", self.fatals);
        }
        let incomplete = self.incomplete
            + if matches!(self.mode, "measure" | "first-use") {
                self.record
                    .samples
                    .iter()
                    .filter(|s| s.outcome != "completed")
                    .count()
            } else {
                0
            };
        if incomplete > 0 {
            let _ = write!(text, " | {incomplete} incomplete outcomes");
        }
        let done = previous + self.received;
        let percent = if total > 0 {
            done as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        let seconds = display()
            .lock()
            .map_or(0.0, |ui| ui.phase_start.elapsed().as_secs_f64());
        let _ = write!(
            text,
            " · {percent:.0}% · {} · {seconds:.0}s",
            self.location()
        );
        if self.mode == "first-use" {
            text.push_str(" · first-use sampling");
        }
        text
    }
    fn reject(&mut self, reason: &str) -> Option<String> {
        self.invalid = true;
        Some(format!(
            "  ERROR {}: {reason}; details in driver logs",
            stage(self.mode)
        ))
    }
    pub fn observe(&mut self, line: &str) -> Option<String> {
        if self.invalid {
            return None;
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return self.reject("invalid driver output");
        };
        if !self.hello {
            if self
                .record
                .built
                .as_ref()
                .is_none_or(|b| crate::execution::check_hello(&event, b, self.record).is_err())
                || event["worker"] != self.worker
                || event["diagnostic"] != (self.mode == "diagnose")
            {
                return self.reject("driver identity mismatch");
            }
            self.hello = true;
            return None;
        }
        let kind = event["event"].as_str().unwrap_or("");
        if matches!(kind, "done" | "error") {
            return None;
        }
        let Some(case) = event["case"].as_str() else {
            return self.reject("event has no case ID");
        };
        if !self.eligible.contains(case) {
            return self.reject("event is outside the test plan");
        }
        let check = &self.record.recipe.as_ref().unwrap().checks[case];
        let category = match check.spec.category.as_str() {
            "correctness" => "Validation tests",
            "operation" => "Operation tests",
            "chain" if check.spec.workload.starts_with("plan-") => "Encoding plans",
            "chain" => "Link tests",
            "workflow" if self.mode == "first-use" => "First-use encoding",
            "workflow" => "Workflow tests",
            _ => "Other workloads",
        };
        let integrity_failure = kind == "validation"
            && event["severity"] == "fatal"
            && event["status"] == "failed"
            && event["rule"] == "execution-integrity"
            && event["message"].as_str().is_some();
        if integrity_failure
            && (self.mode != "validate" || check.spec.validation_family().is_none())
        {
            self.invalid = true;
            return Some(format!(
                "  FATAL {}: {}",
                stage(self.mode),
                concise(
                    event["message"]
                        .as_str()
                        .unwrap_or("driver execution failed")
                )
            ));
        }
        if self.mode == "validate" && kind == "validation" {
            if (!integrity_failure
                && (event["severity"] != serde_json::to_value(check.severity).unwrap()
                    || event["rule"] != check.rule))
                || !matches!(event["status"].as_str(), Some("passed" | "failed"))
                || !self.seen.insert((case.into(), 0))
            {
                return self.reject("inconsistent or repeated validation event");
            }
            self.received += 1;
            if event["status"] == "failed" {
                if check.severity == Severity::Warning && !integrity_failure {
                    self.warnings += 1;
                } else {
                    self.fatals += 1;
                    if self.fatals == 1 {
                        return Some(format!(
                            "  FATAL Validation: {}; continuing remaining V checks",
                            concise(
                                event["message"]
                                    .as_str()
                                    .unwrap_or("check failed; details in report")
                            )
                        ));
                    }
                }
            }
            self.section(if check.spec.validation_family().is_some() {
                "Validation tests (V01–V11)"
            } else {
                category
            });
            return None;
        }
        if matches!(self.mode, "measure" | "first-use") && kind == "sample" {
            let Some(rep) = event["repetition"].as_u64().map(|v| v as usize) else {
                return self.reject("sample repetition missing");
            };
            let expected_worker = if self.mode == "first-use" {
                rep
            } else {
                rep / content::SAMPLES_PER_WORKER
            };
            if rep >= self.record.recipe.as_ref().unwrap().repetitions
                || event["worker"] != self.worker
                || self.worker != expected_worker
                || !self.seen.insert((case.into(), rep))
            {
                return self.reject("sample worker mismatch or duplicate");
            }
            if event["elapsed_ns"].as_u64().is_none_or(|ns| ns == 0) {
                return self.reject("invalid sample duration");
            }
            self.received += 1;
            self.incomplete += usize::from(event["outcome"] != "completed");
            self.section(category);
            return None;
        }
        if self.mode == "diagnose" && kind == "diagnostic" {
            if event["worker"] != self.worker || !self.seen.insert((case.into(), 0)) {
                return self.reject("diagnostic worker mismatch or duplicate");
            }
            self.received += 1;
            self.section(category);
            return None;
        }
        self.reject("unexpected event for this stage")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heartbeat_interval_does_not_depend_on_event_or_process_count() {
        let start = Instant::now();
        let mut last = start;
        for millis in 0..30_000 {
            assert!(!due(
                &mut last,
                start + Duration::from_millis(millis),
                INTERVAL
            ));
        }
        assert!(due(&mut last, start + INTERVAL, INTERVAL));
        assert!(!due(&mut last, start + INTERVAL, INTERVAL));
        assert!(!due(&mut last, start + Duration::from_secs(59), INTERVAL));
        assert!(due(&mut last, start + Duration::from_secs(60), INTERVAL));
    }
    #[cfg(unix)]
    #[test]
    fn process_heartbeats_share_the_interval_and_preserve_stdout() {
        let root = std::env::temp_dir().join(format!(
            "feval-heartbeat-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        set_enabled(true);
        *ACTIVITY
            .get_or_init(|| Mutex::new(Instant::now()))
            .lock()
            .unwrap() = Instant::now() - INTERVAL;
        let mut heartbeats = 0;
        let mut lines = Vec::new();
        let mut output = Vec::new();
        for label in ["worker", "fresh-process"] {
            let result = crate::process::run_observed(
                std::process::Command::new("sh").current_dir(&root).args([
                    "-c",
                    "printf 'ready\\n'; while [ ! -f release ]; do sleep 0.01; done",
                ]),
                &root,
                label,
                Duration::from_secs(5),
                Some(&mut |event| match event {
                    Some(line) => lines.push(line.to_owned()),
                    None => {
                        heartbeats += 1;
                        std::fs::write(root.join("release"), "heartbeat observed").unwrap();
                    }
                }),
            );
            output.push(result);
        }
        set_enabled(false);
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(
            heartbeats, 1,
            "Starting another process must not reset the interval"
        );
        assert_eq!(lines, vec!["ready", "ready"]);
        assert!(output.iter().all(|r| r.as_deref() == Ok("ready\n")));
    }
}
