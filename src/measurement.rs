//! Metric semantics are part of T, independently of report presentation.
use crate::util::Result;
use serde::{Deserialize, Serialize};
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
    pub worker_medians_ns: std::collections::BTreeMap<usize, u64>,
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
            let mut times: Vec<_> = values.iter().map(|s| s.elapsed_ns).collect();
            times.sort_unstable();
            let median = times[times.len() / 2];
            let completed = values.iter().filter(|v| v.outcome == "completed").count();
            let mean = times.iter().map(|v| *v as f64).sum::<f64>() / times.len() as f64;
            let stddev = (times
                .iter()
                .map(|v| (*v as f64 - mean).powi(2))
                .sum::<f64>()
                / times.len() as f64)
                .sqrt();
            let mut workers = std::collections::BTreeMap::<usize, Vec<u64>>::new();
            for v in &values {
                workers.entry(v.worker).or_default().push(v.elapsed_ns);
            }
            let worker_medians_ns = workers
                .into_iter()
                .map(|(k, mut v)| {
                    v.sort_unstable();
                    (k, v[v.len() / 2])
                })
                .collect();
            let successful = completed == values.len();
            Ok(Summary {
                case: case.into(),
                samples: values.len(),
                median_ns: median,
                min_ns: times[0],
                max_ns: *times.last().unwrap(),
                source_payload_gbps: (first.payload_bytes > 0 && successful)
                    .then_some(first.payload_bytes as f64 * 8.0 / median as f64),
                completed,
                incomplete: values.len() - completed,
                mean_ns: mean,
                stddev_ns: stddev,
                coefficient_of_variation: stddev / mean,
                worker_medians_ns,
                plans_per_second: case
                    .starts_with("chain/plan-build/")
                    .then_some(1e9 / median as f64),
                ns_per_operation: (first.operations > 0)
                    .then_some(median as f64 / first.operations as f64),
                repair_output_gbps: first.details["repair_output_bytes"]
                    .as_u64()
                    .filter(|v| *v > 0 && successful)
                    .map(|v| v as f64 * 8.0 / median as f64),
                operations_per_second: (first.operations > 0)
                    .then_some(first.operations as f64 * 1e9 / median as f64),
            })
        })
        .collect()
}
