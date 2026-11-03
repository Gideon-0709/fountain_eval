//! Live, observational CLI progress. Native drivers and their timed regions are unchanged.
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
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
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
fn failure_message(message: &str) -> String {
    let first = message.lines().next().unwrap_or("Driver execution failed");
    let mut concise: String = first.chars().take(240).collect();
    if first.chars().count() > 240 {
        concise.push_str("...");
    }
    format!(
        "{}; details in the Markdown report and raw driver logs",
        clean(&concise)
    )
}
pub fn line(text: impl AsRef<str>) {
    if enabled() {
        // A closed display stream must not turn a valid experiment into a failure.
        let _ = writeln!(std::io::stderr().lock(), "{}", clean(text.as_ref()));
    }
}
fn source(reference: Option<&ComponentReference>) -> String {
    let Some(reference) = reference else {
        return "not selected".into();
    };
    let text = match &reference.source {
        SourceReference::Path { path } => path.display().to_string(),
        SourceReference::Registry {
            package, version, ..
        } => format!("{package}@{version}"),
        SourceReference::Git {
            package, revision, ..
        } => format!("{package}@{}", &revision[..revision.len().min(12)]),
    };
    if let Some(implementation) = &reference.implementation {
        format!("{text} ({implementation})")
    } else {
        text
    }
}
pub fn batch_started(r: &batch::Record) {
    line(format!(
        "Fountain evaluation | {} | {} component selections | run {}",
        r.action,
        r.entries.len(),
        r.run_id
    ));
    if let Some(e) = r.entries.first() {
        line(format!(
            "Test {} | scheme {} | all applicable cases",
            e.request.test.version, e.request.scheme.name
        ));
    }
    line(
        "Live rows describe observed checks and collected samples; each C is accepted only after final evidence validation.",
    );
}
pub fn component_started(index: usize, total: usize, entry: &batch::Entry) {
    line(format!(
        "[C{} {}/{}] START | engine: {} | operator: {}",
        entry.component_id,
        index + 1,
        total,
        source(entry.request.components.engine.as_ref()),
        source(entry.request.components.operator.as_ref())
    ));
}
pub fn phase(id: u32, name: &str) {
    let label = match name {
        "resolve" => "Resolve and freeze component inputs",
        "build" => "Build or verify cached native drivers",
        "probe" => "Check interface and native capabilities",
        "validate" => "Correctness checks",
        "measure" => "Ordinary performance measurements",
        "diagnose" => "Separate stage and resource diagnostics",
        _ => name,
    };
    line(format!("[C{id}] {label}"));
}
pub fn plan(r: &Record) {
    let Some(plan) = &r.recipe else {
        return;
    };
    let mut skipped = BTreeMap::<(&str, &str, &str), usize>::new();
    for c in plan.checks.values() {
        if let Some(skip) = &c.skip {
            *skipped
                .entry(("validation", &skip.code, &skip.reason))
                .or_default() += 1;
        }
        if let Some(skip) = c.skip.as_ref().or(c.measurement_skip.as_ref()) {
            *skipped
                .entry(("measurement", &skip.code, &skip.reason))
                .or_default() += 1;
        }
    }
    line(format!(
        "[C{}] PLAN | {} registered cases; {} applicable checks; {} measured cases",
        r.configuration.component_id,
        plan.checks.len(),
        plan.validation_cases().len(),
        plan.cases.len()
    ));
    for ((phase, code, reason), count) in skipped {
        line(format!(
            "[C{}][SKIP] {count} {phase} cases | {code}: {reason}",
            r.configuration.component_id
        ));
    }
}
pub fn component_finished(r: &Record) {
    let applicable = r.recipe.as_ref().map_or(0, |p| p.validation_cases().len());
    let not_executed = applicable.saturating_sub(r.validation.len());
    line(format!(
        "[C{}] {} | last phase: {} | checks {}/{}; warnings {}; fatal failures {}; not executed {}; samples {} | {:.2} s",
        r.configuration.component_id,
        r.status.to_uppercase(),
        r.stage,
        r.validation.len(),
        applicable,
        r.correctness.warnings.len(),
        r.correctness.fatal_failures.len(),
        not_executed,
        r.samples.len(),
        r.timing.elapsed_ns as f64 / 1e9
    ));
    if let Some(error) = &r.error {
        line(format!("[C{}] {}", r.configuration.component_id, error));
    }
}
pub fn summary(r: &batch::Record, evidence: &Path) -> String {
    let mut text = format!(
        "\nEvaluation {} | {} component selections | {:.2} s\n",
        clean(&r.status),
        r.entries.len(),
        r.elapsed_ns as f64 / 1e9
    );
    for e in &r.entries {
        let _ = writeln!(
            text,
            "  C{}: {} | checks {}; warnings {}; samples {}",
            e.component_id,
            clean(&e.status),
            e.validation_observations,
            e.warnings,
            e.samples
        );
    }
    let _ = writeln!(
        text,
        "Report: {}\nEvidence: {}",
        clean(
            &r.configuration
                .report_root()
                .join(&r.run_id)
                .join("report.md")
                .display()
                .to_string()
        ),
        clean(&evidence.display().to_string())
    );
    text
}
fn duration(ns: u64) -> String {
    let (scale, unit) = match ns {
        0..=999 => (1.0, "ns"),
        1_000..=999_999 => (1e3, "us"),
        1_000_000..=999_999_999 => (1e6, "ms"),
        _ => (1e9, "s"),
    };
    format!("{:.3} {unit}", ns as f64 / scale)
}
fn category(c: &content::Case) -> &'static str {
    match c.category.as_str() {
        "correctness" => "Correctness",
        "operation" => "Operation",
        "chain" => "Chain",
        "workflow" => "Workflow",
        _ => "Test",
    }
}
fn case_name(c: &content::Case) -> String {
    // IDs retain the complete dimensions, while the leading name is readable.
    format!(
        "{} / {} | {}",
        category(c),
        c.workload.replace('-', " "),
        c.id
    )
}
#[derive(Default)]
struct Samples {
    times: Vec<u64>,
    incomplete: usize,
    warnings: bool,
}
/// One invocation's live event view; the sealed execution pipeline remains authoritative.
pub struct DriverProgress<'a> {
    record: &'a Record,
    mode: &'a str,
    worker: usize,
    eligible: BTreeSet<String>,
    completed: BTreeSet<String>,
    repetitions: BTreeSet<(String, usize)>,
    samples: BTreeMap<String, Samples>,
    diagnostics: BTreeMap<String, usize>,
    hello: bool,
    invalid: bool,
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
        let mut out = Self {
            record,
            mode,
            worker,
            eligible,
            completed: BTreeSet::new(),
            repetitions: BTreeSet::new(),
            samples: BTreeMap::new(),
            diagnostics: BTreeMap::new(),
            hello: false,
            invalid: false,
        };
        for sample in &record.samples {
            out.repetitions
                .insert((sample.case.clone(), sample.repetition));
            let state = out.samples.entry(sample.case.clone()).or_default();
            state.times.push(sample.elapsed_ns);
            state.incomplete += usize::from(sample.outcome != "completed");
            if let Some(actual) = &sample.observation
                && let Some(check) = record.validation.iter().find(|v| v["case"] == sample.case)
            {
                state.warnings |=
                    !crate::validation::observation_matches(actual, &check["expected"]);
            }
        }
        for d in &record.diagnostics {
            if let Some(case) = d["case"].as_str() {
                *out.diagnostics.entry(case.into()).or_default() += 1;
            }
        }
        out
    }
    pub fn start(&self) -> Option<String> {
        let id = self.record.configuration.component_id;
        match self.mode {
            "validate" => Some(format!(
                "[C{id}][VALIDATE] 0/{} applicable checks",
                self.eligible.len()
            )),
            "measure" => Some(format!(
                "[C{id}][MEASURE] worker {}/{} | {} cases | {} samples/case after warm-up",
                self.worker + 1,
                content::WORKERS,
                self.eligible.len(),
                content::SAMPLES_PER_WORKER
            )),
            "diagnose" => Some(format!(
                "[C{id}][DIAGNOSE] worker {}/{} | {} cases",
                self.worker + 1,
                content::WORKERS,
                self.eligible.len()
            )),
            _ => None,
        }
    }
    fn reject(&mut self, reason: &str) -> Option<String> {
        self.invalid = true;
        Some(format!(
            "[C{}][PROTOCOL] {reason}; live case progress stopped; see raw driver logs",
            self.record.configuration.component_id
        ))
    }
    pub fn observe(&mut self, line: &str) -> Option<String> {
        if self.invalid {
            return None;
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return self.reject("Invalid JSON event");
        };
        if !self.hello {
            if self.record.built.as_ref().is_none_or(|built| {
                crate::execution::check_hello(&event, built, self.record).is_err()
            }) || event["worker"] != self.worker
                || event["diagnostic"] != (self.mode == "diagnose")
            {
                return self.reject("Driver identity mismatch");
            }
            self.hello = true;
            return None;
        }
        let kind = event["event"].as_str().unwrap_or("");
        if matches!(kind, "done" | "error") {
            return None;
        }
        let Some(case) = event["case"].as_str() else {
            return self.reject("Event has no case ID");
        };
        if !self.eligible.contains(case) {
            return self.reject("Event is outside this invocation's test plan");
        }
        let check = &self.record.recipe.as_ref().unwrap().checks[case];
        let name = case_name(&check.spec);
        let id = self.record.configuration.component_id;
        // Panic diagnostics can appear while measuring; they remain fatal, never a collected sample.
        if kind == "validation"
            && event["severity"] == "fatal"
            && event["status"] == "failed"
            && event["rule"] == "execution-integrity"
        {
            return Some(format!(
                "[C{id}][FATAL] {name} | {}",
                failure_message(
                    event["message"]
                        .as_str()
                        .unwrap_or("Driver execution failed")
                )
            ));
        }
        if self.mode == "validate" && kind == "validation" {
            if event["severity"] != serde_json::to_value(check.severity).unwrap()
                || event["rule"] != check.rule
                || !matches!(event["status"].as_str(), Some("passed" | "failed"))
                || !self.completed.insert(case.into())
            {
                return self.reject("Inconsistent or repeated validation event");
            }
            let status = if event["status"] == "passed" {
                "PASS"
            } else if check.severity == Severity::Warning {
                "WARN"
            } else {
                "FATAL"
            };
            let explanation = if status == "WARN" {
                " | boundary deviation; timing remains enabled"
            } else {
                ""
            };
            return Some(format!(
                "[C{id}][VALIDATE {}/{}][{status}] {name}{explanation}",
                self.completed.len(),
                self.eligible.len()
            ));
        }
        if matches!(self.mode, "measure" | "first-use") && kind == "sample" {
            let Some(rep) = event["repetition"].as_u64().map(|v| v as usize) else {
                return self.reject("Sample repetition missing");
            };
            let expected_worker = if self.mode == "first-use" {
                rep
            } else {
                rep / content::SAMPLES_PER_WORKER
            };
            let expected = self.record.recipe.as_ref().unwrap().repetitions;
            if rep >= expected
                || event["worker"] != self.worker
                || self.worker != expected_worker
                || !self.repetitions.insert((case.into(), rep))
            {
                return self.reject("Sample belongs to the wrong worker or is duplicated");
            }
            let Some(ns) = event["elapsed_ns"].as_u64().filter(|n| *n > 0) else {
                return self.reject("Invalid sample duration");
            };
            let state = self.samples.entry(case.into()).or_default();
            state.times.push(ns);
            state.incomplete += usize::from(event["outcome"] != "completed");
            if check.spec.warning
                && let Some(validation) = self.record.validation.iter().find(|v| v["case"] == case)
            {
                state.warnings |= validation["status"] == "failed"
                    || !crate::validation::observation_matches(
                        &event["observation"],
                        &validation["expected"],
                    );
            }
            let count = state.times.len();
            if self.mode == "first-use" && count != expected {
                return None;
            }
            if self.mode == "measure" && !(rep + 1).is_multiple_of(content::SAMPLES_PER_WORKER) {
                return None;
            }
            self.completed.insert(case.into());
            let status = if count == expected {
                "RECORDED"
            } else {
                "COLLECTED"
            };
            let phase = if self.mode == "first-use" {
                "FIRST USE"
            } else {
                "MEASURE"
            };
            let mut text = format!(
                "[C{id}][{phase} {}/{}][{status}] {name} | {count}/{expected} samples",
                self.completed.len(),
                self.eligible.len()
            );
            if count == expected {
                let mut times = state.times.clone();
                times.sort_unstable();
                let median = times[times.len() / 2];
                let _ = write!(text, " | median {}", duration(median));
                if let Some(ops) = event["operations"].as_u64().filter(|n| *n > 0) {
                    let _ = write!(text, " | {:.3} ns/call", median as f64 / ops as f64);
                }
                if state.incomplete == 0
                    && let Some(bytes) = event["payload_bytes"].as_u64().filter(|n| *n > 0)
                {
                    let _ = write!(
                        text,
                        " | {:.3} {} Gbit/s",
                        bytes as f64 * 8.0 / median as f64,
                        if check.spec.category == "operation" {
                            "logical"
                        } else {
                            "source"
                        }
                    );
                }
                if state.incomplete > 0 {
                    let _ = write!(
                        text,
                        " | {} incomplete samples; no successful throughput",
                        state.incomplete
                    );
                }
            }
            if state.warnings {
                text.push_str(" | WARN: boundary deviation retained");
            }
            Some(text)
        } else if self.mode == "diagnose" && kind == "diagnostic" {
            if event["worker"] != self.worker || !self.completed.insert(case.into()) {
                return self.reject("Diagnostic belongs to the wrong worker or is duplicated");
            }
            let count = self.diagnostics.entry(case.into()).or_default();
            *count += 1;
            Some(format!(
                "[C{id}][DIAGNOSE {}/{}][{}] {name} | {}/{} observations",
                self.completed.len(),
                self.eligible.len(),
                if *count == content::WORKERS {
                    "RECORDED"
                } else {
                    "COLLECTED"
                },
                count,
                content::WORKERS
            ))
        } else {
            self.reject("Unexpected event for the current phase")
        }
    }
}
