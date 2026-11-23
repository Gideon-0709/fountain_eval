//! Versioned correctness checks and non-blocking semantic observations.
use crate::{catalog::Recipe, evidence::Record, util::Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Fatal,
    Warning,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub spec: crate::content::Case,
    pub skip: Option<crate::coverage::Skip>,
    pub measurement_skip: Option<crate::coverage::Skip>,
    pub severity: Severity,
    pub rule: String,
    pub description: String,
    pub measured: bool,
    pub payload_bytes: u64,
    pub operations: u64,
}

/// Verify the registered check coverage. Only fatal mismatches reject execution.
pub fn verify(recipe: &Recipe, events: &[Value]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut fatal = 0;
    let mut v_fatal = false;
    for event in events {
        let case = event["case"].as_str().ok_or("validation case missing")?;
        let check = recipe.check(case).ok_or("unregistered correctness check")?;
        if check.skip.is_some() {
            return Err("skipped check produced validation evidence".into());
        }
        if !seen.insert(case.to_owned()) {
            return Err("duplicate validation case".into());
        }
        if event["status"] == "failed" {
            let violation: crate::findings::Violation =
                serde_json::from_value(event["finding"].clone())
                    .map_err(|_| "failed check is missing its structured contract/failure kind")?;
            if !violation.validate(case) {
                return Err("invalid structured correctness finding".into());
            }
        }
        let integrity_failure = event["rule"] == "execution-integrity"
            && event["severity"] == "fatal"
            && event["status"] == "failed"
            && event["message"].as_str().is_some();
        if (!integrity_failure
            && (event["severity"] != serde_json::to_value(check.severity).unwrap()
                || event["rule"] != check.rule))
            || !matches!(event["status"].as_str(), Some("passed" | "failed"))
        {
            return Err("correctness result does not match its registered severity/rule".into());
        }
        if check.severity == Severity::Warning
            && !integrity_failure
            && (event.get("expected").is_none()
                || event.get("actual").is_none()
                || event["status"]
                    != if event["expected"] == event["actual"] {
                        "passed"
                    } else {
                        "failed"
                    }
                || event["witness"] != crate::util::digest(&event["actual"]))
        {
            return Err("inconsistent semantic warning evidence".into());
        }
        if check.severity == Severity::Warning
            && !integrity_failure
            && !crate::findings::verify_boundary(
                &check.spec,
                &event["expected"],
                &event["actual"],
                &event["finding"],
            )
        {
            return Err("boundary observation does not match its structured finding".into());
        }
        if (check.severity == Severity::Fatal || integrity_failure) && event["status"] == "failed" {
            fatal += 1;
            v_fatal |= check.spec.validation_family().is_some();
        }
        if event["witness"].as_str().is_none() {
            return Err("validation witness missing".into());
        }
    }
    let expected: Vec<_> = recipe
        .validation_cases()
        .into_iter()
        .filter(|case| !v_fatal || recipe.checks[case].spec.validation_family().is_some())
        .collect();
    if events
        .iter()
        .map(|e| e["case"].as_str().unwrap())
        .ne(expected.iter().map(String::as_str))
    {
        return Err(if v_fatal {
            "incomplete or out-of-order V evidence, or O/L/W executed after V fatal".into()
        } else {
            "incomplete or out-of-order correctness evidence".into()
        });
    }
    if fatal > 0 {
        return Err(if v_fatal {
            format!(
                "{fatal} fatal correctness checks failed; all {} applicable V checks completed; O/L/W were not executed",
                expected.len()
            )
        } else {
            format!("{fatal} fatal performance prechecks failed")
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub status: String,
    pub fatal_checks: usize,
    pub warning_checks: usize,
    pub fatal_failures: Vec<Value>,
    pub warnings: Vec<Value>,
}
impl Default for Report {
    fn default() -> Self {
        Self {
            status: "not_executed".into(),
            fatal_checks: 0,
            warning_checks: 0,
            fatal_failures: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
/// Warnings retain validation observations and measured observations, including variation.
pub fn report(record: &Record) -> Report {
    let mut out = Report {
        status: "not_executed".into(),
        ..Report::default()
    };
    let Some(recipe) = &record.recipe else {
        return out;
    };
    let mut warnings = BTreeMap::new();
    for event in &record.validation {
        let case = event["case"].as_str().unwrap_or("unknown");
        let severity = recipe
            .check(case)
            .map(|c| c.severity)
            .unwrap_or(Severity::Fatal);
        if severity == Severity::Fatal || event["rule"] == "execution-integrity" {
            out.fatal_checks += 1;
            if event["status"] == "failed" {
                let mut finding = event.clone();
                if let Some(message) = event["message"].as_str() {
                    finding["message"] = message_summary(message).into();
                }
                out.fatal_failures.push(finding);
            }
        } else {
            out.warning_checks += 1;
            let observations:Vec<_> = record.samples.iter().filter(|s| s.case == case)
                .filter_map(|s| s.observation.as_ref().map(|v| json!({"repetition":s.repetition,"actual":v,"matches_reference":observation_matches(v, &event["expected"])}))).collect();
            if event["status"] == "failed"
                || observations.iter().any(|v| v["matches_reference"] == false)
            {
                let mut warning = event.clone();
                warning["status"] = json!("failed");
                warning["measured_observations"] = json!(observations);
                let mut findings = BTreeMap::new();
                for finding in
                    std::iter::once(&event["finding"]).chain(observations.iter().flat_map(|o| {
                        o["actual"]["outcomes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|o| &o["finding"])
                    }))
                {
                    if !finding.is_null() {
                        findings.insert(crate::util::digest(finding), finding.clone());
                    }
                }
                warning["findings"] = json!(findings.into_values().collect::<Vec<_>>());
                warnings.insert(case.to_owned(), warning);
            }
        }
    }
    out.warnings = warnings.into_values().collect();
    if let Some(error) = &record.error
        && matches!(record.stage.as_str(), "validate" | "measure" | "diagnose")
        && out.fatal_failures.is_empty()
    {
        out.fatal_failures.push(json!({"severity":"fatal","rule":"execution-integrity","phase":record.stage,"message":error,
            "finding":crate::findings::Violation {contract_id:format!("execution.{}",record.stage),failure_kind:crate::findings::FailureKind::ExecutionFailure,
                requirement:"The driver must complete the requested execution stage.".into(),context:json!({"case":"","phase":record.stage}),difference:None}}));
    }
    out.status = if !out.fatal_failures.is_empty() {
        "failed"
    } else if record.validation.is_empty() {
        "not_executed"
    } else if record.validation.len() != recipe.validation_cases().len() {
        "incomplete"
    } else if !out.warnings.is_empty() {
        "passed_with_warnings"
    } else {
        "passed"
    }
    .into();
    out
}

/// Boundary batches report every distinct result and its occurrence count.
pub fn observation_matches(observation: &Value, expected: &Value) -> bool {
    observation["outcomes"]
        .as_array()
        .is_some_and(|items| !items.is_empty() && items.iter().all(|v| v["actual"] == *expected))
}
pub fn verify_observation(observation: &Value, calls: u64) -> Result<()> {
    if observation["calls"].as_u64() != Some(calls) {
        return Err("boundary batch call count mismatch".into());
    }
    let items = observation["outcomes"]
        .as_array()
        .ok_or("boundary batch outcomes missing")?;
    let mut total = 0u64;
    let mut seen = BTreeSet::new();
    for item in items {
        let count = item["count"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("invalid boundary outcome count")?;
        let actual = item.get("actual").ok_or("boundary outcome data missing")?;
        if !seen.insert(crate::util::digest(actual)) {
            return Err("duplicate boundary outcome group".into());
        }
        total = total
            .checked_add(count)
            .ok_or("boundary call count overflow")?;
    }
    if total != calls {
        return Err("boundary outcomes do not cover every measured call".into());
    }
    Ok(())
}

/// Keep the actual mismatch, without copying a multi-megabyte Rust debug byte array into a report.
pub(crate) fn message_summary(message: &str) -> String {
    if message.len() <= 16_384 {
        return message.to_owned();
    }
    let bytes = |key: &str| -> Option<Vec<u8>> {
        let line = message
            .lines()
            .find(|line| line.trim_start().starts_with(key))?;
        serde_json::from_str(line.trim_start().strip_prefix(key)?.trim()).ok()
    };
    if let (Some(left), Some(right)) = (bytes("left:"), bytes("right:")) {
        let first = left
            .iter()
            .zip(&right)
            .position(|(a, b)| a != b)
            .or_else(|| (left.len() != right.len()).then_some(left.len().min(right.len())));
        let differing = left.iter().zip(&right).filter(|(a, b)| a != b).count()
            + left.len().abs_diff(right.len());
        let context = message
            .lines()
            .take_while(|line| !line.trim_start().starts_with("left:"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = format!(
            "{}\nByte-array comparison: left {} bytes; right {} bytes; {} differing offsets.",
            bounded_text(&context),
            left.len(),
            right.len(),
            differing
        );
        if let Some(index) = first {
            out.push_str(&format!(
                "\nFirst difference at byte {index}: left {}; right {}.",
                left.get(index).map_or("missing".into(), u8::to_string),
                right.get(index).map_or("missing".into(), u8::to_string)
            ));
        }
        out.push_str(&format!("\nLeft prefix: {:?}; right prefix: {:?}.\nLeft SHA-256: {}; right SHA-256: {}.\nFull array dumps omitted (original diagnostic: {} bytes; SHA-256 {}).", &left[..left.len().min(16)], &right[..right.len().min(16)], crate::util::hash(&left), crate::util::hash(&right), message.len(), crate::util::hash(message)));
        out
    } else {
        bounded_text(message)
    }
}
fn bounded_text(message: &str) -> String {
    if message.len() <= 16_384 {
        return message.to_owned();
    }
    format!(
        "{}\n[... diagnostic shortened: {} bytes; SHA-256 {} ...]\n{}",
        message.chars().take(2048).collect::<String>(),
        message.len(),
        crate::util::hash(message),
        message
            .chars()
            .rev()
            .take(1024)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    )
}

#[cfg(test)]
mod diagnostic_tests {
    #[test]
    fn large_payload_failure_preserves_first_difference_lengths_and_hashes() {
        let left = vec![0_u8; 10_000];
        let mut right = left.clone();
        right[4321] = 17;
        let message =
            format!("assertion failed: decoded payload\n  left: {left:?}\n right: {right:?}");
        let summary = super::message_summary(&message);
        assert!(summary.contains("left 10000 bytes; right 10000 bytes; 1 differing offsets"));
        assert!(summary.contains("byte 4321: left 0; right 17"));
        assert!(summary.contains(&crate::util::hash(&right)));
        assert!(summary.len() < 1500);
        let generic = super::message_summary(&"unexpected failure 🦀".repeat(4000));
        assert!(generic.contains("diagnostic shortened") && generic.len() < 16_384);
    }
}
