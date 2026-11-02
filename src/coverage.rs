//! Complete per-case coverage. Applicability skips are never inferred from execution errors.
use crate::evidence::Record;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Skip {
    pub code: String,
    pub reason: String,
}

pub fn report(record: &Record) -> Value {
    let Some(recipe) = &record.recipe else {
        return json!({"status":"not_planned","cases":[],"reason":"Test content has not been resolved"});
    };
    let rows: Vec<_> = recipe.checks.iter().map(|(case,check)| {
        let event = record.validation.iter().find(|e|e["case"]==*case);
        let samples=record.samples.iter().filter(|s|s.case==*case).count();
        let validation = if let Some(skip)=&check.skip {
            json!({"status":"skipped","reason":skip})
        } else if let Some(event)=event {
            json!({"status":event["status"],"severity":event["severity"]})
        } else { json!({"status":"not_executed","reason":"No completed validation observation; see run status and stage"}) };
        let measurement = if let Some(skip)=check.skip.as_ref().or(check.measurement_skip.as_ref()) {
            json!({"status":"skipped","reason":skip})
        } else if !check.measured {
            json!({"status":"not_applicable","reason":"This registered check only validates correctness"})
        } else {
            json!({"status":if samples==recipe.repetitions {"recorded"} else if samples>0 {"incomplete"} else {"not_executed"},"samples":samples,"expected_samples":recipe.repetitions,"accepted":record.status=="succeeded" && samples==recipe.repetitions})
        };
        let diagnostics=if let Some(skip)=&check.skip{json!({"status":"skipped","reason":skip})}
            else if !check.measured||check.spec.category=="operation"||check.spec.fresh_process{json!({"status":"not_applicable","reason":"Separate stage/resource diagnostics apply to non-fresh chain and workflow cases"})}
            else{let count=record.diagnostics.iter().filter(|v|v["case"]==*case).count();json!({"status":if count==crate::content::WORKERS{"recorded"}else if count>0{"incomplete"}else{"not_executed"},"observations":count,"expected":crate::content::WORKERS})};
        json!({"case":case,"category":check.spec.category,"family":check.spec.family,"rule":check.rule,"validation":validation,"measurement":measurement,"diagnostics":diagnostics})
    }).collect();
    let skipped_validation = rows
        .iter()
        .filter(|r| r["validation"]["status"] == "skipped")
        .count();
    let skipped_measurement = rows
        .iter()
        .filter(|r| r["measurement"]["status"] == "skipped")
        .count();
    json!({"status":if record.status=="succeeded" {if skipped_validation+skipped_measurement>0 {"completed_with_skips"} else {"completed"}} else {"incomplete"},"registered_cases":rows.len(),"skipped_validation":skipped_validation,"skipped_measurement":skipped_measurement,"cases":rows})
}
