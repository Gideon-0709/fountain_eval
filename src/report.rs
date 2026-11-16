//! Markdown reports: one self-contained reading surface, backed by sealed evidence.
use crate::{evidence::Record, measurement::Summary, util::Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

const PAGE: usize = 12;
const CATEGORIES: [(&str, &str); 4] = [
    ("correctness", "Correctness checks"),
    ("operation", "Individual operations"),
    ("chain", "Execution chains and encoding plans"),
    ("workflow", "Integrated workflows"),
];
fn cell(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .replace('*', "&#42;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace(['\n', '\r'], " ")
}
fn string(v: &Value) -> String {
    match v {
        Value::Null => "Not recorded".into(),
        Value::String(s) => s.clone(),
        Value::Array(xs) => {
            let mut text = xs
                .iter()
                .take(16)
                .map(string)
                .collect::<Vec<_>>()
                .join(", ");
            if xs.len() > 16 {
                let _ = write!(text, ", … ({} items)", xs.len());
            }
            format!("[{text}]")
        }
        Value::Object(fields) => fields
            .iter()
            .map(|(k, v)| format!("{}: {}", title(k), string(v)))
            .collect::<Vec<_>>()
            .join("; "),
        _ => v.to_string(),
    }
}
fn seconds(n: u64) -> String {
    format!("{:.2} s", n as f64 / 1e9)
}
fn title(s: &str) -> String {
    s.replace(['-', '_'], " ")
}
fn table(out: &mut String, headers: &[&str], rows: Vec<Vec<String>>) {
    for chunk in rows.chunks(PAGE) {
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
        let _ = writeln!(out, "| {} |", headers.join(" | "));
        let _ = writeln!(out, "| {} |", vec!["---"; headers.len()].join(" | "));
        for row in chunk {
            let _ = writeln!(
                out,
                "| {} |",
                row.iter().map(|v| cell(v)).collect::<Vec<_>>().join(" | ")
            );
        }
        out.push('\n');
    }
}
/// Reserve a unique report directory before expensive execution. Never overwrite another run.
pub fn reserve(root: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("invalid report run identifier".into());
    }
    fs::create_dir_all(root).map_err(|e| format!("report directory {}: {e}", root.display()))?;
    let dir = root.join(id);
    fs::create_dir(&dir).map_err(|e| {
        format!(
            "report destination {}: {e}; choose a new output directory",
            dir.display()
        )
    })?;
    Ok(dir)
}
pub fn publish(directory: &Path, text: &str) -> Result<PathBuf> {
    if text.trim().is_empty() {
        return Err("refusing to write an empty report".into());
    }
    let path = directory.join("report.md");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    if let Err(e) = file
        .write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(format!("report write failed: {e}"));
    }
    Ok(path)
}
fn brief(s: &str, limit: usize) -> String {
    let mut text: String = s.chars().take(limit).collect();
    if s.chars().count() > limit {
        text.push('…');
    }
    text
}
fn outcome(c: &crate::artifact::Component) -> String {
    match c.status.as_str() {
        "succeeded" if !c.correctness.warnings.is_empty() => {
            format!("PASS · {} warnings", c.correctness.warnings.len())
        }
        "succeeded" => "PASS".into(),
        "failed" => format!("FAIL · {} fatal", c.correctness.fatal_failures.len()),
        other => title(other).to_uppercase(),
    }
}
fn identity(c: &crate::artifact::Component, role: &str) -> String {
    if let Some(q) = &c.quadruple {
        let v = &q.c[role];
        if let Some(s) = v.as_str() {
            return s.into();
        }
        if !v.is_null() {
            let mut name = format!("{} {}", string(&v["package"]), string(&v["version"]));
            if role == "operator" {
                let _ = write!(name, " / {}", string(&q.c["implementation"]));
            }
            if let Some(rev) = v["origin"]["revision"].as_str() {
                let _ = write!(name, " ({})", brief(rev, 12));
            }
            return name;
        }
    }
    let selected = if role == "engine" {
        &c.request.components.engine
    } else {
        &c.request.components.operator
    };
    selected
        .as_ref()
        .map(|r| crate::progress::source(Some(r)))
        .unwrap_or_else(|| "—".into())
}
fn mark(s: &Summary) -> &'static str {
    if s.coefficient_of_variation > 0.10 {
        "†"
    } else {
        ""
    }
}
fn metric(c: &crate::artifact::Component, id: &str, kind: &str) -> String {
    if c.status != "succeeded" {
        return "Not measured".into();
    }
    let Some(s) = c.summary.iter().find(|s| s.case == id) else {
        return if c
            .plan
            .as_ref()
            .and_then(|p| p.check(id))
            .is_some_and(|p| p.skip.is_some() || p.measurement_skip.is_some())
        {
            "Skipped"
        } else {
            "Not measured"
        }
        .into();
    };
    if s.incomplete > 0 {
        return format!("Incomplete ({}/{})", s.completed, s.samples);
    }
    let value = match kind {
        "rate" => s.source_payload_gbps,
        "op" => s.ns_per_operation,
        _ => Some(s.median_ns as f64 / 1e6),
    };
    value
        .map(|n| format!("{n:.3}{}", mark(s)))
        .unwrap_or_else(|| "Not applicable".into())
}
const OPS: &[&str] = &[
    "zero-one",
    "copy-one",
    "move-one",
    "xor-one",
    "xor-many-4",
    "scale-7",
    "mul-add-7",
];
fn important(c: &crate::content::Case) -> bool {
    match c.category.as_str() {
        "operation" => {
            OPS.contains(&c.workload.as_str())
                && [128, 1016].contains(&c.width)
                && c.instances == 64
        }
        "chain" => matches!(c.workload.as_str(), "plan-build" | "plan-export"),
        "workflow" => {
            [512, 5008].contains(&c.k)
                && [128, 1016].contains(&c.width)
                && c.seed == crate::content::SEEDS[0]
                && ((c.workload == "decode"
                    && c.missing == c.k.div_ceil(10)
                    && c.order == "source-first")
                    || (c.workload == "encode-repairs" && c.repairs == 8)
                    || matches!(c.workload.as_str(), "round-trip"))
        }
        _ => false,
    }
}
pub fn snapshot(saved: &crate::artifact::Snapshot) -> String {
    let b = &saved.batch;
    let mut out = format!(
        "# Fountain evaluation report\n\nRun: `{}` · **{}** · wall time {} · test {} · {}\n\n",
        cell(&b.run_id),
        title(&b.status),
        seconds(b.elapsed_ns),
        b.configuration.selections[0].request.test.version,
        b.configuration.selections[0]
            .request
            .scheme
            .name
            .to_uppercase()
    );
    out.push_str("## Key findings\n");
    table(
        &mut out,
        &["C", "Engine / operator", "Outcome", "Performance"],
        b.entries
            .iter()
            .map(|e| {
                if let Some(c) = saved
                    .components
                    .iter()
                    .find(|c| c.component_id == e.component_id)
                {
                    vec![
                        format!("C{}", e.component_id),
                        format!("{} + {}", identity(c, "engine"), identity(c, "operator")),
                        outcome(c),
                        if c.sample_count == 0 {
                            "Not measured".into()
                        } else {
                            format!(
                                "{} cases · {} samples{}",
                                c.summary.len(),
                                c.sample_count,
                                if c.status == "succeeded" {
                                    ""
                                } else {
                                    " (partial; not accepted)"
                                }
                            )
                        },
                    ]
                } else {
                    vec![
                        format!("C{}", e.component_id),
                        "No resolved component".into(),
                        format!(
                            "{} · {}",
                            title(&e.status),
                            e.error.as_deref().unwrap_or("No child result")
                        ),
                        "Not measured".into(),
                    ]
                }
            })
            .collect(),
    );
    sections(&mut out, &saved.components);
    out
}
pub fn markdown(r: &Record) -> String {
    match crate::artifact::Component::from_record(r) {
        Ok(mut c) => {
            // The record's derived view is also used by callers constructing report previews.
            c.correctness = r.correctness.clone();
            let mut out = format!(
                "# Fountain evaluation report\n\nRun `{}` · C{} · **{}**\n\n",
                cell(&r.run_id),
                c.component_id,
                outcome(&c)
            );
            sections(&mut out, &[c]);
            out
        }
        Err(e) => format!(
            "# Fountain evaluation report\n\nRun: `{}` · Report unavailable: {}.\n",
            cell(&r.run_id),
            cell(&e)
        ),
    }
}
fn sections(out: &mut String, components: &[crate::artifact::Component]) {
    out.push_str("## Correctness and warnings\n\nWarnings allow measurement to continue. A fatal failure stops the affected component.\n");
    for c in components {
        findings(out, c);
    }
    codec_results(out, components);
    plan_results(out, components);
    operation_results(out, components);
    coverage(out, components);
}
fn codec_results(out: &mut String, components: &[crate::artifact::Component]) {
    out.push_str("## Codec throughput\n\nMedian source-equivalent throughput for 8 repair symbols and recovery with 10% missing sources (source-first, seed 6513). K counts source symbols. T is bytes per symbol. † marks CV above 10%.\n");
    for c in components {
        if !c.summary.iter().any(|s| s.case.starts_with("workflow/")) || c.status != "succeeded" {
            continue;
        }
        let _ = writeln!(out, "### C{}\n", c.component_id);
        table(
            out,
            &["K", "T", "Encode Gbit/s", "Decode Gbit/s", "Round trip ms"],
            [512usize, 5008]
                .into_iter()
                .flat_map(|k| [128, 1016].into_iter().map(move |w| (k, w)))
                .map(|(k, w)| {
                    let base = format!("{}/k{k}/b{w}", c.request.scheme.name);
                    vec![
                        k.to_string(),
                        w.to_string(),
                        metric(c, &format!("workflow/encode-repairs/{base}/r8"), "rate"),
                        metric(
                            c,
                            &format!(
                                "workflow/decode/{base}/m{}/source-first/s6513",
                                k.div_ceil(10)
                            ),
                            "rate",
                        ),
                        metric(c, &format!("workflow/round-trip/{base}"), "ms"),
                    ]
                })
                .collect(),
        );
    }
    if !components.iter().any(|c| {
        c.status == "succeeded" && c.summary.iter().any(|s| s.case.starts_with("workflow/"))
    }) {
        out.push_str("Not measured. See outcomes and coverage below.\n\n");
    }
}
fn plan_results(out: &mut String, components: &[crate::artifact::Component]) {
    out.push_str("## Encoding plans and dominant stages\n\nPlan construction and export are timed independently.\n");
    for c in components {
        if c.status != "succeeded" {
            continue;
        }
        let _ = writeln!(out, "### C{}\n", c.component_id);
        if c.summary.iter().any(|s| s.case.starts_with("chain/plan-")) {
            table(
                out,
                &["K", "Build plan ms", "Export plan ms"],
                [64, 512, 5008]
                    .into_iter()
                    .map(|k| {
                        vec![
                            k.to_string(),
                            metric(
                                c,
                                &format!("chain/plan-build/{}/k{k}", c.request.scheme.name),
                                "ms",
                            ),
                            metric(
                                c,
                                &format!("chain/plan-export/{}/k{k}", c.request.scheme.name),
                                "ms",
                            ),
                        ]
                    })
                    .collect(),
            );
        } else {
            out.push_str(
                "Plan generation/export: Not measured (no applicable standalone result).\n\n",
            );
        }
        let mut rows = Vec::new();
        for k in [512, 5008] {
            for w in [128, 1016] {
                let a = dominant(
                    c,
                    &format!("chain/encode-chain/{}/k{k}/b{w}", c.request.scheme.name),
                );
                let b = dominant(
                    c,
                    &format!("chain/decode-chain/{}/k{k}/b{w}", c.request.scheme.name),
                );
                if a.is_some() || b.is_some() {
                    rows.push(vec![
                        format!("{k} / {w}"),
                        a.unwrap_or_else(|| "Not measured".into()),
                        b.unwrap_or_else(|| "Not measured".into()),
                    ]);
                }
            }
        }
        if !rows.is_empty() {
            out.push_str("Two largest exclusive stages, as shares of the instrumented lifecycle (separate from headline timings):\n");
            table(out, &["K / T", "Encode", "Decode"], rows);
        }
    }
    if !components.iter().any(|c| c.status == "succeeded") {
        out.push_str("Not measured.\n\n");
    }
}
fn operation_results(out: &mut String, components: &[crate::artifact::Component]) {
    out.push_str("## Individual operations\n\nMedian ns/call across 64 independent instances. The multi-source XOR uses 4 sources. GF operations use scalar 7.\n");
    let mut measured = false;
    for c in components {
        if c.status != "succeeded" || !c.summary.iter().any(|s| s.case.starts_with("operation/")) {
            continue;
        }
        measured = true;
        let _ = writeln!(out, "### C{}\n", c.component_id);
        table(
            out,
            &[
                "Operation",
                "128 direct",
                "128 execute",
                "1016 direct",
                "1016 execute",
            ],
            OPS.iter()
                .map(|op| {
                    let mut row = vec![title(op)];
                    for w in [128, 1016] {
                        for dispatch in ["direct", "execute"] {
                            row.push(metric(
                                c,
                                &format!("operation/{op}/b{w}/{dispatch}/small"),
                                "op",
                            ));
                        }
                    }
                    row
                })
                .collect(),
        );
    }
    if !measured {
        out.push_str("Not measured. See outcomes and coverage below.\n\n");
    }
}
fn coverage(out: &mut String, components: &[crate::artifact::Component]) {
    out.push_str("## Coverage and measurement context\n");
    table(
        out,
        &[
            "C",
            "Checks run / applicable",
            "Measurements / planned",
            "Skipped checks",
            "Unfinished checks",
        ],
        components
            .iter()
            .map(|c| {
                if c.coverage["status"] == "not_planned" {
                    return vec![
                        format!("C{}", c.component_id),
                        "Plan unavailable".into(),
                        "Not measured".into(),
                        "—".into(),
                        "—".into(),
                    ];
                }
                vec![
                    format!("C{}", c.component_id),
                    format!(
                        "{} / {}",
                        c.validation_count,
                        c.plan.as_ref().map_or(0, |p| p.validation_cases().len())
                    ),
                    format!("{} / {}", c.summary.len(), c.planned_measurements),
                    string(&c.coverage["skipped_validation"]),
                    c.plan
                        .as_ref()
                        .map_or(0, |p| {
                            p.validation_cases()
                                .len()
                                .saturating_sub(c.validation_count)
                        })
                        .to_string(),
                ]
            })
            .collect(),
    );
    for c in components {
        let mut reasons = BTreeMap::<String, usize>::new();
        for row in c.coverage["cases"].as_array().into_iter().flatten() {
            if row["validation"]["status"] == "skipped" {
                *reasons
                    .entry(title(&string(&row["validation"]["reason"]["code"])))
                    .or_default() += 1;
            }
        }
        if !reasons.is_empty() {
            let _ = writeln!(
                out,
                "C{} skips: {}.\n",
                c.component_id,
                reasons
                    .into_iter()
                    .map(|(s, n)| format!("{n} × {}", brief(&s, 160)))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }
    if let Some(q) = components.iter().find_map(|c| c.quadruple.as_ref()) {
        let runtime = &q.e.conditions["runtime"];
        let cpu = runtime["cpu"]["brand"]
            .as_str()
            .or_else(|| runtime["cpu"]["model"].as_str())
            .or_else(|| runtime["cpu"].as_str())
            .unwrap_or("recorded in result index");
        let _ = writeln!(
            out,
            "Host: {} / {} · {}. Ordinary cases: 3 workers × 5 samples; one excluded warmup per worker. First use: 15 fresh processes.\n",
            string(&runtime["os"]),
            string(&runtime["arch"]),
            cell(cpu)
        );
    }
    out.push_str("Read all case results, diagnostic metrics and provenance with `feval report REPORT.md --json`.\n");
}
fn dominant(c: &crate::artifact::Component, id: &str) -> Option<String> {
    let metrics = &c.diagnostic_summaries.get(id)?["metrics"];
    let total = metrics["observation/stages/total_ns"]["mean"].as_f64()?;
    if total <= 0.0 {
        return None;
    }
    let mut stages: Vec<_> = metrics
        .as_object()?
        .iter()
        .filter_map(|(key, v)| {
            Some((
                key.strip_prefix("observation/stages/exclusive_ns/")?,
                v["mean"].as_f64()?,
            ))
        })
        .collect();
    stages.sort_by(|a, b| b.1.total_cmp(&a.1));
    if stages.is_empty() {
        return None;
    }
    Some(
        stages
            .into_iter()
            .take(2)
            .map(|(s, n)| format!("{} {:.1}%", title(s), n / total * 100.0))
            .collect::<Vec<_>>()
            .join("; "),
    )
}
fn diagnostic_state(message: &str, key: &str) -> Option<BTreeMap<usize, Vec<u8>>> {
    let line = message.lines().find(|s| s.trim_start().starts_with(key))?;
    let body = line
        .split_once(':')?
        .1
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?;
    body.split("],")
        .map(|piece| {
            let (id, bytes) = piece.trim().split_once(':')?;
            let bytes = bytes.trim().trim_end_matches(']');
            Some((
                id.parse().ok()?,
                serde_json::from_str(&format!("{bytes}]")).ok()?,
            ))
        })
        .collect()
}
fn findings(out: &mut String, c: &crate::artifact::Component) {
    let _ = writeln!(out, "**C{} — {}.**\n", c.component_id, outcome(c));
    for f in &c.correctness.fatal_failures {
        let case = f["case"].as_str().unwrap_or("execution integrity");
        let message = f["message"].as_str().unwrap_or("No diagnostic message");
        let _ = writeln!(out, "**Fatal** · `{}`", cell(case));
        if message.contains("V05 direct versus execute")
            || message.contains("V05 batch versus elementary direct calls")
        {
            let context = message.lines().find(|line| line.contains("V05 ")).unwrap();
            let _ = writeln!(out, "\n{}\n", cell(&brief(context, 500)));
            out.push_str("The compared paths produced different states. This is an equivalence failure; neither side is an independent scalar reference.\n\n");
            if let (Some(left), Some(right)) = (
                diagnostic_state(message, "left:"),
                diagnostic_state(message, "right:"),
            ) {
                let labels = if message.contains("V05 direct versus execute") {
                    ("direct", "execute")
                } else {
                    ("batch", "elementary calls")
                };
                if let Some((id, a, b)) = left
                    .iter()
                    .filter_map(|(id, a)| right.get(id).filter(|b| *b != a).map(|b| (id, a, b)))
                    .next()
                {
                    let offset = a
                        .iter()
                        .zip(b)
                        .position(|(x, y)| x != y)
                        .unwrap_or(a.len().min(b.len()));
                    let value = |bytes: &[u8]| {
                        bytes
                            .get(offset)
                            .map_or_else(|| "missing".into(), u8::to_string)
                    };
                    let _ = writeln!(
                        out,
                        "Symbol {id}, byte {offset}: {} = {}, {} = {}.\n",
                        labels.0,
                        value(a),
                        labels.1,
                        value(b)
                    );
                }
            }
            continue;
        }
        if message.contains("MulAdd { src: 0, scalar: 0, dst: 8 }") {
            out.push_str(
                "\nFor GF multiply-add with scalar 0, the destination must stay unchanged.",
            );
            if let (Some(actual), Some(expected)) = (
                diagnostic_state(message, "left:"),
                diagnostic_state(message, "right:"),
            ) && let (Some(left), Some(right)) = (actual.get(&8), expected.get(&8))
            {
                let _ = writeln!(out, " Symbol 8 expected {right:?}, observed {left:?}.\n");
                continue;
            }
        }
        let _ = writeln!(out, "\n{}\n", cell(&brief(message, 500)));
    }
    if c.correctness.fatal_failures.is_empty() && c.status != "succeeded" {
        let _ = writeln!(
            out,
            "Stopped at {}: {}.\n",
            cell(&c.stage),
            cell(c.error.as_deref().unwrap_or("Not executed"))
        );
    }
    if let Some(e) = &c.aggregation_error {
        let _ = writeln!(
            out,
            "Partial samples could not be aggregated: {}.\n",
            cell(e)
        );
    }
    if c.status != "succeeded" && c.sample_count > 0 {
        let _ = writeln!(
            out,
            "{} partial samples collected before termination.\n",
            c.sample_count
        );
    }
    let mut warnings = BTreeMap::<String, Vec<&Value>>::new();
    for w in &c.correctness.warnings {
        warnings
            .entry(
                w["case"]
                    .as_str()
                    .unwrap_or("unknown")
                    .split('/')
                    .nth(1)
                    .unwrap_or("unknown")
                    .into(),
            )
            .or_default()
            .push(w);
    }
    if !warnings.is_empty() {
        table(
            out,
            &[
                "Boundary",
                "Affected cases",
                "Observed deviation (example)",
                "Timed cases",
            ],
            warnings
                .into_iter()
                .map(|(name, ws)| {
                    let timed = ws
                        .iter()
                        .filter(|w| {
                            w["measurement_summary"]["samples"].as_u64().unwrap_or(0) > 0
                                || w["measured_observations"]
                                    .as_array()
                                    .is_some_and(|a| !a.is_empty())
                        })
                        .count();
                    vec![
                        title(&name),
                        ws.len().to_string(),
                        difference(&ws[0]["expected"], &ws[0]["actual"]),
                        if timed == 0 {
                            "Not measured".into()
                        } else {
                            format!("{timed}; {} not measured", ws.len() - timed)
                        },
                    ]
                })
                .collect(),
        );
    }
    if !c.failure_details.is_empty() && c.stage == "build" {
        let notes = c
            .failure_details
            .iter()
            .flat_map(|s| s.lines())
            .filter(|s| s.trim_start().starts_with("error") || s.contains("panicked at"))
            .take(3)
            .map(|s| brief(s, 300))
            .collect::<Vec<_>>();
        if !notes.is_empty() {
            let _ = writeln!(
                out,
                "{}\n",
                notes
                    .into_iter()
                    .map(|s| format!("> {}", cell(&s)))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}
fn difference(expected: &Value, actual: &Value) -> String {
    if expected == actual {
        return "Validation matched the reference".into();
    }
    if let Some(symbols) = expected["symbols"].as_array()
        && let Some(status) = actual["status"].as_str()
    {
        return format!(
            "Expected {} readable output symbols; observed {}",
            symbols.len(),
            title(status)
        );
    }
    if let (Some(a), Some(b)) = (expected["symbols"].as_array(), actual["symbols"].as_array()) {
        let changed: Vec<_> =
            a.iter()
                .filter_map(|e| {
                    let got = b.iter().find(|v| v["id"] == e["id"]);
                    if got == Some(e) {
                        None
                    } else {
                        let detail = got
                            .and_then(|got| {
                                let a = e["prefix"].as_array()?;
                                let b = got["prefix"].as_array()?;
                                a.iter().zip(b).enumerate().find(|(_, (a, b))| a != b).map(
                                    |(i, (a, b))| format!("byte {i}: expected {a}, observed {b}"),
                                )
                            })
                            .unwrap_or_else(|| {
                                if got.is_none() {
                                    "absent".into()
                                } else {
                                    "stored prefix or state hash differs".into()
                                }
                            });
                        Some(format!("symbol {}: {detail}", e["id"]))
                    }
                })
                .collect();
        if changed.is_empty() {
            return "Stored prefixes match; full-state hashes differ or extra symbols were returned".into();
        }
        return format!(
            "{} differing retained symbols; {}{}",
            changed.len(),
            changed
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            if changed.len() > 2 {
                "; additional differences retained in the result index"
            } else {
                ""
            }
        );
    }
    format!("Expected {}; observed {}", string(expected), string(actual))
}

pub fn comparison(value: &Value) -> String {
    let mut out = format!(
        "# Fountain comparison\n\nBaseline: {} · Candidate: {} · {}\n\n{}\n\n",
        cell(&string(&value["left"])),
        cell(&string(&value["right"])),
        cell(&string(&value["status"])),
        cell(&string(&value["attribution"]))
    );
    let rows = value["rows"].as_array().cloned().unwrap_or_default();
    let shown: std::collections::BTreeSet<_> = crate::content::cases()
        .into_iter()
        .filter(important)
        .map(|c| c.id)
        .collect();
    out.push_str("Negative time change means a lower candidate median. The tables cover the fixed headline workloads from the run report.\n\n");
    for (category, label) in CATEGORIES.iter().skip(1) {
        let _ = writeln!(out, "## {label}\n");
        let selected: Vec<_> = rows
            .iter()
            .filter(|v| {
                v["case"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(category) && shown.contains(s))
            })
            .collect();
        if selected.is_empty() {
            out.push_str("No common headline measurements.\n\n");
            continue;
        }
        let unit = match *category {
            "operation" => "ns/call",
            "workflow" => "Gbit/s",
            _ => "ms",
        };
        let side = |row: &Value, side: &str| -> String {
            let value = match *category {
                "operation" => row[format!("{side}_operations_per_second")]
                    .as_f64()
                    .filter(|n| *n > 0.0)
                    .map(|rate| 1e9 / rate),
                "workflow" => row[format!("{side}_source_payload_gbps")].as_f64(),
                _ => row[format!("{side}_median_ns")].as_f64().map(|ns| ns / 1e6),
            };
            value.map_or_else(|| "Unavailable".into(), |n| format!("{n:.3}"))
        };
        table(
            &mut out,
            &[
                "Case",
                &format!("Baseline {unit}"),
                &format!("Candidate {unit}"),
                "Time change",
                "CV % (base / new)",
            ],
            selected
                .into_iter()
                .map(|row| {
                    let ratio = row["left_over_right_time"]
                        .as_f64()
                        .expect("validated comparison");
                    vec![
                        string(&row["case"]),
                        side(row, "left"),
                        side(row, "right"),
                        format!("{:+.2}%", (1.0 / ratio - 1.0) * 100.0),
                        format!(
                            "{:.1} / {:.1}",
                            row["left_cv"].as_f64().unwrap_or_default() * 100.0,
                            row["right_cv"].as_f64().unwrap_or_default() * 100.0
                        ),
                    ]
                })
                .collect(),
        );
    }
    let _ = writeln!(
        out,
        "Comparison validated {} common cases. Use `feval compare LEFT RIGHT --json` for every validated comparison row.\n",
        rows.len()
    );
    for side in ["left", "right"] {
        let _ = writeln!(
            out,
            "{}: {} semantic warnings; {} skipped checks.\n",
            if side == "left" {
                "Baseline"
            } else {
                "Candidate"
            },
            value["correctness"][side]["warnings"]
                .as_array()
                .map_or(0, Vec::len),
            value["coverage"][side]["skipped_validation"]
        );
    }
    out
}
