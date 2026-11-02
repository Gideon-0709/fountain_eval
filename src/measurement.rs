//! Metric semantics are part of T, independently of report presentation.
use crate::util::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub case: String,
    pub repetition: usize,
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
            Ok(Summary {
                case: case.into(),
                samples: values.len(),
                median_ns: median,
                min_ns: times[0],
                max_ns: *times.last().unwrap(),
                source_payload_gbps: (first.payload_bytes > 0)
                    .then_some(first.payload_bytes as f64 * 8.0 / median as f64),
                operations_per_second: (first.operations > 0)
                    .then_some(first.operations as f64 * 1e9 / median as f64),
            })
        })
        .collect()
}
