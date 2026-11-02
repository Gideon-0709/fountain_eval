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
    for event in events {
        let case = event["case"].as_str().ok_or("validation case missing")?;
        let check = recipe.check(case).ok_or("unregistered correctness check")?;
        if check.skip.is_some() {
            return Err("skipped check produced validation evidence".into());
        }
        if !seen.insert(case.to_owned()) {
            return Err("duplicate validation case".into());
        }
        if event["severity"] != serde_json::to_value(check.severity).unwrap()
            || event["rule"] != check.rule
            || !matches!(event["status"].as_str(), Some("passed" | "failed"))
        {
            return Err("correctness result does not match its registered severity/rule".into());
        }
        if check.severity == Severity::Warning
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
        if check.severity == Severity::Fatal && event["status"] == "failed" {
            return Err(format!("fatal correctness check failed: {case}"));
        }
        if event["witness"].as_str().is_none() {
            return Err("validation witness missing".into());
        }
    }
    if seen != recipe.validation_cases().into_iter().collect() {
        return Err("incomplete correctness evidence".into());
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
                out.fatal_failures.push(event.clone());
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
                warnings.insert(case.to_owned(), warning);
            }
        }
    }
    out.warnings = warnings.into_values().collect();
    if let Some(error) = &record.error
        && matches!(record.stage.as_str(), "validate" | "measure")
        && out.fatal_failures.is_empty()
    {
        out.fatal_failures.push(json!({"severity":"fatal","rule":"execution-integrity","phase":record.stage,"message":error}));
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
