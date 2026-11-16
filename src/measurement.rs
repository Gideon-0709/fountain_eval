//! Metric semantics are part of T, independently of report presentation.
use crate::util::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub case: String,
    pub repetition: usize,
    pub worker: usize,
    pub outcome: String,
    pub details: serde_json::Value,
    pub elapsed_ns: u64,
    pub payload_bytes: u64,
    pub operations: u64,
    pub witness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub case: String,
    pub samples: usize,
    pub median_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub source_payload_gbps: Option<f64>,
    pub operations_per_second: Option<f64>,
    pub ns_per_operation: Option<f64>,
    pub plans_per_second: Option<f64>,
    pub repair_output_gbps: Option<f64>,
    pub completed: usize,
    pub incomplete: usize,
    pub mean_ns: f64,
    pub stddev_ns: f64,
    pub coefficient_of_variation: f64,
    pub worker_medians_ns: BTreeMap<usize, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_median_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_median_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workload: BTreeMap<String, Value>,
}

fn median(values: &mut [u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

/// Summarize observed work and endpoints before execution evidence is cleaned.
fn workload_summary(values: &[&Sample]) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for field in [
        "input_digest",
        "sequence_digest",
        "capture_decoded",
        "dispatch",
        "fields",
        "histogram",
        "reuse_scope",
        "timed_work",
    ] {
        let mut variants = BTreeMap::new();
        for sample in values {
            let Some(value) = sample.details.get(field).filter(|v| !v.is_null()) else {
                continue;
            };
            let entry = variants.entry(value.to_string()).or_insert((value, 0));
            entry.1 += 1;
        }
        if !variants.is_empty() {
            out.insert(
                field.into(),
                json!(
                    variants
                        .into_values()
                        .map(|(value, samples)| json!({"value":value,"samples":samples}))
                        .collect::<Vec<_>>()
                ),
            );
        }
    }
    for field in [
        "output_bytes",
        "repair_output_bytes",
        "received_symbols",
        "received_bytes",
        "received_sources",
        "received_repairs",
        "offered_symbols",
        "repair_budget",
        "consumed",
        "calls",
        "public_calls",
        "initial_symbol_bytes",
        "retained_input_bytes",
        "output_ready_ns",
        "attempt_end_ns",
        "component_released_ns",
        "release_ns",
        "lifecycle_ns",
        "first_block_ready_ns",
        "plan_constructed_ns",
        "max_block_gap_ns",
        "batch_total_ns",
    ] {
        let split: &[&str] =
            if field.ends_with("_ns") && values.iter().any(|v| v.outcome != "completed") {
                &["completed", "incomplete_recovery"]
            } else {
                &["all"]
            };
        let mut stats = BTreeMap::new();
        for &status in split {
            let mut observed: Vec<_> = values
                .iter()
                .filter(|s| status == "all" || s.outcome == status)
                .filter_map(|s| s.details[field].as_u64())
                .collect();
            if let Some(median) = median(&mut observed) {
                stats.insert(status, json!({"median":median,"min":observed[0],"max":observed.last(),"samples":observed.len()}));
            }
        }
        if !stats.is_empty() {
            out.insert(
                field.into(),
                if split == ["all"] {
                    stats.remove("all").unwrap()
                } else {
                    json!(stats)
                },
            );
        }
    }
    out
}
pub fn summarize(samples: &[Sample]) -> Result<Vec<Summary>> {
    let mut groups = std::collections::BTreeMap::<&str, Vec<&Sample>>::new();
    for sample in samples {
        if sample.elapsed_ns == 0 {
            return Err("zero duration is not a measurement".into());
        }
        groups.entry(&sample.case).or_default().push(sample);
    }
    groups
        .into_iter()
        .map(|(case, values)| {
            let first = values[0];
            if values
                .iter()
                .any(|s| s.payload_bytes != first.payload_bytes || s.operations != first.operations)
            {
                return Err(format!("inconsistent work or outputs in {case}"));
            }
            let completed: Vec<_> = values
                .iter()
                .copied()
                .filter(|v| v.outcome == "completed")
                .collect();
            let mut complete_times: Vec<_> = completed.iter().map(|v| v.elapsed_ns).collect();
            let mut incomplete_times: Vec<_> = values
                .iter()
                .filter(|v| v.outcome != "completed")
                .map(|v| v.elapsed_ns)
                .collect();
            let completed_median_ns = median(&mut complete_times);
            let incomplete_median_ns = median(&mut incomplete_times);
            let (times, homogeneous) = if complete_times.is_empty() {
                (&incomplete_times, &values)
            } else {
                (&complete_times, &completed)
            };
            let median = completed_median_ns.or(incomplete_median_ns).unwrap();
            let mean = times.iter().map(|v| *v as f64).sum::<f64>() / times.len() as f64;
            let stddev = (times
                .iter()
                .map(|v| (*v as f64 - mean).powi(2))
                .sum::<f64>()
                / times.len() as f64)
                .sqrt();
            let mut workers = std::collections::BTreeMap::<usize, Vec<u64>>::new();
            for v in homogeneous {
                workers.entry(v.worker).or_default().push(v.elapsed_ns);
            }
            let worker_medians_ns = workers
                .into_iter()
                .map(|(k, mut v)| {
                    v.sort_unstable();
                    (k, v[v.len() / 2])
                })
                .collect();
            let successful = completed.len() == values.len();
            let repair_bytes = first.details["repair_output_bytes"].as_u64();
            if values
                .iter()
                .any(|v| v.details["repair_output_bytes"].as_u64() != repair_bytes)
            {
                return Err(format!("inconsistent repair output bytes in {case}"));
            }
            Ok(Summary {
                case: case.into(),
                samples: values.len(),
                median_ns: median,
                min_ns: times[0],
                max_ns: *times.last().unwrap(),
                source_payload_gbps: (first.payload_bytes > 0 && successful)
                    .then_some(first.payload_bytes as f64 * 8.0 / median as f64),
                completed: completed.len(),
                incomplete: incomplete_times.len(),
                completed_median_ns,
                incomplete_median_ns,
                workload: workload_summary(&values),
                mean_ns: mean,
                stddev_ns: stddev,
                coefficient_of_variation: stddev / mean,
                worker_medians_ns,
                plans_per_second: case
                    .starts_with("chain/plan-build/")
                    .then_some(1e9 / median as f64),
                ns_per_operation: (first.operations > 0)
                    .then_some(median as f64 / first.operations as f64),
                repair_output_gbps: repair_bytes
                    .filter(|v| *v > 0 && successful)
                    .map(|v| v as f64 * 8.0 / median as f64),
                operations_per_second: (first.operations > 0)
                    .then_some(first.operations as f64 * 1e9 / median as f64),
            })
        })
        .collect()
}
