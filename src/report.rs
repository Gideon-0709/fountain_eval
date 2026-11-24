//! Markdown reports: one self-contained reading surface, backed by sealed evidence.
use crate::{
    artifact::Component,
    content::{SOURCE_COUNTS, SYMBOL_WIDTHS},
    evidence::Record,
    measurement::Summary,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

const PAGE: usize = 12;
const REPORT_HEADER: &str = concat!(
    "# Fountain evaluation report\n\n",
    "[Summary](#summary) · [V · Validation](#validation-tests-v) · ",
    "[O · Operations](#operation-tests-o) · [L · Links](#link-tests-l) · ",
    "[W · Workflows](#workflow-tests-w)\n\n",
);
const CATEGORIES: [(&str, &str); 4] = [
    ("correctness", "Validation tests"),
    ("operation", "Operation tests"),
    ("chain", "Link tests"),
    ("workflow", "Workflow tests"),
];
const FAMILIES: [(&str, &[&str]); 4] = [
    (
        "V",
        &[
            "Participation",
            "Arithmetic values",
            "Storage and ownership",
            "Mixed operations",
            "Dispatch and composition",
            "Boundary contracts",
            "Scheme and padding",
            "Plan application and reuse",
            "Encoding and recovery",
            "Receive progression",
            "Errors and lifecycle",
        ],
    ),
    (
        "O",
        &[
            "Insert at a new ID or overwrite an existing ID",
            "Look up a symbol and return its reference",
            "Zero one symbol, a list or a range",
            "XOR one or more sources into one target",
            "Broadcast one source by XOR into four or eight targets",
            "Multiply or multiply-add in GF(256)",
            "Copy or move one symbol or a range",
            "Share a symbol and time the first write with and without sharing",
            "Remove a symbol or record an ID association",
        ],
    ),
    (
        "L",
        &[
            "Context setup",
            "Plan generation",
            "Operation-list clone",
            "Loading and padding",
            "Plan application",
            "Symbol generation",
            "Decode ingestion",
            "Solve progression",
            "Output materialization",
            "Release",
            "Encode replay",
            "Decode replay",
        ],
    ),
    (
        "W",
        &[
            "First use",
            "Per-block encoding",
            "Plan reuse",
            "All-source decode",
            "Missing-source recovery",
            "Round trip",
            "Block continuity",
            "Component lifecycle",
        ],
    ),
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
        "failed" if !c.correctness.fatal_failures.is_empty() => {
            format!("FAIL · {} fatal", c.correctness.fatal_failures.len())
        }
        "failed" => format!("FAIL · {}", title(&c.stage)),
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

fn decimal(n: f64) -> String {
    if n > 0.0 && n < 0.001 {
        "<0.001".into()
    } else {
        format!("{n:.3}")
    }
}
#[derive(Clone, Copy)]
enum Metric {
    SourceRate,
    RepairRate,
    NanosecondsPerCall,
    Milliseconds,
    LifecycleMilliseconds,
}

fn metric(c: &Component, id: &str, kind: Metric) -> String {
    if c.plan
        .as_ref()
        .and_then(|p| p.checks.get(id))
        .is_some_and(|check| check.skip.is_some() || check.measurement_skip.is_some())
    {
        return "Skipped".into();
    }
    let Some(s) = c
        .summary
        .iter()
        .find(|s| s.case == id)
        .filter(|_| c.status == "succeeded")
    else {
        return "Not measured".into();
    };
    if s.incomplete > 0 {
        return format!("Incomplete ({}/{} recovered)", s.completed, s.samples);
    }
    let value = match kind {
        Metric::SourceRate => s.source_payload_gbps,
        Metric::RepairRate => s.repair_output_gbps,
        Metric::NanosecondsPerCall => s.ns_per_operation,
        Metric::Milliseconds => Some(s.median_ns as f64 / 1e6),
        Metric::LifecycleMilliseconds => s
            .workload
            .get("lifecycle_ns")
            .and_then(|v| v["median"].as_f64())
            .map(|ns| ns / 1e6),
    };
    let variation = if matches!(kind, Metric::LifecycleMilliseconds) {
        ""
    } else {
        mark(s)
    };
    value
        .map(|n| format!("{}{variation}", decimal(n)))
        .unwrap_or_else(|| "Unavailable".into())
}
const OPS: &[(&str, &str)] = &[
    ("insert-new", "Insert a symbol at a new ID"),
    ("read", "Look up a stored symbol reference"),
    ("zero-one", "Zero one stored symbol"),
    ("xor-one", "XOR one source into one target"),
    ("xor-many-4", "XOR four sources into one target"),
    (
        "broadcast-4",
        "Broadcast XOR from one source into four targets",
    ),
    ("scale-7", "Multiply one symbol by 7"),
    (
        "mul-add-7",
        "Multiply a source by 7 and XOR into one target",
    ),
    ("copy-one", "Copy one symbol to another ID"),
    ("move-one", "Move one symbol to another ID"),
    ("alias", "Share one symbol under a second ID"),
    ("remove", "Remove one stored symbol"),
];
const OPERATOR_HEADLINE_WIDTHS: [usize; 2] = [128, 1024];
const HEADLINE: [(usize, usize); 4] = [(512, 128), (512, 1024), (8192, 128), (8192, 1024)];
const STAGED: (usize, usize) = (512, 1024);

fn matrix<T>(
    out: &mut String,
    components: &[Component],
    headers: &[&str],
    rows: &[(Vec<String>, T)],
    render: impl Fn(&Component, &T) -> String,
) {
    for group in components.chunks(3) {
        let ids: Vec<_> = group
            .iter()
            .map(|c| format!("C{}", c.component_id))
            .collect();
        let columns: Vec<_> = headers
            .iter()
            .copied()
            .chain(ids.iter().map(String::as_str))
            .collect();
        table(
            out,
            &columns,
            rows.iter()
                .map(|(labels, case)| {
                    labels
                        .iter()
                        .cloned()
                        .chain(group.iter().map(|c| render(c, case)))
                        .collect()
                })
                .collect(),
        );
    }
}

fn recovery_id(scheme: &str, k: usize, width: usize, missing: usize) -> String {
    format!(
        "workflow/decode/{scheme}/k{k}/b{width}/m{missing}/source-first/s{}",
        crate::content::SEEDS[0]
    )
}

fn important(c: &crate::content::Case) -> bool {
    match c.category.as_str() {
        "operation" => {
            OPS.iter().any(|(operation, _)| *operation == c.workload)
                && OPERATOR_HEADLINE_WIDTHS.contains(&c.width)
                && c.instances == 64
        }
        "chain" => {
            matches!(c.workload.as_str(), "plan-build" | "plan-export")
                || (c.workload.ends_with("-replay")
                    && crate::content::SOURCE_COUNTS.contains(&c.k)
                    && c.width == 128)
        }
        "workflow" => {
            let at_headline = HEADLINE.contains(&(c.k, c.width));
            let at_staged = STAGED == (c.k, c.width);
            match c.workload.as_str() {
                "decode" if c.order == "source-first" && c.seed == crate::content::SEEDS[0] => {
                    (at_staged && c.missing == 0) || (at_headline && c.missing == c.k.div_ceil(10))
                }
                "encode-repairs" => at_headline && c.repairs == 8,
                "round-trip" => at_headline,
                "reuse-encode" => at_staged && c.repairs == 8,
                "first-encode" | "continuous-rebuild" | "continuous-reuse" => at_staged,
                _ => false,
            }
        }
        _ => false,
    }
}
fn report_metadata(
    run: &str,
    config: &std::path::Path,
    request: &crate::protocol::EvaluationRequest,
    elapsed: u64,
    status: &str,
) -> Vec<Vec<String>> {
    vec![
        vec!["Run".into(), run.into()],
        vec!["Configuration".into(), config.display().to_string()],
        vec![
            "Scheme".into(),
            format!(
                "{} · {}",
                request.scheme.name.to_uppercase(),
                request.scheme.mode
            ),
        ],
        vec!["Test content version".into(), request.test.version.clone()],
        vec!["Run wall time".into(), seconds(elapsed)],
        vec!["Overall result".into(), title(status)],
    ]
}

fn summary(
    out: &mut String,
    components: &[Component],
    requests: &[(u32, &crate::protocol::EvaluationRequest)],
    metadata: Vec<Vec<String>>,
    results: Vec<Vec<String>>,
) {
    out.push_str("## Summary\n\n### Test results\n\n");
    table(
        out,
        &["C", "Result", "Fatal", "Warning", "Performance"],
        results,
    );
    out.push_str("### Test configuration\n\n");
    table(out, &["Setting", "Value"], metadata);
    table(
        out,
        &["C", "Engine", "Operator", "Interface", "Cargo options"],
        requests
            .iter()
            .map(|(id, request)| {
                let component = components.iter().find(|c| c.component_id == *id);
                let options =
                    |role: &str, reference: Option<&crate::protocol::ComponentReference>| {
                        reference.map(|r| {
                            format!(
                                "{role} · features [{}], defaults {}",
                                r.build.features.join(", "),
                                r.build.default_features
                            )
                        })
                    };
                let cargo: Vec<_> = [
                    options("engine", request.components.engine.as_ref()),
                    options("operator", request.components.operator.as_ref()),
                ]
                .into_iter()
                .flatten()
                .collect();
                vec![
                    format!("C{id}"),
                    component.map_or_else(
                        || crate::progress::source(request.components.engine.as_ref()),
                        |c| identity(c, "engine"),
                    ),
                    component.map_or_else(
                        || crate::progress::source(request.components.operator.as_ref()),
                        |c| identity(c, "operator"),
                    ),
                    component
                        .and_then(|c| c.quadruple.as_ref())
                        .map_or_else(|| "Unavailable".into(), |q| q.i.id.clone()),
                    cargo.join(" / "),
                ]
            })
            .collect(),
    );
    out.push_str("### Test environment\n\n");
    let mut environments = BTreeMap::<&str, Vec<&Component>>::new();
    for c in components {
        if let Some(q) = &c.quadruple {
            environments.entry(&q.e.digest).or_default().push(c);
        }
    }
    for group in environments.values() {
        let q = group[0].quadruple.as_ref().unwrap();
        let runtime = &q.e.conditions["runtime"];
        let build = &q.e.conditions["build"];
        let cpu = &runtime["cpu"];
        let model = cpu["brand"]
            .as_str()
            .or_else(|| cpu["cpu"]["model name"].as_str())
            .or_else(|| cpu["model"].as_str())
            .or_else(|| cpu.as_str())
            .unwrap_or("Recorded in index");
        let memory = cpu["memory"]
            .as_str()
            .map(|memory| {
                memory.parse::<u64>().map_or_else(
                    |_| memory.to_owned(),
                    |bytes| format!("{:.1} GiB", bytes as f64 / 1073741824.0),
                )
            })
            .unwrap_or_else(|| "Unavailable".into());
        let rustc = build["rustc"]
            .as_str()
            .and_then(|text| text.lines().next())
            .unwrap_or("Unavailable");
        let names = group
            .iter()
            .map(|c| format!("C{}", c.component_id))
            .collect::<Vec<_>>()
            .join(", ");
        table(
            out,
            &["Environment", &names],
            vec![
                vec![
                    "Host".into(),
                    format!("{} / {}", string(&runtime["os"]), string(&runtime["arch"])),
                ],
                vec!["CPU".into(), model.into()],
                vec!["Memory".into(), memory],
                vec!["Rust compiler".into(), rustc.into()],
                vec![
                    "Build".into(),
                    format!(
                        "{} · {}",
                        string(&build["profile"]),
                        string(&build["rustflags"])
                    ),
                ],
            ],
        );
    }
    let unresolved: Vec<_> = components
        .iter()
        .filter(|c| c.quadruple.is_none())
        .map(|c| {
            vec![
                format!("C{}", c.component_id),
                "Unavailable · stopped before environment/interface identity was completed".into(),
            ]
        })
        .collect();
    table(out, &["C", "Environment"], unresolved);
}

fn result_row(c: &Component) -> Vec<String> {
    vec![
        format!("C{}", c.component_id),
        outcome(c),
        c.correctness.fatal_failures.len().to_string(),
        c.correctness.warnings.len().to_string(),
        if c.sample_count == 0 {
            "Not measured".into()
        } else {
            performance_summary(c)
        },
    ]
}

pub fn snapshot(saved: &crate::artifact::Snapshot) -> String {
    let b = &saved.batch;
    let mut out = REPORT_HEADER.to_owned();
    summary(
        &mut out,
        &saved.components,
        &b.configuration
            .selections
            .iter()
            .map(|s| (s.id, &s.request))
            .collect::<Vec<_>>(),
        report_metadata(
            &b.run_id,
            &b.configuration.path,
            &b.configuration.selections[0].request,
            b.elapsed_ns,
            &b.status,
        ),
        b.entries
            .iter()
            .map(|entry| {
                saved
                    .components
                    .iter()
                    .find(|c| c.component_id == entry.component_id)
                    .map_or_else(
                        || {
                            vec![
                                format!("C{}", entry.component_id),
                                format!(
                                    "{} · {}",
                                    title(&entry.status),
                                    entry
                                        .error
                                        .as_deref()
                                        .unwrap_or("No completed component record")
                                ),
                                "Not checked".into(),
                                "Not checked".into(),
                                "Not measured".into(),
                            ]
                        },
                        result_row,
                    )
            })
            .collect(),
    );
    sections(&mut out, &saved.components);
    out
}

pub fn markdown(r: &Record) -> String {
    match crate::artifact::Component::from_record(r) {
        Ok(mut c) => {
            c.correctness = r.correctness.clone();
            let mut out = REPORT_HEADER.to_owned();
            summary(
                &mut out,
                std::slice::from_ref(&c),
                &[(c.component_id, &r.request)],
                report_metadata(
                    &r.run_id,
                    &r.configuration.path,
                    &r.request,
                    r.timing.elapsed_ns,
                    &r.status,
                ),
                vec![result_row(&c)],
            );
            sections(&mut out, &[c]);
            out
        }
        Err(e) => format!(
            "# Fountain evaluation report\n\nRun: {} · Report unavailable: {}.\n",
            cell(&r.run_id),
            cell(&e)
        ),
    }
}

fn sections(out: &mut String, components: &[Component]) {
    if components
        .iter()
        .filter_map(|c| c.plan.as_ref())
        .any(|plan| {
            plan.definition["symbol_widths"] != serde_json::json!(SYMBOL_WIDTHS)
                || plan.definition["source_counts"] != serde_json::json!(SOURCE_COUNTS)
        })
    {
        out.push_str("> Earlier test matrix. Coverage counts describe the original run. The tables use the current K and B values and mark missing observations as Not measured. Measurements outside the displayed ranges are available in the result index.\n\n");
    }
    if components.iter().any(|c| {
        c.status == "succeeded" && c.summary.iter().any(|s| s.coefficient_of_variation > 0.10)
    }) {
        out.push_str("† marks a timing standard deviation above 10% of the mean.\n\n");
    }
    for render in [
        validation_results,
        operation_results,
        link_results,
        workflow_results,
    ] {
        out.push_str("---\n\n");
        render(out, components);
    }
}

fn validation_results(out: &mut String, components: &[Component]) {
    out.push_str("## Validation tests (V)\n\n### Summary\n\n");
    table(
        out,
        &[
            "C",
            "Correctness",
            "Checked / applicable",
            "Not checked",
            "Fatal cases",
            "Warning cases",
        ],
        components
            .iter()
            .map(|c| {
                let (checked, pending) = if c.coverage["status"] == "not_planned" {
                    ("Plan unavailable".into(), "—".into())
                } else {
                    let counts = Coverage::Validation.counts(c);
                    (
                        format!("{}/{}", counts.completed, counts.applicable()),
                        counts.pending.to_string(),
                    )
                };
                vec![
                    format!("C{}", c.component_id),
                    title(&c.correctness.status),
                    checked,
                    pending,
                    c.correctness.fatal_failures.len().to_string(),
                    c.correctness.warnings.len().to_string(),
                ]
            })
            .collect(),
    );
    out.push_str("Problems are grouped by observed behavior, with affected components, test families and case counts. Each fatal problem has one representative diagnostic.\n\n");
    findings(out, components);
    out.push_str("### Detailed results\n\n");
    exclusion_reasons(out, components, Coverage::Validation);
    family_coverage(out, components, "V");
}

fn accepts(c: &Component, prefix: &str) -> bool {
    c.status == "succeeded" && c.summary.iter().any(|s| s.case.starts_with(prefix))
}

fn performance_summary(c: &Component) -> String {
    if c.status != "succeeded" {
        return format!("Not measured · {} unaccepted samples", c.sample_count);
    }
    let count = Coverage::Timing(None).counts(c).completed;
    let incomplete = c.summary.iter().filter(|s| s.incomplete > 0).count();
    if incomplete == 0 {
        format!("{count} accepted cases · {} samples", c.sample_count)
    } else {
        format!("{count} accepted cases · {incomplete} with incomplete recovery")
    }
}

fn operation_results(out: &mut String, components: &[Component]) {
    out.push_str("## Operation tests (O)\n\n### Summary\n\n");
    let scope = Coverage::Timing(Some("operation"));
    measurement_summary(out, components, &[scope]);
    out.push_str("### Key results · Direct call latency (ns/call)\n\n");
    let operations = crate::content::operations();
    let columns = SYMBOL_WIDTHS.map(|b| format!("{b} B"));
    let headers: Vec<_> = std::iter::once("One timed call")
        .chain(columns.iter().map(String::as_str))
        .collect();
    let mut measured = false;
    for c in components.iter().filter(|c| accepts(c, "operation/")) {
        if !measured {
            out.push_str("Median batch time divided by 64 independent direct calls. Storage is prepared before timing. Each column is bytes per symbol.\n\n");
            measured = true;
        }
        let _ = writeln!(out, "C{}\n", c.component_id);
        let rows = OPS
            .iter()
            .map(|&(name, description)| {
                let (_, family, _) = operations
                    .iter()
                    .find(|(workload, _, _)| workload == name)
                    .expect("representative operation is in the test catalog");
                let mut row = vec![format!("{family} · {description}")];
                for width in SYMBOL_WIDTHS {
                    let id = format!("operation/{name}/b{width}/direct/small");
                    row.push(metric(c, &id, Metric::NanosecondsPerCall));
                }
                row
            })
            .collect();
        table(out, &headers, rows);
    }
    if !measured {
        out.push_str("Not measured.\n\n");
    }
    out.push_str("### Detailed results\n\n");
    measurement_details(out, components, scope);
    family_coverage(out, components, "O");
    if components
        .iter()
        .any(|c| scope.rows(c).any(|(_, check)| !check.spec.direct))
    {
        out.push_str(
            "This run's coverage includes Execute cases. Their timings are in the index.\n\n",
        );
    }
}

fn link_results(out: &mut String, components: &[Component]) {
    out.push_str("## Link tests (L)\n\n### Summary\n\n");
    let independent = Coverage::Timing(Some("chain"));
    measurement_summary(out, components, &[independent, Coverage::LinkStages]);
    let (k, width) = STAGED;
    let missing = k.div_ceil(10);
    out.push_str("### Key results · Encode and decode stages\n\n");
    if components.iter().any(|c| accepts(c, "workflow/")) {
        out.push_str("Stage times are means from the instrumented run. Percentages use the full instrumented lifecycle for the same block. Combined intervals count once.\n\n");
        for (encode, direction) in [(true, "Encode"), (false, "Decode")] {
            let _ = writeln!(out, "#### {direction} · mean µs/block\n");
            let mut any = false;
            for c in components.iter().filter(|c| c.status == "succeeded") {
                let scheme = &c.request.scheme.name;
                let id = if encode {
                    format!("workflow/encode-repairs/{scheme}/k{k}/b{width}/r8")
                } else {
                    recovery_id(scheme, k, width, missing)
                };
                if let Some(rows) = stage_rows(c, &id) {
                    any = true;
                    let workload = if encode {
                        "8 repair symbols generated".into()
                    } else {
                        format!(
                            "{missing} missing source symbols (10% rounded up) · source-first · seed {}",
                            crate::content::SEEDS[0]
                        )
                    };
                    let _ = writeln!(
                        out,
                        "C{} · {} · K={k} · B={width} bytes/symbol · {workload}\n",
                        c.component_id,
                        scheme.to_uppercase(),
                    );
                    table(
                        out,
                        &["Stage", "Mean µs/block", "Share of lifecycle (%)"],
                        rows,
                    );
                }
            }
            if !any {
                out.push_str("Not measured.\n\n");
            }
        }
    } else {
        out.push_str("Encode and decode stages were not measured.\n\n");
    }

    out.push_str("### Detailed results\n\n");
    if components.iter().any(|c| accepts(c, "chain/")) {
        out.push_str("#### Encoding plans (L02–L03) · median ms/call\n\n");
        out.push_str("L02 builds a new plan. L03 clones the stored instructions. L05 covers applying the plan to source data inside the Encode workflow.\n\n");
        link_scale_table(
            out,
            components,
            "Workload",
            &[
                ("plan-build", "L02 · Build a new plan".into(), None),
                (
                    "plan-export",
                    "L03 · Clone an existing instruction list".into(),
                    None,
                ),
            ],
        );

        for (work, heading, scenario) in [
            (
                "encode-replay",
                "Encode replay (L11) · median ms/replay",
                "Replays the recorded calls for encoding 8 repair symbols.",
            ),
            (
                "decode-replay",
                "Decode replay (L12) · median ms/replay",
                "Replays the recorded recovery calls for 10% source loss rounded up. Surviving sources arrive first.",
            ),
        ] {
            let _ = writeln!(out, "#### {heading}\n\n{scenario}\n");
            let replay: Vec<_> = SYMBOL_WIDTHS
                .into_iter()
                .map(|width| (work, width.to_string(), Some(width)))
                .collect();
            link_scale_table(out, components, "B (bytes/symbol)", &replay);
        }
    } else {
        out.push_str("Independent plan and replay timings were not measured.\n\n");
    }
    out.push_str("#### Link coverage details\n\n");
    measurement_details(out, components, independent);
    measurement_details(out, components, Coverage::LinkStages);
    family_coverage(out, components, "L");
}

fn link_scale_table(
    out: &mut String,
    components: &[Component],
    first_column: &str,
    workloads: &[(&str, String, Option<usize>)],
) {
    let columns = SOURCE_COUNTS.map(|k| format!("K={k}"));
    let headers: Vec<_> = std::iter::once(first_column)
        .chain(columns.iter().map(String::as_str))
        .collect();
    for c in components.iter().filter(|c| accepts(c, "chain/")) {
        if components.len() > 1 {
            let _ = writeln!(out, "C{}\n", c.component_id);
        }
        let scheme = &c.request.scheme.name;
        let rows = workloads
            .iter()
            .map(|(work, label, width)| {
                let mut row = vec![label.clone()];
                for k in SOURCE_COUNTS {
                    let mut id = format!("chain/{work}/{scheme}/k{k}");
                    if let Some(width) = width {
                        let _ = write!(id, "/b{width}");
                    }
                    row.push(metric(c, &id, Metric::Milliseconds));
                }
                row
            })
            .collect();
        table(out, &headers, rows);
        if workloads
            .iter()
            .any(|(work, _, _)| *work == "decode-replay")
        {
            let missing = SOURCE_COUNTS
                .iter()
                .map(|k| format!("K={k} → {}", k.div_ceil(10)))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "Missing source symbols · {missing}\n");
        }
    }
}

fn stage_rows(c: &Component, id: &str) -> Option<Vec<Vec<String>>> {
    const STAGE_ORDER: &[&str] = &[
        "scheme-and-storage-setup",
        "decoder-context-setup",
        "source-loading-and-padding",
        "encoder-context-and-plan-application",
        "product-construction-loading-and-plan-application",
        "symbol-generation",
        "product-generation-and-output",
        "ingestion-and-solve",
        "output-materialization",
        "diagnostic-query-overhead",
        "component-release",
        "workflow-complete",
    ];
    let metrics = c.diagnostic_summaries.get(id)?.get("metrics")?;
    let total = &metrics["observation/stages/total_ns"];
    let count = total["n"].as_f64()?;
    let total = total["mean"].as_f64()?;
    if count <= 0.0 || total <= 0.0 {
        return None;
    }
    let micros = |ns: f64| decimal(ns / 1e3);
    let row = |name, ns| vec![name, micros(ns), format!("{:.1}%", ns / total * 100.0)];
    // Sum contributions over the same observations as the total, even if an
    // optional stage only appears in some of the instrumented executions.
    let mut stages: Vec<_> = metrics
        .as_object()?
        .iter()
        .filter_map(|(key, value)| {
            let stage = key.strip_prefix("observation/stages/exclusive_ns/")?;
            let mean = value["mean"].as_f64()? * value["n"].as_f64()? / count;
            let links = stage_families(metrics, stage);
            let label = if links.is_empty() {
                title(stage)
            } else {
                format!("{} ({links})", title(stage))
            };
            let order = STAGE_ORDER.iter().position(|&known| known == stage);
            Some((order.unwrap_or(STAGE_ORDER.len()), row(label, mean)))
        })
        .collect();
    stages.sort_by_key(|(order, _)| *order);
    let mut rows: Vec<_> = stages.into_iter().map(|(_, row)| row).collect();
    rows.push(row(
        "Unattributed".into(),
        metrics["observation/stages/unattributed_ns"]["mean"].as_f64()?,
    ));
    rows.push(row("Instrumented lifecycle total".into(), total));
    Some(rows)
}

fn stage_families(metrics: &Value, stage: &str) -> String {
    let families = metrics
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            let family = key
                .strip_prefix("observation/link_coverage/")?
                .strip_suffix("/interval")?;
            value["variants"]
                .as_array()?
                .iter()
                .any(|v| v["value"] == stage)
                .then_some(family)
        })
        .collect::<Vec<_>>();
    let combined = families.iter().any(|family| {
        metrics[format!("observation/link_coverage/{family}/scope")]["variants"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v["value"] == "combined parent"))
    });
    let links = families.join(", ");
    if combined {
        format!("{links} · combined")
    } else {
        links
    }
}

fn workflow_grid(
    out: &mut String,
    components: &[Component],
    render: impl Fn(&Component, usize, usize) -> String,
) {
    let columns = SOURCE_COUNTS.map(|k| format!("K={k}"));
    let headers: Vec<_> = std::iter::once("B (bytes/symbol)")
        .chain(columns.iter().map(String::as_str))
        .collect();
    for c in components.iter().filter(|c| accepts(c, "workflow/")) {
        if components.len() > 1 {
            let _ = writeln!(out, "C{}\n", c.component_id);
        }
        let rows = SYMBOL_WIDTHS
            .into_iter()
            .map(|width| {
                std::iter::once(width.to_string())
                    .chain(SOURCE_COUNTS.into_iter().map(|k| render(c, k, width)))
                    .collect()
            })
            .collect();
        table(out, &headers, rows);
    }
}

fn encode_phase_means(c: &Component, k: usize, width: usize) -> Option<(f64, f64)> {
    let id = format!(
        "workflow/encode-repairs/{}/k{k}/b{width}/r8",
        c.request.scheme.name
    );
    let diagnostic = c.diagnostic_summaries.get(&id)?;
    let count = diagnostic["observations"].as_u64().filter(|&n| n > 0)?;
    let metrics = &diagnostic["metrics"];

    let intervals = |families: &[&str]| -> Option<BTreeSet<&str>> {
        families
            .iter()
            .map(|family| {
                let variants =
                    metrics[format!("observation/link_coverage/{family}/interval")]["variants"]
                        .as_array()?;
                let [variant] = variants.as_slice() else {
                    return None;
                };
                if variant["n"].as_u64()? != count {
                    return None;
                }
                variant["value"].as_str()
            })
            .collect()
    };
    let precode = intervals(&["L01", "L04", "L05"])?;
    let repair = intervals(&["L06", "L09"])?;
    if !precode.is_disjoint(&repair) {
        return None;
    }
    let mean = |intervals: BTreeSet<&str>| -> Option<f64> {
        intervals
            .into_iter()
            .map(|interval| {
                let sample = &metrics[format!("observation/stages/exclusive_ns/{interval}")];
                if sample["n"].as_u64()? != count {
                    return None;
                }
                sample["mean"].as_f64().filter(|&ns| ns >= 0.0)
            })
            .sum::<Option<f64>>()
            .filter(|&ns| ns > 0.0)
    };
    Some((mean(precode)?, mean(repair)?))
}

fn workflow_results(out: &mut String, components: &[Component]) {
    out.push_str("## Workflow tests (W)\n\n### Summary\n\n");
    let scope = Coverage::Timing(Some("workflow"));
    measurement_summary(out, components, &[scope]);
    out.push_str("### Key results · Precode, Encode and Decode\n\n");
    let measured = components.iter().any(|c| accepts(c, "workflow/"));
    if measured {
        out.push_str("K is the source-symbol count and B is bytes per symbol. Each rate in decimal Gbit/s is 8 × counted bytes divided by elapsed nanoseconds.\n\n");
        out.push_str("#### Precode · source-equivalent Gbit/s\n\nCounts K × B source bytes. Uses mean instrumented W02 time from scheme and storage setup through source loading, padding and preparation of intermediate symbols.\n\n");
        workflow_grid(out, components, |c, k, width| {
            encode_phase_means(c, k, width).map_or_else(
                || "Not measured".into(),
                |(precode, _)| decimal(8.0 * (k * width) as f64 / precode),
            )
        });

        out.push_str("#### Encode · repair-output Gbit/s\n\nCounts R × B repair bytes with R=8. Uses mean instrumented W02 time from a ready encoder through generation and materialization of all eight repair symbols.\n\n");
        workflow_grid(out, components, |c, k, width| {
            let repair_bytes = 8 * width;
            encode_phase_means(c, k, width).map_or_else(
                || "Not measured".into(),
                |(_, repair)| decimal(8.0 * repair_bytes as f64 / repair),
            )
        });

        let _ = writeln!(
            out,
            "#### Decode · source-equivalent Gbit/s\n\nCounts K × B recovered source bytes. Uses median W05 time from decoder construction through input ingestion, recovery and materialization of the source block. Missing sources are 10% of K rounded up. Surviving sources come first (seed {}).\n",
            crate::content::SEEDS[0]
        );
        workflow_grid(out, components, |c, k, width| {
            metric(
                c,
                &recovery_id(&c.request.scheme.name, k, width, k.div_ceil(10)),
                Metric::SourceRate,
            )
        });
    } else {
        out.push_str("Not measured.\n\n");
    }

    out.push_str("### Detailed results\n\n");
    if measured {
        workflow_details(out, components);
    }
    measurement_details(out, components, scope);
    family_coverage(out, components, "W");
}

fn workflow_details(out: &mut String, components: &[Component]) {
    out.push_str("#### Round trip (W06) · median ms/block\n\nMeasures encoder construction, symbol generation, simulated omission and mixed-order delivery in memory, and decoding through recovered output. Each K omits 10% of source symbols rounded up.\n\n");
    workflow_grid(out, components, |c, k, width| {
        metric(
            c,
            &format!(
                "workflow/round-trip/{}/k{k}/b{width}",
                c.request.scheme.name
            ),
            Metric::Milliseconds,
        )
    });
    let (k, width) = STAGED;
    let _ = writeln!(
        out,
        "#### Other workloads at K={k}, B={width}\n\nMedian ms except where noted. The lifecycle includes component release.\n"
    );
    let rows = [
        (
            "W01 First encode, 8 repairs",
            "first-encode",
            Metric::Milliseconds,
        ),
        (
            "W02 End-to-end repair-output throughput (Gbit/s)",
            "encode-repairs",
            Metric::RepairRate,
        ),
        (
            "W03 Reused plan, 8 repairs",
            "reuse-encode",
            Metric::Milliseconds,
        ),
        (
            "W04 Decode all sources, ordered",
            "decode",
            Metric::Milliseconds,
        ),
        (
            "W07 Rebuild over 32 blocks",
            "continuous-rebuild",
            Metric::Milliseconds,
        ),
        (
            "W07 Reuse over 32 blocks",
            "continuous-reuse",
            Metric::Milliseconds,
        ),
        (
            "W08 Encode and release, 8 repairs",
            "encode-repairs",
            Metric::LifecycleMilliseconds,
        ),
    ]
    .map(|(label, name, kind)| (vec![label.into()], (name, kind)));
    matrix(out, components, &["Workflow"], &rows, |c, &(name, kind)| {
        let scheme = &c.request.scheme.name;
        let id = match name {
            "decode" => recovery_id(scheme, k, width, 0),
            "reuse-encode" | "encode-repairs" => {
                format!("workflow/{name}/{scheme}/k{k}/b{width}/r8")
            }
            _ => format!("workflow/{name}/{scheme}/k{k}/b{width}"),
        };
        metric(c, &id, kind)
    });
}

fn case_rows(c: &Component) -> impl Iterator<Item = &Value> {
    c.coverage["cases"].as_array().into_iter().flatten()
}

#[derive(Clone, Copy)]
enum Coverage {
    Validation,
    Timing(Option<&'static str>),
    LinkStages,
}

impl Coverage {
    fn field(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Timing(_) => "measurement",
            Self::LinkStages => "diagnostics",
        }
    }

    fn rows(
        self,
        component: &Component,
    ) -> impl Iterator<Item = (&Value, &crate::validation::Check)> {
        case_rows(component).filter_map(move |row| {
            let check = component.plan.as_ref()?.checks.get(row["case"].as_str()?)?;
            let spec = &check.spec;
            let applies = match self {
                Self::Validation => spec.validation_family().is_some(),
                Self::Timing(category) => {
                    check.measured && category.is_none_or(|category| spec.category == category)
                }
                // Staged observations use workflow cases that carry L families.
                Self::LinkStages => {
                    check.measured
                        && spec.category == "workflow"
                        && spec.family.split(',').any(|family| family.starts_with('L'))
                }
            };
            applies.then_some((row, check))
        })
    }

    fn counts(self, c: &Component) -> Counts {
        self.count_rows(c, self.rows(c).map(|(row, _)| row))
    }

    fn count_rows<'a>(self, c: &Component, rows: impl Iterator<Item = &'a Value>) -> Counts {
        let mut counts = Counts::default();
        for row in rows {
            let entry = &row[self.field()];
            match entry["status"].as_str() {
                Some("skipped") => counts.excluded += 1,
                Some("not_executed") => counts.pending += 1,
                Some("passed") => counts.completed += 1,
                Some("failed") => {
                    counts.completed += 1;
                    if entry["severity"] == "fatal" {
                        counts.fatals += 1;
                    } else {
                        counts.warnings += 1;
                    }
                }
                Some("recorded") => {
                    let accepted = match self {
                        Self::LinkStages => c.status == "succeeded",
                        _ => entry["accepted"] == true,
                    };
                    if accepted {
                        counts.completed += 1;
                    } else {
                        counts.rejected += 1;
                    }
                }
                Some("incomplete") => counts.partial += 1,
                _ => {}
            }
        }
        counts
    }
}

#[derive(Default)]
struct Counts {
    completed: usize,
    pending: usize,
    excluded: usize,
    partial: usize,
    rejected: usize,
    fatals: usize,
    warnings: usize,
}
impl Counts {
    fn applicable(&self) -> usize {
        self.completed + self.pending + self.partial + self.rejected
    }
}

fn measurement_summary(out: &mut String, components: &[Component], scopes: &[Coverage]) {
    let multiple_scopes = scopes.len() > 1;
    let mut headers = vec![
        "C",
        "Applicable cases",
        "Accepted cases",
        "Not started",
        "Excluded",
    ];
    if multiple_scopes {
        headers.insert(1, "Scope");
    }
    let rows = components
        .iter()
        .flat_map(|c| {
            scopes.iter().map(move |&scope| {
                let mut row = vec![format!("C{}", c.component_id)];
                if multiple_scopes {
                    let name = match scope {
                        Coverage::LinkStages => "Stage observations",
                        _ => "Independent timings",
                    };
                    row.push(name.into());
                }
                if c.coverage["status"] == "not_planned" {
                    row.extend([
                        "Plan unavailable".into(),
                        "—".into(),
                        "—".into(),
                        "—".into(),
                    ]);
                } else {
                    let counts = scope.counts(c);
                    row.extend([
                        counts.applicable().to_string(),
                        counts.completed.to_string(),
                        counts.pending.to_string(),
                        counts.excluded.to_string(),
                    ]);
                }
                row
            })
        })
        .collect();
    table(out, &headers, rows);
}

fn measurement_details(out: &mut String, components: &[Component], scope: Coverage) {
    let (partial, rejected) = match scope {
        Coverage::LinkStages => (
            "Incomplete observation sets",
            "Complete observation sets not accepted",
        ),
        _ => (
            "Incomplete sample sets",
            "Complete sample sets not accepted",
        ),
    };
    table(
        out,
        &["C", partial, rejected],
        components
            .iter()
            .map(|c| (c, scope.counts(c)))
            .filter(|(_, counts)| counts.partial + counts.rejected > 0)
            .map(|(c, counts)| {
                vec![
                    format!("C{}", c.component_id),
                    counts.partial.to_string(),
                    counts.rejected.to_string(),
                ]
            })
            .collect(),
    );
    exclusion_reasons(out, components, scope);
}

fn exclusion_reasons(out: &mut String, components: &[Component], scope: Coverage) {
    let mut grouped = BTreeMap::<(String, usize), Vec<String>>::new();
    for c in components {
        let mut reasons = BTreeMap::<String, usize>::new();
        for (row, check) in scope.rows(c) {
            let entry = &row[scope.field()];
            if entry["status"] != "skipped" {
                continue;
            }
            let code = entry["reason"]["code"].as_str().unwrap_or("");
            let raw = entry["reason"]["reason"]
                .as_str()
                .unwrap_or("Reason unavailable");
            let reason = if code == "scheme_not_selected" {
                check.spec.scheme.as_ref().map_or_else(
                    || raw.into(),
                    |scheme| {
                        format!(
                            "{} is selected. {} cases are excluded.",
                            c.request.scheme.name.to_uppercase(),
                            scheme.to_uppercase()
                        )
                    },
                )
            } else if code == "capability_unavailable" && raw.contains("does not expose alias") {
                "The selected interface has no alias-operation entry point.".into()
            } else {
                raw.to_owned()
            };
            *reasons.entry(reason).or_default() += 1;
        }
        for (reason, count) in reasons {
            grouped
                .entry((reason, count))
                .or_default()
                .push(format!("C{}", c.component_id));
        }
    }
    if !grouped.is_empty() {
        let heading = match scope {
            Coverage::LinkStages => "Stage observation exclusions",
            Coverage::Timing(Some("chain")) => "Independent timing exclusions",
            _ => "Exclusion reasons",
        };
        let _ = writeln!(out, "#### {heading}\n");
        table(
            out,
            &["Applies to", "Reason", "Excluded cases per C"],
            grouped
                .into_iter()
                .map(|((reason, count), ids)| vec![ids.join(", "), reason, count.to_string()])
                .collect(),
        );
    }
}

fn family_coverage(out: &mut String, components: &[Component], selected: &str) {
    if selected == "L" {
        out.push_str(
            "L01 and L04–L10 count observed stage cases. Their workflow timings appear in W.\n\n",
        );
    }
    for (prefix, labels) in FAMILIES
        .into_iter()
        .filter(|(prefix, _)| *prefix == selected)
    {
        let mut rows: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let n = index + 1;
                let family = format!("{prefix}{n:02}");
                (vec![format!("{family} · {label}")], (family, n))
            })
            .collect();
        let heading = if prefix == "O" {
            rows.push((
                vec!["V06 · Time boundary calls such as self-XOR and overlapping copies".into()],
                ("V06".into(), 6),
            ));
            "Timed workloads by family"
        } else {
            "Family"
        };
        matrix(out, components, &[heading], &rows, |c, (family, n)| {
            let (scope, result) = match prefix {
                "V" => (Coverage::Validation, "checked"),
                "L" if matches!(n, 1 | 4..=10) => (Coverage::LinkStages, "diagnosed"),
                "O" => (Coverage::Timing(Some("operation")), "accepted"),
                _ => (Coverage::Timing(None), "accepted"),
            };
            let relevant = scope.rows(c).filter_map(|(row, check)| {
                (check.spec.family.split(',').any(|f| f == family)
                    && row[scope.field()]["reason"]["code"] != "scheme_not_selected")
                    .then_some(row)
            });
            let counts = scope.count_rows(c, relevant);
            if counts.applicable() == 0 {
                return if counts.excluded > 0 {
                    format!("Skipped ({})", counts.excluded)
                } else {
                    "Unavailable".into()
                };
            }
            let mut text = format!("{}/{}, {result}", counts.completed, counts.applicable());
            if counts.fatals > 0 {
                let _ = write!(text, " · {} fatal", counts.fatals);
            }
            if counts.warnings > 0 {
                let _ = write!(text, " · {} warnings", counts.warnings);
            }
            if counts.excluded > 0 {
                let _ = write!(text, " · {} skipped", counts.excluded);
            }
            text
        });
    }
}
fn structured_finding(finding: &Value) -> Option<crate::findings::Violation> {
    serde_json::from_value(finding["finding"].clone()).ok()
}
fn fatal(out: &mut String, c: &Component, finding: &Value) {
    let case = finding["case"].as_str().unwrap_or("execution integrity");
    let violation = structured_finding(finding);
    let check = c.plan.as_ref().and_then(|p| p.checks.get(case));
    if let Some(v) = &violation {
        let _ = writeln!(out, "{}\n", cell(&v.requirement));
        let kind = serde_json::to_value(v.failure_kind).unwrap();
        let _ = writeln!(
            out,
            "Contract ID · {} · {}\n",
            cell(&v.contract_id),
            cell(&string(&kind))
        );
        if let Some(difference) = &v.difference {
            let value = |v: &Value| {
                if v.is_null() {
                    "missing".into()
                } else {
                    string(v)
                }
            };
            let mut rows = vec![vec![
                difference.location.clone(),
                value(&difference.left),
                value(&difference.right),
            ]];
            if difference.left_length != difference.right_length {
                rows.push(vec![
                    "Output length (bytes)".into(),
                    difference
                        .left_length
                        .map_or("missing".into(), |n| n.to_string()),
                    difference
                        .right_length
                        .map_or("missing".into(), |n| n.to_string()),
                ]);
            }
            table(
                out,
                &[
                    "First difference",
                    &difference.left_label,
                    &difference.right_label,
                ],
                rows,
            );
        }
        if matches!(
            v.failure_kind,
            crate::findings::FailureKind::UnexpectedPanic
                | crate::findings::FailureKind::ExecutionFailure
        ) {
            let _ = writeln!(
                out,
                "{}\n",
                cell(&brief(
                    finding["message"]
                        .as_str()
                        .unwrap_or("No diagnostic message"),
                    350
                ))
            );
        }
    } else {
        out.push_str("This result has no structured contract/failure identity. It is shown separately without inferring a problem from its message.\n\n");
        let _ = writeln!(
            out,
            "{}\n",
            cell(&brief(
                finding["message"]
                    .as_str()
                    .unwrap_or("No diagnostic message"),
                350
            ))
        );
    }
    let context = violation.as_ref().map(|v| &v.context);
    let mut rows = Vec::new();
    if let Some(check) = check {
        rows.push(vec![
            "Test".into(),
            format!("{} · {}", check.spec.family, title(&check.spec.workload)),
        ]);
        let spec = &check.spec;
        let mut parameters = if let Some(scheme) = &spec.scheme {
            format!(
                "{} · K={} · B={} (bytes/symbol)",
                scheme.to_uppercase(),
                spec.k,
                spec.width
            )
        } else {
            format!("B={} (bytes/symbol)", spec.width)
        };
        if let Some(dispatch) = context.and_then(|c| c["dispatch"].as_str()) {
            let _ = write!(parameters, " · {dispatch}");
        }
        rows.push(vec!["Parameters".into(), parameters]);
        let mut seed = spec.seed.to_string();
        if let Some(input_seed) = context.and_then(|c| c["input_seed"].as_u64()) {
            let _ = write!(seed, " · step input seed {input_seed}");
        }
        rows.push(vec!["Reproduce with seed".into(), seed]);
    }
    if let Some(context) = context {
        if !context["operation"].is_null() {
            let operation =
                serde_json::from_value::<crate::content::Action>(context["operation"].clone())
                    .map_or_else(|_| string(&context["operation"]), |op| format!("{op:?}"));
            rows.push(vec!["Operation".into(), operation]);
        }
        if let Some(step) = context["step"].as_u64() {
            let mut position = format!("Step {step}");
            if let Some(total) = context["total_steps"].as_u64() {
                let _ = write!(position, "/{total}");
            }
            rows.push(vec!["Sequence position".into(), position]);
        }
        if let Some(scenario) = context["scenario"].as_str() {
            rows.push(vec!["Scenario".into(), title(scenario)]);
        }
        if let Some(count) = context["input_count"].as_u64() {
            let mut point = format!("After {count} received symbols");
            if let Some(id) = context["input_symbol"].as_u64() {
                let _ = write!(point, " · last input symbol ID {id}");
            }
            rows.push(vec!["Input at failure".into(), point]);
        }
    }
    table(out, &["Context", "Value"], rows);
    let _ = writeln!(
        out,
        "Case `{}`. Full diagnostic and replay context are in the index (`feval report REPORT_DIR --json`).\n",
        cell(case)
    );
}
type Finding<'a> = (&'a Component, &'a Value);
fn problem_key(c: &Component, finding: &Value) -> String {
    structured_finding(finding).map_or_else(
        || {
            format!(
                "unclassified:C{}:{}:{}",
                c.component_id, finding["case"], finding["phase"]
            )
        },
        |v| format!("{}:{:?}", v.contract_id, v.failure_kind),
    )
}
fn affected_cases(out: &mut String, findings: &[Finding<'_>]) {
    let mut rows = BTreeMap::<u32, (usize, BTreeSet<&str>)>::new();
    for (c, finding) in findings {
        let row = rows.entry(c.component_id).or_default();
        row.0 += 1;
        if let Some(check) = c
            .plan
            .as_ref()
            .and_then(|p| p.checks.get(finding["case"].as_str().unwrap_or("")))
        {
            row.1.extend(check.spec.family.split(','));
        }
    }
    table(
        out,
        &["C", "Test families", "Affected cases"],
        rows.into_iter()
            .map(|(id, (count, families))| {
                vec![
                    format!("C{id}"),
                    if families.is_empty() {
                        "Execution integrity".into()
                    } else {
                        families.into_iter().collect::<Vec<_>>().join(", ")
                    },
                    count.to_string(),
                ]
            })
            .collect(),
    );
}
fn fatal_findings(out: &mut String, components: &[Component]) {
    let mut problems = BTreeMap::<String, Vec<Finding<'_>>>::new();
    for c in components {
        for finding in &c.correctness.fatal_failures {
            problems
                .entry(problem_key(c, finding))
                .or_default()
                .push((c, finding));
        }
    }
    let mut problems: Vec<_> = problems.into_iter().collect();
    problems.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    for (index, (_, findings)) in problems.iter().enumerate() {
        let (c, finding) = findings[0];
        let label = structured_finding(finding)
            .map_or("Unclassified diagnostic", |v| v.failure_kind.label());
        let _ = writeln!(out, "#### Fatal {}\n\n{label}\n", index + 1);
        fatal(out, c, finding);
        affected_cases(out, findings);
        let _ = writeln!(
            out,
            "Representative diagnostic: C{}. All case-level evidence remains in the index.\n",
            c.component_id
        );
    }
}
fn findings(out: &mut String, components: &[Component]) {
    out.push_str("### Key results · Findings\n\n");
    if components
        .iter()
        .all(|c| c.correctness.fatal_failures.is_empty())
    {
        out.push_str("Fatal · None detected in completed checks.\n\n");
    }
    if components.iter().all(|c| c.correctness.warnings.is_empty()) {
        out.push_str("Warning · None detected in completed checks.\n\n");
    }
    fatal_findings(out, components);
    for c in components
        .iter()
        .filter(|c| c.stage == "validate" && !c.correctness.fatal_failures.is_empty())
    {
        let _ = writeln!(
            out,
            "C{} · Fatal validation findings blocked O/L/W measurements.\n",
            c.component_id
        );
    }
    for c in components {
        if c.correctness.fatal_failures.is_empty() && c.status != "succeeded" {
            table(
                out,
                &["C", "Stopped stage", "Execution issue"],
                vec![vec![
                    format!("C{}", c.component_id),
                    title(&c.stage),
                    c.error
                        .clone()
                        .unwrap_or_else(|| "No completed correctness execution".into()),
                ]],
            );
            let diagnostics: Vec<_> = c
                .failure_details
                .iter()
                .flat_map(|text| text.lines())
                .filter(|line| {
                    line.trim_start().starts_with("error") || line.contains("panicked at")
                })
                .take(3)
                .map(|line| vec![format!("C{}", c.component_id), brief(line, 300)])
                .collect();
            table(out, &["C", "Compiler/runtime diagnostic"], diagnostics);
        }
        if let Some(error) = &c.aggregation_error {
            table(
                out,
                &["C", "Sample aggregation issue"],
                vec![vec![format!("C{}", c.component_id), error.clone()]],
            );
        }
    }
    warning_findings(out, components);
}

fn warning_findings(out: &mut String, components: &[Component]) {
    let mut problems =
        BTreeMap::<String, (Option<crate::findings::Violation>, Vec<Finding<'_>>)>::new();
    for c in components {
        for warning in &c.correctness.warnings {
            let mut identities = BTreeMap::new();
            let values = warning["findings"].as_array().cloned().unwrap_or_default();
            for value in values
                .into_iter()
                .chain(std::iter::once(warning["finding"].clone()))
            {
                if let Ok(v) = serde_json::from_value::<crate::findings::Violation>(value) {
                    identities.insert(format!("{}:{:?}", v.contract_id, v.failure_kind), Some(v));
                }
            }
            if identities.is_empty() {
                identities.insert(problem_key(c, warning), None);
            }
            for (key, v) in identities {
                problems
                    .entry(key)
                    .or_insert_with(|| (v, Vec::new()))
                    .1
                    .push((c, warning));
            }
        }
    }
    for (index, (key, (violation, findings))) in problems.iter().enumerate() {
        let label = violation
            .as_ref()
            .map_or("Unclassified diagnostic", |v| v.failure_kind.label());
        let (component, example) = findings[0];
        let spec = component
            .plan
            .as_ref()
            .and_then(|p| p.checks.get(example["case"].as_str()?))
            .map(|check| &check.spec);
        let _ = writeln!(out, "#### Warning {}\n", index + 1);
        if let Some(spec) = spec {
            let _ = writeln!(out, "{label} · {}\n", cell(&title(&spec.workload)));
        } else {
            let _ = writeln!(out, "{label}\n");
        }
        if let Some(v) = violation {
            let _ = writeln!(out, "- Reference expectation · {}", cell(&v.requirement));
            if let Some(boundary) = spec.and_then(|spec| spec.boundary.as_ref()) {
                let _ = writeln!(out, "- Warning basis · {}", cell(&boundary.basis));
            }
            let kind = serde_json::to_value(v.failure_kind).unwrap();
            let _ = writeln!(
                out,
                "- Contract ID · {} · {}\n",
                cell(&v.contract_id),
                cell(&string(&kind))
            );
        } else {
            out.push_str("The saved case has no structured contract identity.\n\n");
        }
        let mut groups = BTreeMap::<u32, (&Component, Vec<&Value>)>::new();
        for (c, warning) in findings {
            groups
                .entry(c.component_id)
                .or_insert_with(|| (c, Vec::new()))
                .1
                .push(warning);
        }
        for (c, ws) in groups.into_values() {
            let mut checks = BTreeMap::new();
            let mut calls = BTreeMap::new();
            for w in &ws {
                let expected = &w["expected"];
                if violation.is_none() || problem_key(c, w) == *key {
                    *checks
                        .entry(difference(expected, &w["actual"]))
                        .or_default() += 1;
                }
                if let Some(variants) = w["measurement_summary"]["variants"].as_array() {
                    for variant in variants {
                        count_warning_calls(
                            &mut calls,
                            expected,
                            &variant["actual"],
                            variant["samples"].as_u64().unwrap_or_default(),
                            key,
                            violation.is_none(),
                        );
                    }
                } else if let Some(observations) = w["measured_observations"].as_array() {
                    for observation in observations {
                        count_warning_calls(
                            &mut calls,
                            expected,
                            &observation["actual"],
                            1,
                            key,
                            violation.is_none(),
                        );
                    }
                }
            }
            let timed = if calls.is_empty() {
                "Not measured".into()
            } else {
                let text = list_variants(calls, "call");
                if c.status == "succeeded" {
                    text
                } else {
                    format!("{text} (unaccepted run)")
                }
            };
            let validation = if checks.is_empty() {
                "No validation deviation of this kind".into()
            } else {
                list_variants(checks, "case")
            };
            let cases = if ws.len() == 1 { "case" } else { "cases" };
            let _ = writeln!(out, "C{} · {} affected {cases}\n", c.component_id, ws.len());
            let _ = writeln!(out, "- Validation observations · {}", cell(&validation));
            let _ = writeln!(out, "- Timed observations · {}", cell(&timed));
            let _ = writeln!(
                out,
                "- Direct timing example · {}\n",
                cell(&warning_timings(c, &ws))
            );
        }
    }
}

fn warning_timings(c: &Component, warnings: &[&Value]) -> String {
    if c.status != "succeeded" {
        return "Not measured".into();
    }
    let mut measured = false;
    warnings
        .iter()
        .find_map(|w| {
            let id = w["case"].as_str()?;
            let sample = c.summary.iter().find(|s| s.case == id)?;
            let ns = sample.ns_per_operation?;
            measured = true;
            let mut fields = id.split('/').skip(2);
            let width = fields.next()?.strip_prefix('b')?.parse::<usize>().ok()?;
            if fields.next() != Some("direct") {
                return None;
            }
            let size = fields.next()?;
            Some(format!(
                "B={width} direct {size} · {}{} ns/call",
                decimal(ns),
                mark(sample)
            ))
        })
        .unwrap_or_else(|| {
            if measured {
                "See index for timings".into()
            } else {
                "Not measured".into()
            }
        })
}

fn count_warning_calls(
    counts: &mut BTreeMap<String, u64>,
    expected: &Value,
    observation: &Value,
    samples: u64,
    key: &str,
    unclassified: bool,
) {
    for outcome in observation["outcomes"].as_array().into_iter().flatten() {
        let identity =
            serde_json::from_value::<crate::findings::Violation>(outcome["finding"].clone())
                .ok()
                .map(|v| format!("{}:{:?}", v.contract_id, v.failure_kind));
        if !unclassified && outcome["actual"] != *expected && identity.as_deref() != Some(key) {
            continue;
        }
        if let Some(count) = outcome["count"].as_u64() {
            let text = difference(expected, &outcome["actual"]);
            *counts.entry(text).or_default() += count * samples;
        }
    }
}

fn list_variants(variants: BTreeMap<String, u64>, unit: &str) -> String {
    let mut variants: Vec<_> = variants.into_iter().collect();
    variants.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let quantity = |n| {
        let plural = if n == 1 { "" } else { "s" };
        format!("{n} {unit}{plural}")
    };
    let mut text = variants
        .iter()
        .take(2)
        .map(|(label, count)| format!("{} · {}", quantity(*count), brief(label, 135)))
        .collect::<Vec<_>>();
    if variants.len() > 2 {
        let remaining: u64 = variants[2..].iter().map(|(_, count)| count).sum();
        text.push(format!(
            "{} with other observations (see index)",
            quantity(remaining)
        ));
    }
    text.join(" / ")
}

fn difference(expected: &Value, actual: &Value) -> String {
    if expected == actual {
        return "Matched the reference".into();
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
    format!("Expected {}, observed {}", string(expected), string(actual))
}

pub fn comparison(value: &Value) -> String {
    let mut out = format!(
        "# Fountain comparison\n\nBaseline: {} · Candidate: {} · {}\n\nScope: {}.\n\n",
        cell(&string(&value["left"])),
        cell(&string(&value["right"])),
        cell(&string(&value["status"])),
        cell(&string(&value["attribution"]))
    );
    let all_rows = value["rows"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let catalog = crate::content::cases();
    let cases: BTreeMap<_, _> = catalog.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut boundary_examples = std::collections::BTreeSet::new();
    let shown: Vec<_> = all_rows
        .iter()
        .filter_map(|row| {
            let case = cases.get(row["case"].as_str()?)?;
            if case.category == "operation" && !case.direct {
                return None;
            }
            let boundary = case.category == "operation"
                && ["left_warnings", "right_warnings"].iter().any(|key| {
                    row[*key]
                        .as_array()
                        .is_some_and(|warnings| !warnings.is_empty())
                })
                && boundary_examples.insert(case.workload.as_str());
            (important(case) || boundary).then_some((row, *case))
        })
        .collect();
    let mut change_counts = [0; 4];
    for (row, _) in &shown {
        match row["left_over_right_time"].as_f64() {
            Some(ratio) if ratio > 1.0 => change_counts[0] += 1,
            Some(ratio) if ratio < 1.0 => change_counts[1] += 1,
            Some(_) => change_counts[2] += 1,
            None => change_counts[3] += 1,
        }
    }
    let [lower, higher, equal, incomplete] = change_counts;
    let _ = writeln!(
        out,
        "## Key findings\n\n{} common cases. The headline tables show {} cases with {lower} lower candidate medians, {higher} higher, {equal} equal, and {incomplete} with incomplete recovery. Workflow rates are source-equivalent Gbit/s. Time change uses the baseline median as the denominator. CV shows timing dispersion.\n",
        all_rows.len(),
        shown.len()
    );

    out.push_str("## Validation and warnings\n\n");
    for (side, label) in [("left", "Baseline"), ("right", "Candidate")] {
        let warnings = value["correctness"][side]["warnings"]
            .as_array()
            .map_or(0, Vec::len);
        let skips = string(&value["coverage"][side]["skipped_validation"]);
        let _ = writeln!(
            out,
            "- {label} · {warnings} semantic warning cases · {skips} validation skips"
        );
    }
    out.push('\n');
    for (category, label) in CATEGORIES.iter().skip(1) {
        let _ = writeln!(out, "## {label}\n");
        let selected: Vec<_> = shown
            .iter()
            .filter(|(_, case)| case.category == *category)
            .collect();
        if selected.is_empty() {
            out.push_str("No common headline measurements.\n\n");
            continue;
        }
        table(
            &mut out,
            &[
                "Case",
                "Baseline",
                "Candidate",
                "Time change",
                "CV % (base / candidate)",
            ],
            selected
                .into_iter()
                .map(|(row, case)| comparison_row(row, case))
                .collect(),
        );
    }
    out.push_str("Use `feval compare LEFT RIGHT --json` for every common case, work counts and warning details.\n");
    out
}

fn comparison_row(row: &Value, case: &crate::content::Case) -> Vec<String> {
    let change = row["left_over_right_time"].as_f64().map_or_else(
        || "Incomplete recovery".into(),
        |ratio| format!("{:+.2}%", (1.0 / ratio - 1.0) * 100.0),
    );
    let mut label = comparison_label(case);
    let differences: Vec<_> = [
        ("same_offered_input", "offered input"),
        ("same_recorded_sequence", "recorded sequence"),
        ("same_consumed_count", "consumed count"),
    ]
    .into_iter()
    .filter_map(|(key, what)| (row[key] == false).then_some(what))
    .collect();
    if !differences.is_empty() {
        let _ = write!(label, " · changed {}", differences.join(", "));
    }
    let warnings = ["left", "right"].map(|side| {
        row[format!("{side}_warnings")]
            .as_array()
            .map_or(0, Vec::len)
    });
    if warnings != [0, 0] {
        let _ = write!(label, " · warnings {}/{}", warnings[0], warnings[1]);
    }
    let value = |side: &str| {
        let incomplete = row[format!("{side}_incomplete")].as_u64().unwrap_or(0);
        if incomplete > 0 {
            let completed = row[format!("{side}_completed")].as_u64().unwrap_or(0);
            return format!("{completed}/{} recovered", completed + incomplete);
        }
        let (amount, unit) = match case.category.as_str() {
            "operation" => (
                row[format!("{side}_operations_per_second")]
                    .as_f64()
                    .filter(|n| *n > 0.0)
                    .map(|rate| 1e9 / rate),
                "ns/call",
            ),
            "workflow"
                if case.workload == "encode-repairs"
                    || (case.workload == "decode" && case.missing > 0) =>
            {
                (
                    row[format!("{side}_source_payload_gbps")].as_f64(),
                    "Gbit/s",
                )
            }
            _ => (
                row[format!("{side}_median_ns")].as_f64().map(|ns| ns / 1e6),
                "ms",
            ),
        };
        amount.map_or_else(
            || "Unavailable".into(),
            |n| format!("{} {unit}", decimal(n)),
        )
    };
    let cv = |side: &str| {
        row[format!("{side}_cv")]
            .as_f64()
            .map_or_else(|| "Unavailable".into(), |n| format!("{:.1}", n * 100.0))
    };
    vec![
        label,
        value("left"),
        value("right"),
        change,
        format!("{} / {}", cv("left"), cv("right")),
    ]
}

fn comparison_label(case: &crate::content::Case) -> String {
    let name = if case.workload == "decode" {
        if case.missing == 0 {
            "Decode all sources".into()
        } else {
            format!("Recover {} missing sources", case.missing)
        }
    } else {
        title(&case.workload)
    };
    let family = case.family.split(',').next().unwrap_or(&case.family);
    let mut label = format!("{family} · {name}");
    if case.category == "operation" {
        let _ = write!(
            label,
            " · B={} {}",
            case.width,
            if case.direct { "direct" } else { "execute" }
        );
    } else {
        let _ = write!(label, " · K={}", case.k);
        if case.width != 0 {
            let _ = write!(label, " B={}", case.width);
        }
        if matches!(
            case.workload.as_str(),
            "encode-repairs" | "reuse-encode" | "first-encode"
        ) {
            let _ = write!(label, " R={}", case.repairs);
        }
        if case.blocks > 1 {
            let _ = write!(label, " · {} blocks", case.blocks);
        }
    }
    label
}
