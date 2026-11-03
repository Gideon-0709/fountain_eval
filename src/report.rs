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
fn number(v: Option<f64>) -> String {
    v.map(|n| format!("{n:.3}")).unwrap_or_else(|| "—".into())
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
fn component_reference(r: &crate::protocol::ComponentReference) -> String {
    use crate::protocol::SourceReference;
    let mut text = match &r.source {
        SourceReference::Path { path } => format!("path:{}", path.display()),
        SourceReference::Registry {
            package, version, ..
        } => format!("registry:{package}@{version}"),
        SourceReference::Git {
            url,
            revision,
            package,
        } => format!("git+{url}?package={package}&rev={revision}"),
    };
    if let Some(implementation) = &r.implementation {
        let _ = write!(text, "#{implementation}");
    }
    text
}
fn reference(r: &Record, role: &str) -> String {
    let selection = if role == "engine" {
        &r.request.components.engine
    } else {
        &r.request.components.operator
    };
    selection
        .as_ref()
        .map(component_reference)
        .unwrap_or_else(|| "Not selected".into())
}
fn identity(r: &Record, role: &str) -> String {
    let Some(q) = &r.quadruple else {
        return reference(r, role);
    };
    let c = &q.c[role];
    if c.is_null() {
        return "Not selected".into();
    }
    if c.is_string() {
        return string(c);
    }
    let mut text = format!("{} {}", string(&c["package"]), string(&c["version"]));
    if role == "operator" {
        let _ = write!(text, " / {}", string(&q.c["implementation"]));
    }
    if let Some(rev) = c["origin"]["revision"].as_str() {
        let _ = write!(text, " / {}", &rev[..rev.len().min(12)]);
    }
    text
}
fn explanation(r: &Record) -> String {
    if let Some(f) = r.correctness.fatal_failures.first() {
        return format!(
            "Fatal check {}; later work in this C stopped",
            f["case"].as_str().unwrap_or("execution integrity")
        );
    }
    if let Some(error) = &r.error {
        return error.split("; see ").next().unwrap_or(error).to_owned();
    }
    match r.status.as_str() {
        "succeeded" => "All applicable work completed; review warnings and skipped coverage".into(),
        "prepared" => {
            "Build and capability checks completed; correctness and measurements not executed"
                .into()
        }
        _ => format!("Execution ended at {}", r.stage),
    }
}
fn highlights(out: &mut String, records: &[(u32, Record)]) {
    out.push_str("## Key findings\n\n");
    table(
        out,
        &[
            "C",
            "Outcome",
            "Checks / warnings",
            "Measured cases / samples",
            "Main finding",
        ],
        records
            .iter()
            .map(|(id, r)| {
                let finding = if let Some(f) = r.correctness.fatal_failures.first() {
                    format!(
                        "Fatal: {}; later work stopped",
                        f["case"].as_str().unwrap_or("execution integrity")
                    )
                } else if let Some(error) = &r.error {
                    error.split("; see ").next().unwrap_or(error).to_owned()
                } else if r.status == "prepared" {
                    "Inputs frozen; measurement not started".into()
                } else if !r.correctness.warnings.is_empty() {
                    "Applicable work completed with semantic warnings".into()
                } else {
                    "Applicable work completed".into()
                };
                vec![
                    format!("C{id}"),
                    if r.status == "succeeded" && !r.correctness.warnings.is_empty() {
                        "passed with warnings".into()
                    } else {
                        r.status.clone()
                    },
                    format!("{} / {}", r.validation.len(), r.correctness.warnings.len()),
                    if r.samples.is_empty() {
                        "Not measured".into()
                    } else {
                        format!("{} / {}", r.summary.len(), r.samples.len())
                    },
                    finding,
                ]
            })
            .collect(),
    );
    for (id, r) in records {
        if r.status != "succeeded" {
            continue;
        }
        let noisy = r
            .summary
            .iter()
            .filter(|s| s.coefficient_of_variation > 0.10)
            .count();
        let _ = writeln!(
            out,
            "C{id}: {noisy}/{} measured cases have CV above 10%; this flags dispersion, not failure. No baseline is selected, so this report makes no improvement/regression claim.\n",
            r.summary.len()
        );
        let key: Vec<_> = r
            .summary
            .iter()
            .filter(|s| {
                if s.case.starts_with("chain/plan-build/") {
                    return true;
                }
                let Some(check) = r.recipe.as_ref().and_then(|p| p.check(&s.case)) else {
                    return false;
                };
                let c = &check.spec;
                [512, 5008].contains(&c.k)
                    && [128, 1016].contains(&c.width)
                    && c.seed == crate::content::SEEDS[0]
                    && ((c.workload == "encode-repairs" && c.repairs == 8)
                        || c.workload == "round-trip"
                        || (c.workload == "decode"
                            && c.missing == c.k.div_ceil(10)
                            && c.order == "source-first"))
            })
            .collect();
        if !key.is_empty() {
            out.push_str("Planning and codec overview: K=512/5008, b=128/1016, first registered seed; 8 repairs for encoding, 10% missing sources for recovery. Plan-only rows also include K=64. All other cases are in the category details.\n\n");
            table(
                out,
                &["Case", "Median ns", "Source Gbit/s", "CV %"],
                key.iter()
                    .map(|s| {
                        vec![
                            s.case.clone(),
                            s.median_ns.to_string(),
                            number(s.source_payload_gbps),
                            format!("{:.2}", s.coefficient_of_variation * 100.0),
                        ]
                    })
                    .collect(),
            );
        }
    }
}
fn intro(out: &mut String) {
    out.push_str("[Components and outcomes](#1-components-and-outcomes) · [Correctness](#2-correctness-and-failures) · [Operations](#3-individual-operations) · [Chains and plans](#4-execution-chains-and-encoding-plans) · [Workflows](#5-integrated-workflows) · [Coverage](#6-coverage-and-explicit-skips) · [Reproducibility](#7-execution-time-and-reproducibility)\n\n");
    out.push_str("Success covers applicable cases only. A fatal failure stops that C; warnings retain their timings. Skipped cases and work not reached after a failure are reported separately. A successful product API run does not imply that skipped operator or planning APIs passed.\n\nRead the outcome and correctness sections first, then individual operations, execution chains and integrated workflows. Performance tables contain ordinary measurements. Instrumented stage and resource observations are separate. No baseline is selected automatically; use `compare` for verified relative changes.\n\n");
}
pub fn batch_markdown(directory: &Path, batch: &crate::batch::Record) -> Result<String> {
    let mut records = Vec::new();
    for e in &batch.entries {
        if let Some(path) = &e.directory {
            records.push((
                e.component_id,
                crate::evidence::load(&directory.join(path))?,
            ));
        }
    }
    let mut out = format!(
        "# Fountain evaluation report\n\nRun: `{}` · Action: **{}** · Outcome: **{}** · Execution wall time: **{}**.\n\n",
        cell(&batch.run_id),
        cell(&batch.action),
        cell(&batch.status),
        seconds(batch.elapsed_ns)
    );
    if let Some(selection) = batch.configuration.selections.first() {
        let _ = writeln!(
            out,
            "Test content: **{}** · Scheme: **{}** · Component selections: **{}**.\n",
            cell(&selection.request.test.version),
            cell(&selection.request.scheme.name),
            batch.entries.len()
        );
    }
    highlights(&mut out, &records);
    intro(&mut out);
    out.push_str("## 1. Components and outcomes\n");
    table(
        &mut out,
        &["C", "Selected scope", "Engine", "Operator"],
        batch
            .entries
            .iter()
            .map(|e| {
                let r = records
                    .iter()
                    .find(|(id, _)| *id == e.component_id)
                    .map(|(_, r)| r);
                vec![
                    format!("C{}", e.component_id),
                    match e.request.components.scope().unwrap() {
                        crate::protocol::Scope::Pair => "Engine + operator",
                        crate::protocol::Scope::Engine => "Engine only",
                        crate::protocol::Scope::Operator => "Operator only",
                    }
                    .into(),
                    r.map(|r| identity(r, "engine")).unwrap_or_else(|| {
                        e.request
                            .components
                            .engine
                            .as_ref()
                            .map(component_reference)
                            .unwrap_or_else(|| "Not selected".into())
                    }),
                    r.map(|r| identity(r, "operator")).unwrap_or_else(|| {
                        e.request
                            .components
                            .operator
                            .as_ref()
                            .map(component_reference)
                            .unwrap_or_else(|| "Not selected".into())
                    }),
                ]
            })
            .collect(),
    );
    table(
        &mut out,
        &["C", "Outcome", "Wall time", "Meaning / failure boundary"],
        batch
            .entries
            .iter()
            .map(|e| {
                vec![
                    format!("C{}", e.component_id),
                    e.status.clone(),
                    e.elapsed_ns
                        .map(seconds)
                        .unwrap_or_else(|| "Not executed".into()),
                    records
                        .iter()
                        .find(|(id, _)| *id == e.component_id)
                        .map(|(_, r)| explanation(r))
                        .unwrap_or_else(|| {
                            e.error
                                .clone()
                                .unwrap_or_else(|| "No child evidence".into())
                        }),
                ]
            })
            .collect(),
    );
    sections(&mut out, &records);
    out.push_str("## 7. Execution time and reproducibility\n\nWall time includes component resolution, builds, validation, measurement, diagnostics and evidence processing. It is not a codec latency. Failed entries show time to failure. The batch total includes sequential child invocations and checkpoints, excluding its own final seal and Markdown publication.\n");
    let mut environments = BTreeMap::new();
    for (id, r) in &records {
        let previous = r
            .quadruple
            .as_ref()
            .and_then(|q| environments.get(&q.e.digest).copied());
        let _ = writeln!(
            out,
            "<details>\n<summary>C{id}: inputs, identity and environment</summary>\n"
        );
        provenance(&mut out, *id, r, previous);
        out.push_str("</details>\n\n");
        if let Some(q) = &r.quadruple {
            environments.entry(q.e.digest.clone()).or_insert(*id);
        }
    }
    out.push_str("### Result retention\n\nThis file contains the complete result summary and a checksummed machine-readable result index for `report`, `compare` and prepared execution. Raw logs and individual samples are temporary and are removed only after this file has been published and verified. The build cache is retained. Aggregate statistics cannot be recomputed from individual samples after cleanup.\n\n");
    Ok(out)
}
pub fn markdown(r: &Record) -> String {
    let mut out = format!(
        "# Fountain evaluation report\n\nRun: `{}` · C{} · Outcome: **{}** · Last phase: **{}**.\n\n",
        cell(&r.run_id),
        r.configuration.component_id,
        cell(&r.status),
        cell(&r.stage)
    );
    highlights(&mut out, &[(r.configuration.component_id, r.clone())]);
    intro(&mut out);
    out.push_str("## 1. Components and outcomes\n");
    table(
        &mut out,
        &["Engine", "Operator", "Outcome detail"],
        vec![vec![
            identity(r, "engine"),
            identity(r, "operator"),
            explanation(r),
        ]],
    );
    sections(&mut out, &[(r.configuration.component_id, r.clone())]);
    out.push_str("## 7. Execution time and reproducibility\n\n");
    provenance(&mut out, r.configuration.component_id, r, None);
    out
}
fn sections(out: &mut String, records: &[(u32, Record)]) {
    out.push_str("## 2. Correctness and failures\n\nFatal checks enforce defined arithmetic, payload and execution contracts. Boundary warnings report ambiguous or divergent semantics without suppressing performance. Counts below are executed observations, not every registered case.\n");
    table(
        out,
        &[
            "C",
            "Correctness",
            "Fatal checks executed",
            "Fatal failures",
            "Boundary checks executed",
            "Warnings",
        ],
        records
            .iter()
            .map(|(id, r)| {
                vec![
                    format!("C{id}"),
                    r.correctness.status.clone(),
                    r.correctness.fatal_checks.to_string(),
                    r.correctness.fatal_failures.len().to_string(),
                    r.correctness.warning_checks.to_string(),
                    r.correctness.warnings.len().to_string(),
                ]
            })
            .collect(),
    );
    for (id, r) in records {
        let _ = writeln!(out, "### C{id}: correctness details\n");
        correctness(out, r);
    }
    out.push_str("## 3. Individual operations\n\nTable key: K = source symbols; b = bytes per symbol; m = missing sources; r = repairs; s = seed. Timing columns use nanoseconds; CV is standard deviation / mean. A dash is undefined, not zero. Each short table repeats its header. Sample counts, completion counts and worker medians remain available in the result index and expanded statistics below.\n\nEach row measures a prepared public call across independently initialized instances. `ns/call` divides the ordinary sample median by its call count. Gbit/s, when defined, uses logical bytes; it is not physical memory bandwidth. Direct and execute paths remain separate.\n");
    performance(out, records, "operation");
    out.push_str("## 4. Execution chains and encoding plans\n\nPlan construction and export are independent chain workloads. Recorded replay measures the captured operation sequence, excluding solving and sequence capture. Diagnostic values are means from separate instrumented processes; exclusive stage means plus unattributed time equal their lifecycle mean, not the ordinary median. Nested counters belong to their parent stages. Instrumentation differences include process variation; allocation growth can include retained output.\n");
    performance(out, records, "chain");
    out.push_str("## 5. Integrated workflows\n\nThese timings cover the selected complete public workflow through owned output. Source Gbit/s = source payload bytes × 8 / elapsed ns; repair Gbit/s uses actual repair output bytes. An incomplete recovery has no successful source throughput. Lifecycle cases include release; output-ready cases end before release. Continuous cases measure the whole configured block sequence.\n");
    performance(out, records, "workflow");
    out.push_str("## 6. Coverage and explicit skips\n\nEvery registered case is accounted for. Validation, ordinary measurement and diagnostic eligibility are different. `not_executed` means applicable work was not reached; `skipped` records a known applicability reason; `not_applicable` means the case does not define that kind of observation. Counts are grouped by test family and reason. Exact case IDs remain in coverage.json.\n");
    for (id, r) in records {
        let _ = writeln!(out, "### C{id}: coverage\n");
        let coverage = crate::coverage::report(r);
        let _ = writeln!(
            out,
            "Registered cases: {}; validation skips: {}; measurement skips: {}.\n\n<details>\n<summary>Coverage by category, phase and reason</summary>\n",
            coverage["registered_cases"],
            coverage["skipped_validation"],
            coverage["skipped_measurement"]
        );
        self::coverage(out, r);
        out.push_str("</details>\n\n");
    }
}
fn correctness(out: &mut String, r: &Record) {
    if r.correctness.fatal_failures.is_empty() {
        out.push_str("No fatal correctness failure was recorded. Review execution status and coverage before inferring that checks ran.\n\n");
        if let Some(error) = &r.error {
            let _ = writeln!(
                out,
                "**Execution outcome at {}:** {}\n",
                cell(&r.stage),
                cell(error)
            );
        }
    } else {
        out.push_str("#### Fatal correctness failures\n\n");
        for event in &r.correctness.fatal_failures {
            finding(out, r, event, true);
        }
    }
    if r.correctness.warnings.is_empty() {
        out.push_str("No semantic warning recorded.\n\n");
    } else {
        out.push_str("#### Semantic warnings\n\nThe following deviations remain warnings. Their operation timings are included in the performance section. Expected and observed values below are validation results; repeated measured observations are summarized separately.\n\n");
        let mut groups = BTreeMap::<String, Vec<&Value>>::new();
        for w in &r.correctness.warnings {
            let case = w["case"].as_str().unwrap_or("unknown");
            let prefix = case.split('/').take(2).collect::<Vec<_>>().join("/");
            groups.entry(prefix).or_default().push(w);
        }
        for (group, events) in groups {
            let _ = writeln!(
                out,
                "**{} — {} affected cases.**\n",
                cell(&group),
                events.len()
            );
            if let Some(check) = r
                .recipe
                .as_ref()
                .and_then(|p| p.check(events[0]["case"].as_str().unwrap_or("")))
            {
                let _ = writeln!(out, "Contract: {}.\n", cell(&check.description));
            }
            let measured = events
                .iter()
                .filter(|e| {
                    e["measured_observations"]
                        .as_array()
                        .is_some_and(|v| !v.is_empty())
                })
                .count();
            let _ = writeln!(
                out,
                "{} cases measured; {} not measured.\n\n<details>\n<summary>Case-specific expected/observed results</summary>\n",
                measured,
                events.len() - measured
            );
            table(
                out,
                &[
                    "Case",
                    "Operation / dispatch",
                    "Expected versus observed",
                    "Measured observations",
                ],
                events
                    .iter()
                    .map(|e| {
                        let operations = e["details"]["operations"]
                            .as_array()
                            .map(|xs| xs.iter().map(action).collect::<Vec<_>>().join("; "))
                            .unwrap_or_default();
                        let repeated = e["measured_observations"].as_array();
                        let observed = repeated
                            .filter(|xs| !xs.is_empty())
                            .map(|xs| {
                                let mut text = format!(
                                    "{}/{} samples differ",
                                    xs.iter()
                                        .filter(|v| v["matches_reference"] == false)
                                        .count(),
                                    xs.len()
                                );
                                if let Some(outcome) = xs
                                    .iter()
                                    .filter_map(|v| v["actual"]["outcomes"].as_array())
                                    .flatten()
                                    .find(|v| v["actual"] != e["expected"])
                                {
                                    let _ = write!(
                                        text,
                                        "; example: {}",
                                        difference(&e["expected"], &outcome["actual"])
                                    );
                                }
                                text
                            })
                            .unwrap_or_else(|| "Not measured".into());
                        vec![
                            short_case(e["case"].as_str().unwrap_or("unknown"), &group),
                            format!("{}; {}", operations, string(&e["details"]["dispatch"])),
                            difference(&e["expected"], &e["actual"]),
                            observed,
                        ]
                    })
                    .collect(),
            );
            out.push_str("</details>\n\n");
        }
    }
}
fn action(v: &Value) -> String {
    if let Some(fields) = v.as_object() {
        format!(
            "{}({})",
            v["op"].as_str().unwrap_or("operation"),
            fields
                .iter()
                .filter(|(k, _)| *k != "op")
                .map(|(k, v)| format!("{k}={}", string(v)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        string(v)
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
fn finding(out: &mut String, r: &Record, event: &Value, fatal: bool) {
    let case = event["case"].as_str().unwrap_or("execution integrity");
    let _ = writeln!(
        out,
        "**{}: {}**\n",
        if fatal { "Fatal" } else { "Warning" },
        cell(case)
    );
    if let Some(check) = r.recipe.as_ref().and_then(|p| p.check(case)) {
        let _ = writeln!(
            out,
            "- Check: {} ({}).\n- Input: symbol width {} bytes; seed {}; workload {}.\n- Expected contract: {}.\n",
            cell(&check.spec.family),
            cell(&check.rule),
            check.spec.width,
            check.spec.seed,
            cell(&check.spec.workload),
            cell(&check.description)
        );
    }
    let message = event["message"].as_str().unwrap_or("");
    if message.contains("MulAdd { src: 0, scalar: 0, dst: 8 }")
        && message.contains("left:")
        && message.contains("right:")
    {
        out.push_str("The failed operation is **GF multiply-add with scalar 0**, source ID 0 and destination ID 8 (distinct symbols). The contract is `dst = dst XOR (0 × src)`, so the destination must stay unchanged.\n\n");
        let parse_state = |key: &str| -> Option<BTreeMap<usize, Vec<u8>>> {
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
        };
        if let (Some(actual), Some(expected)) = (parse_state("left:"), parse_state("right:")) {
            table(
                out,
                &["Symbol", "Expected bytes", "Observed bytes"],
                expected
                    .iter()
                    .filter(|(id, value)| actual.get(id) != Some(value))
                    .map(|(id, value)| {
                        vec![
                            id.to_string(),
                            format!("{value:?}"),
                            format!("{:?}", actual.get(id).unwrap_or(&Vec::new())),
                        ]
                    })
                    .collect(),
            );
            out.push_str("This is defined arithmetic, not an ambiguous alias rule. The observed mismatch can invalidate computations using this operation. It does not establish that a codec workload produced incorrect bytes: that C stopped before subsequent measurements.\n\n");
        }
    }
    if !message.is_empty() {
        let _ = writeln!(
            out,
            "<details>\n<summary>Original diagnostic message</summary>\n\n> {}\n\n</details>\n",
            message.lines().map(cell).collect::<Vec<_>>().join("\n> ")
        );
    }
    if !event["expected"].is_null() || !event["actual"].is_null() {
        let _ = writeln!(
            out,
            "{}\n",
            cell(&difference(&event["expected"], &event["actual"]))
        );
    }
    let _ = writeln!(
        out,
        "Phase: **{}**. Impact: {}.\n",
        cell(event["phase"].as_str().unwrap_or(&r.stage)),
        if fatal {
            "this C is not accepted; later unexecuted cases are not passes and no complete performance aggregate is published"
        } else {
            "the warning remains visible alongside measured performance"
        }
    );
}
fn group(case: &str) -> String {
    case.split('/').take(2).collect::<Vec<_>>().join("/")
}
fn short_case(case: &str, prefix: &str) -> String {
    case.strip_prefix(&format!("{prefix}/"))
        .unwrap_or(case)
        .into()
}
fn performance(out: &mut String, records: &[(u32, Record)], category: &str) {
    for (id, r) in records {
        let _ = writeln!(
            out,
            "### C{id}: {}\n",
            CATEGORIES.iter().find(|(c, _)| *c == category).unwrap().1
        );
        let partial;
        let summaries = if r.status != "succeeded" {
            let _ = writeln!(
                out,
                "No accepted performance summary: **{}** at **{}**. {} samples recorded before termination; no accepted aggregate.\n",
                cell(&r.status),
                cell(&r.stage),
                r.samples.len()
            );
            partial = crate::measurement::summarize(&r.samples).unwrap_or_default();
            &partial
        } else {
            &r.summary
        };
        let mut groups = BTreeMap::<String, Vec<&Summary>>::new();
        for row in summaries {
            if row.case.starts_with(&format!("{category}/")) {
                groups.entry(group(&row.case)).or_default().push(row);
            }
        }
        if groups.is_empty() {
            out.push_str(
                "No measured cases in this category. See this C's explicit coverage reasons.\n\n",
            );
            continue;
        }
        let unstable = r
            .summary
            .iter()
            .filter(|s| {
                s.case.starts_with(&format!("{category}/")) && s.coefficient_of_variation > 0.10
            })
            .count();
        if unstable > 0 {
            let _ = writeln!(
                out,
                "Dispersion note: {unstable} cases have CV above 10%. This is a descriptive flag, not a correctness or performance threshold. Inspect their ranges and worker medians before interpreting small changes.\n"
            );
        }
        for (prefix, rows) in groups {
            let _ = writeln!(
                out,
                "#### {}\n\nCase prefix: `{}`.\n",
                title(prefix.split('/').nth(1).unwrap_or(&prefix)),
                cell(&prefix)
            );
            let counts: std::collections::BTreeSet<_> =
                rows.iter().map(|s| (s.completed, s.samples)).collect();
            let _ = writeln!(
                out,
                "<details>\n<summary>{} cases: timing, rates and dispersion</summary>\n",
                rows.len()
            );
            if counts.len() == 1 {
                let (done, total) = counts.iter().next().unwrap();
                let _ = writeln!(
                    out,
                    "Every case: {total} samples; {done} completed, {} incomplete.\n",
                    total - done
                );
            }
            table(
                out,
                &[
                    "Case suffix",
                    "Median [min–max] ns",
                    "CV %; mean ± SD ns",
                    "Work-normalized rate",
                    "Worker medians ns",
                ],
                rows.iter()
                    .map(|s| {
                        let mut rate = if category == "operation" {
                            format!(
                                "{} ns/call; {} Gbit/s",
                                number(s.ns_per_operation),
                                number(s.source_payload_gbps)
                            )
                        } else if let Some(p) = s.plans_per_second {
                            format!("{p:.3} plans/s")
                        } else {
                            format!(
                                "{} source; {} repair Gbit/s{}",
                                number(s.source_payload_gbps),
                                number(s.repair_output_gbps),
                                s.ns_per_operation
                                    .map(|n| format!("; {n:.3} ns/call"))
                                    .unwrap_or_default()
                            )
                        };
                        if counts.len() != 1 {
                            let _ = write!(rate, "; {}/{} complete", s.completed, s.samples);
                        }
                        vec![
                            short_case(&s.case, &prefix),
                            format!("{} [{}–{}]", s.median_ns, s.min_ns, s.max_ns),
                            format!(
                                "{:.2}; {:.1} ± {:.1}",
                                s.coefficient_of_variation * 100.0,
                                s.mean_ns,
                                s.stddev_ns
                            ),
                            rate,
                            s.worker_medians_ns
                                .iter()
                                .map(|(w, n)| format!("{w}:{n}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ]
                    })
                    .collect(),
            );
            out.push_str("</details>\n\n");
            if category != "operation" {
                diagnostics(out, r, &prefix);
            }
        }
    }
}
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}
fn flatten(prefix: &str, v: &Value, fields: &mut BTreeMap<String, Vec<Value>>) {
    match v {
        Value::Object(m) => {
            for (key, value) in m {
                if matches!(
                    key.as_str(),
                    "block_timeline" | "program_digest" | "sequence_digest" | "sha256" | "stages"
                ) {
                    continue;
                }
                flatten(
                    &if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix} / {}", title(key))
                    },
                    value,
                    fields,
                );
            }
        }
        Value::Array(xs) if xs.iter().all(|x| !x.is_array() && !x.is_object()) => {
            fields.entry(prefix.into()).or_default().push(v.clone());
        }
        Value::Array(xs) => {
            for (i, x) in xs.iter().enumerate() {
                flatten(&format!("{prefix} / item {}", i + 1), x, fields);
            }
        }
        Value::Null => {}
        _ => fields.entry(prefix.into()).or_default().push(v.clone()),
    }
}
fn diagnostics(out: &mut String, r: &Record, prefix: &str) {
    let mut cases = BTreeMap::<String, BTreeMap<String, Vec<Value>>>::new();
    for d in &r.diagnostics {
        let case = d["case"].as_str().unwrap_or("");
        if group(case) != prefix {
            continue;
        }
        let fields = cases.entry(case.into()).or_default();
        fields
            .entry("Instrumented elapsed ns".into())
            .or_default()
            .push(d["elapsed_ns"].clone());
        flatten("Work", &d["observation"], fields);
        flatten("Allocator", &d["resources"], fields);
        let stages = &d["observation"]["stages"];
        if let Some(exclusive) = stages["exclusive_ns"].as_object() {
            let mut sum = 0.0;
            for (name, value) in exclusive {
                sum += value.as_f64().unwrap_or(0.0);
                fields
                    .entry(format!("Stage / {} ns", title(name)))
                    .or_default()
                    .push(value.clone());
            }
            for name in ["total_ns", "unattributed_ns"] {
                fields
                    .entry(format!("Stage / {}", title(name)))
                    .or_default()
                    .push(stages[name].clone());
            }
            let residual = stages["total_ns"].as_f64().unwrap_or(0.0)
                - sum
                - stages["unattributed_ns"].as_f64().unwrap_or(0.0);
            fields
                .entry("Stage / partition residual ns".into())
                .or_default()
                .push(Value::from(residual));
        }
        if let Some(blocks) = d["observation"]["block_timeline"].as_array() {
            for (index, block) in blocks.iter().enumerate() {
                for (label, start, end) in [
                    (
                        "start to ready",
                        block["start_ns"].as_f64(),
                        block["output_ready_ns"].as_f64(),
                    ),
                    (
                        "ready to release",
                        block["output_ready_ns"].as_f64(),
                        block["release_boundary_ns"].as_f64(),
                    ),
                    (
                        "previous release to start",
                        index
                            .checked_sub(1)
                            .and_then(|i| blocks[i]["release_boundary_ns"].as_f64()),
                        block["start_ns"].as_f64(),
                    ),
                ] {
                    if let (Some(start), Some(end)) = (start, end) {
                        fields
                            .entry(format!(
                                "Continuity / worker {} / {label} ns",
                                string(&d["worker"])
                            ))
                            .or_default()
                            .push(Value::from(end - start));
                    }
                }
            }
        }
    }
    if cases.is_empty() {
        return;
    }
    out.push_str("<details>\n<summary>Stage, work and resource details</summary>\n\nNumeric cells: mean [min–max; n]; a single number means all n observations agree. Stage partition residual must be zero before rounding. Continuity intervals are local to each worker.\n\n");
    fn metric(values: &[Value]) -> String {
        if let Some(numbers) = values.iter().map(Value::as_f64).collect::<Option<Vec<_>>>() {
            let min = numbers.iter().copied().reduce(f64::min).unwrap_or(0.0);
            let max = numbers.iter().copied().reduce(f64::max).unwrap_or(0.0);
            if min == max {
                format!("{min:.1} (n={})", numbers.len())
            } else {
                format!(
                    "{:.1} [{min:.1}–{max:.1}; n={}]",
                    mean(&numbers),
                    numbers.len()
                )
            }
        } else {
            let mut text: Vec<_> = values.iter().map(string).collect();
            text.sort();
            text.dedup();
            text.join("; ")
        }
    }
    let names: std::collections::BTreeSet<_> =
        cases.values().flat_map(|f| f.keys().cloned()).collect();
    let mut shared = Vec::new();
    let mut varying = Vec::new();
    for name in names {
        let values: Vec<_> = cases
            .values()
            .map(|f| {
                f.get(&name)
                    .map(|v| metric(v))
                    .unwrap_or_else(|| "Not observed".into())
            })
            .collect();
        if values.iter().all(|v| *v == values[0]) {
            shared.push(vec![name, values[0].clone()]);
        } else {
            varying.push(name);
        }
    }
    if !shared.is_empty() {
        out.push_str("Shared by every case in this group:\n");
        table(out, &["Metric", "Value"], shared);
    }
    for names in varying.chunks(4) {
        let mut headers = vec!["Case suffix"];
        headers.extend(names.iter().map(String::as_str));
        table(
            out,
            &headers,
            cases
                .iter()
                .map(|(case, fields)| {
                    let mut row = vec![short_case(case, prefix)];
                    row.extend(names.iter().map(|name| {
                        fields
                            .get(name)
                            .map(|v| metric(v))
                            .unwrap_or_else(|| "Not observed".into())
                    }));
                    row
                })
                .collect(),
        );
    }
    out.push_str("</details>\n\n");
}

fn coverage(out: &mut String, r: &Record) {
    let coverage = crate::coverage::report(r);
    let Some(rows) = coverage["cases"].as_array().filter(|rows| !rows.is_empty()) else {
        out.push_str("The test plan was not resolved. No cases are reported as passed or skipped; see the execution error.\n\n");
        return;
    };
    for (category, label) in CATEGORIES {
        let _ = writeln!(out, "**{label}**\n");
        let cases: Vec<_> = rows.iter().filter(|v| v["category"] == category).collect();
        let mut totals = BTreeMap::<(String, String), usize>::new();
        for row in &cases {
            for phase in ["validation", "measurement", "diagnostics"] {
                *totals
                    .entry((phase.into(), string(&row[phase]["status"])))
                    .or_default() += 1;
            }
        }
        table(
            out,
            &["Phase", "Status", "Cases"],
            totals
                .into_iter()
                .map(|((phase, status), n)| vec![phase, status, n.to_string()])
                .collect(),
        );
        let mut gaps = BTreeMap::<(String, String, String, String), usize>::new();
        for row in cases {
            for phase in ["validation", "measurement", "diagnostics"] {
                let status = row[phase]["status"].as_str().unwrap_or("");
                if matches!(status, "skipped" | "not_executed" | "incomplete") {
                    let reason = &row[phase]["reason"];
                    let message = reason["reason"]
                        .as_str()
                        .or_else(|| reason.as_str())
                        .unwrap_or(
                            "Expected observations were not completed; see execution outcome",
                        );
                    *gaps
                        .entry((
                            string(&row["family"]),
                            phase.into(),
                            status.into(),
                            message.into(),
                        ))
                        .or_default() += 1;
                }
            }
        }
        if !gaps.is_empty() {
            table(
                out,
                &["Test family", "Phase", "Status", "Cases", "Reason"],
                gaps.into_iter()
                    .map(|((family, phase, status, reason), n)| {
                        vec![family, phase, status, n.to_string(), reason]
                    })
                    .collect(),
            );
        }
    }
}
fn provenance(out: &mut String, id: u32, r: &Record, shared_environment: Option<u32>) {
    let _ = writeln!(
        out,
        "### C{id}: inputs, identity and host\n\nEngine request: {}.\n\nOperator request: {}.\n\nScheme: **{}**, mode **{}**.\n",
        cell(&reference(r, "engine")),
        cell(&reference(r, "operator")),
        cell(&r.request.scheme.name),
        cell(&r.request.scheme.mode)
    );
    table(
        out,
        &["Execution phase", "Wall time"],
        r.timing
            .phases_ns
            .iter()
            .map(|(k, v)| vec![title(k), seconds(*v)])
            .collect(),
    );
    let Some(q) = &r.quadruple else {
        out.push_str("No complete (C,I,T,E) identity: this invocation ended before all identity inputs were available.\n\n");
        return;
    };
    table(
        out,
        &["Identity", "Version / value", "Content digest"],
        vec![
            vec!["I".into(), q.i.id.clone(), q.i.digest.clone()],
            vec!["T".into(), q.t.id.clone(), q.t.digest.clone()],
            vec![
                "E".into(),
                "Recorded common experiment conditions".into(),
                q.e.digest.clone(),
            ],
        ],
    );
    for role in ["engine", "operator", "scheme"] {
        let c = &q.c[role];
        if c.is_object() {
            let _ = writeln!(
                out,
                "- {}: {} {}; source SHA-256 {}; revision {}.\n",
                title(role),
                cell(&string(&c["package"])),
                cell(&string(&c["version"])),
                cell(&string(&c["source_digest"])),
                cell(&string(&c["origin"]["revision"]))
            );
        }
    }
    if let Some(other) = shared_environment {
        let _ = writeln!(
            out,
            "Environment: identical to C{other}; see that component's conditions.\n"
        );
        return;
    }
    let mut fields = BTreeMap::new();
    flatten("Environment", &q.e.conditions, &mut fields);
    table(
        out,
        &["Condition", "Recorded value"],
        fields
            .into_iter()
            .map(|(k, v)| {
                vec![
                    title(&k),
                    v.iter().map(string).collect::<Vec<_>>().join("; "),
                ]
            })
            .collect(),
    );
}
pub fn comparison(value: &Value) -> String {
    let mut out = format!(
        "# Fountain comparison\n\nBaseline: **{}**. Candidate: **{}**. Status: **{}**. Attribution: **{}**.\n\n",
        cell(&string(&value["left"])),
        cell(&string(&value["right"])),
        cell(&string(&value["status"])),
        cell(&string(&value["attribution"]))
    );
    out.push_str("Only common measured cases are compared after T/E/I validation. Negative time change means the candidate is faster. Warnings remain eligible for comparison. Different amounts of work, allocation scopes and interfaces require joint interpretation; this report does not assign a causal speedup to one internal optimization.\n\n");
    let rows = value["rows"].as_array().cloned().unwrap_or_default();
    let improved = rows
        .iter()
        .filter(|v| v["right_median_ns"].as_f64() < v["left_median_ns"].as_f64())
        .count();
    let regressed = rows
        .iter()
        .filter(|v| v["right_median_ns"].as_f64() > v["left_median_ns"].as_f64())
        .count();
    let _ = writeln!(
        out,
        "Measured directions: {improved} lower candidate medians, {regressed} higher, {} equal. These counts are descriptive; no statistical significance or global ranking is implied.\n",
        rows.len() - improved - regressed
    );
    for (category, label) in CATEGORIES.iter().skip(1) {
        let _ = writeln!(out, "## {label}\n");
        let mut groups = BTreeMap::<String, Vec<&Value>>::new();
        for r in &rows {
            let case = r["case"].as_str().unwrap_or("");
            if case.starts_with(category) {
                groups.entry(group(case)).or_default().push(r);
            }
        }
        if groups.is_empty() {
            out.push_str("No common measured cases in this category.\n\n");
        }
        for (prefix, rows) in groups {
            let _ = writeln!(
                out,
                "### {}\n\nCase prefix: `{}`. CV columns are percentages.\n",
                title(prefix.split('/').nth(1).unwrap_or("")),
                cell(&prefix)
            );
            for chunk in rows.chunks(PAGE) {
                table(
                    &mut out,
                    &[
                        "Case suffix",
                        "Baseline ns",
                        "Candidate ns",
                        "Time change",
                        "CV baseline/candidate %",
                    ],
                    chunk
                        .iter()
                        .map(|v| {
                            let a = v["left_median_ns"].as_f64().unwrap();
                            let b = v["right_median_ns"].as_f64().unwrap();
                            vec![
                                short_case(v["case"].as_str().unwrap(), &prefix),
                                format!("{a:.0}"),
                                format!("{b:.0}"),
                                format!("{:+.2}%", (b / a - 1.0) * 100.0),
                                format!(
                                    "{:.2}/{:.2}",
                                    v["left_cv"].as_f64().unwrap() * 100.0,
                                    v["right_cv"].as_f64().unwrap() * 100.0
                                ),
                            ]
                        })
                        .collect(),
                );
                table(
                    &mut out,
                    &[
                        "Case suffix",
                        "Source Gbit/s baseline/candidate",
                        "Calls/s baseline/candidate",
                        "Completed baseline/candidate",
                        "Warnings baseline/candidate",
                    ],
                    chunk
                        .iter()
                        .map(|v| {
                            vec![
                                short_case(v["case"].as_str().unwrap(), &prefix),
                                format!(
                                    "{}/{}",
                                    number(v["left_source_payload_gbps"].as_f64()),
                                    number(v["right_source_payload_gbps"].as_f64())
                                ),
                                format!(
                                    "{}/{}",
                                    number(v["left_operations_per_second"].as_f64()),
                                    number(v["right_operations_per_second"].as_f64())
                                ),
                                format!("{}/{}", v["left_completed"], v["right_completed"]),
                                format!(
                                    "{}/{}",
                                    v["left_warnings"].as_array().map_or(0, Vec::len),
                                    v["right_warnings"].as_array().map_or(0, Vec::len)
                                ),
                            ]
                        })
                        .collect(),
                );
            }
        }
    }
    out.push_str("## Coverage and semantic warnings\n");
    for side in ["left", "right"] {
        let _ = writeln!(
            out,
            "### {}\n",
            if side == "left" {
                "Baseline"
            } else {
                "Candidate"
            }
        );
        let c = &value["coverage"][side];
        let _ = writeln!(
            out,
            "Registered cases: {}; skipped validation: {}; skipped measurement: {}.\n",
            c["registered_cases"], c["skipped_validation"], c["skipped_measurement"]
        );
        if let Some(rows) = c["cases"].as_array() {
            let mut reasons = BTreeMap::<(String, String, String), usize>::new();
            for r in rows {
                for phase in ["validation", "measurement"] {
                    if r[phase]["status"] == "skipped" {
                        *reasons
                            .entry((
                                string(&r["family"]),
                                phase.into(),
                                string(&r[phase]["reason"]["reason"]),
                            ))
                            .or_default() += 1;
                    }
                }
            }
            table(
                &mut out,
                &["Family", "Phase", "Skipped cases", "Reason"],
                reasons
                    .into_iter()
                    .map(|((f, p, reason), n)| vec![f, p, n.to_string(), reason])
                    .collect(),
            );
        }
        if let Some(warnings) = value["correctness"][side]["warnings"].as_array() {
            table(
                &mut out,
                &["Warning case", "Expected versus observed"],
                warnings
                    .iter()
                    .map(|w| vec![string(&w["case"]), difference(&w["expected"], &w["actual"])])
                    .collect(),
            );
        }
    }
    out.push_str("\nRead each run's chain and workflow observations for operation counts, received symbols, stage shares and allocation domains. A stage shift is an investigation lead; ordinary performance and diagnostic timings are separate measurements.\n");
    out
}

/// Compiler/host diagnostics remain readable after temporary logs are removed.
pub fn failure_logs(out: &mut String, saved: &crate::artifact::Snapshot) {
    for c in &saved.components {
        if c.failure_details.is_empty() && c.aggregation_error.is_none() {
            continue;
        }
        let _ = writeln!(
            out,
            "<details>\n<summary>C{}: failure diagnostics</summary>\n",
            c.component_id
        );
        if let Some(error) = &c.aggregation_error {
            let _ = writeln!(
                out,
                "Partial samples could not be aggregated: {}. Counts and failed coverage are retained.\n",
                cell(error)
            );
        }
        for text in &c.failure_details {
            for line in text.lines() {
                let _ = writeln!(out, "> {}", cell(line));
            }
            out.push('\n');
        }
        out.push_str("</details>\n\n");
    }
}
