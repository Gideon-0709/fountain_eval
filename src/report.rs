//! Markdown reports: one self-contained reading surface, backed by sealed evidence.
use crate::{artifact::Component, evidence::Record, measurement::Summary};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

const PAGE: usize = 12;
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
            "Insert",
            "Read",
            "Zero",
            "XOR",
            "Multi-source XOR",
            "GF arithmetic",
            "Copy and move",
            "Alias",
            "Remove",
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
    ("O01", "insert-new"),
    ("O02", "read"),
    ("O03", "zero-one"),
    ("O04", "xor-one"),
    ("O05", "xor-many-4"),
    ("O06", "scale-7"),
    ("O06", "mul-add-7"),
    ("O07", "copy-one"),
    ("O07", "move-one"),
    ("O08", "alias"),
    ("O09", "remove"),
];
const HEADLINE: [(usize, usize); 4] = [(512, 128), (512, 1016), (5008, 128), (5008, 1016)];
const STAGED: (usize, usize) = HEADLINE[0];

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
            OPS.iter().any(|(_, operation)| *operation == c.workload)
                && [128, 1016].contains(&c.width)
                && c.instances == 64
        }
        "chain" => {
            matches!(c.workload.as_str(), "plan-build" | "plan-export")
                || (c.workload.ends_with("-replay") && [512, 5008].contains(&c.k) && c.width == 128)
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
) {
    out.push_str("## Summary\n\n### Test configuration\n\n");
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
    out.push_str("### Test results\n\n");
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
    let mut out = "# Fountain evaluation report\n\n".to_owned();
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
    );
    table(
        &mut out,
        &["C", "Result", "Fatal", "Warning", "Performance"],
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
            let mut out = "# Fountain evaluation report\n\n".to_owned();
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
            );
            table(
                &mut out,
                &["C", "Result", "Fatal", "Warning", "Performance"],
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
    validation_results(out, components);
    operation_results(out, components);
    link_results(out, components);
    workflow_results(out, components);
}

fn validation_results(out: &mut String, components: &[Component]) {
    out.push_str("## Validation tests (V)\n\n### Summary\n\n");
    table(
        out,
        &["C", "Correctness", "Fatal cases", "Warning cases"],
        components
            .iter()
            .map(|c| {
                vec![
                    format!("C{}", c.component_id),
                    title(&c.correctness.status),
                    c.correctness.fatal_failures.len().to_string(),
                    c.correctness.warnings.len().to_string(),
                ]
            })
            .collect(),
    );
    out.push_str("Problems are grouped by observed behavior, with affected components, test families and case counts. Each fatal problem has one representative diagnostic.\n\n");
    findings(out, components);
    out.push_str("### Detailed results\n\n");
    coverage(out, components);
    family_coverage(out, components, "V");
}

fn accepts(c: &Component, prefix: &str) -> bool {
    c.status == "succeeded" && c.summary.iter().any(|s| s.case.starts_with(prefix))
}

fn performance_summary(c: &Component) -> String {
    if c.status != "succeeded" {
        return format!("Not measured · {} unaccepted samples", c.sample_count);
    }
    let count = Counts::collect(c, case_rows(c)).accepted;
    let incomplete = c.summary.iter().filter(|s| s.incomplete > 0).count();
    if incomplete == 0 {
        format!("{count} accepted cases · {} samples", c.sample_count)
    } else {
        format!("{count} accepted cases · {incomplete} with incomplete recovery")
    }
}

fn operation_results(out: &mut String, components: &[Component]) {
    out.push_str("## Operation tests (O)\n\n");
    family_coverage(out, components, "O");
    if !components.iter().any(|c| accepts(c, "operation/")) {
        out.push_str("Not measured. See coverage for applicability and execution status.\n\n");
        return;
    }
    out.push_str("Median ns/call across 64 independent instances, shown as direct / execute. Multi-source XOR uses four sources. GF arithmetic uses scalar 7.\n\n");
    let rows: Vec<_> = OPS
        .iter()
        .map(|(family, name)| (vec![format!("{family} {}", title(name))], *name))
        .collect();
    for width in [128, 1016] {
        let _ = writeln!(out, "### {width} bytes per symbol\n");
        matrix(out, components, &["Operation"], &rows, |c, name| {
            let at = |dispatch| {
                metric(
                    c,
                    &format!("operation/{name}/b{width}/{dispatch}/small"),
                    Metric::NanosecondsPerCall,
                )
            };
            let direct = at("direct");
            let execute = at("execute");
            if direct == execute && matches!(direct.as_str(), "Skipped" | "Not measured") {
                direct
            } else {
                format!("{direct} / {execute}")
            }
        });
    }
}

fn link_results(out: &mut String, components: &[Component]) {
    out.push_str("## Link tests (L)\n\n");
    family_coverage(out, components, "L");
    out.push_str("### Encoding plans (L02–L03)\n\n");
    if components.iter().any(|c| accepts(c, "chain/plan-")) {
        let rows: Vec<_> = [64, 512, 5008]
            .into_iter()
            .flat_map(|k| {
                [
                    ("plan-build", "Build plan"),
                    ("plan-export", "Clone operations"),
                ]
                .map(|(name, label)| (vec![k.to_string(), label.into()], (k, name)))
            })
            .collect();
        matrix(
            out,
            components,
            &["K", "Independent interval · median ms"],
            &rows,
            |c, (k, name)| {
                metric(
                    c,
                    &format!("chain/{name}/{}/k{k}", c.request.scheme.name),
                    Metric::Milliseconds,
                )
            },
        );
    } else {
        out.push_str("Not measured. See coverage for standalone plan support.\n\n");
    }

    out.push_str("### Recorded operator replay (L11–L12)\n\n");
    if components
        .iter()
        .any(|c| accepts(c, "chain/encode-replay/") || accepts(c, "chain/decode-replay/"))
    {
        out.push_str("Median ms for replaying a captured operation sequence at 128 bytes per symbol. The full index retains operation counts and sequence identity.\n\n");
        let rows: Vec<_> = [512, 5008]
            .into_iter()
            .flat_map(|k| {
                [("encode-replay", "Encode"), ("decode-replay", "Decode")]
                    .map(|(name, label)| (vec![k.to_string(), label.into()], (k, name)))
            })
            .collect();
        matrix(out, components, &["K", "Replay"], &rows, |c, (k, name)| {
            metric(
                c,
                &format!("chain/{name}/{}/k{k}/b128", c.request.scheme.name),
                Metric::Milliseconds,
            )
        });
    } else {
        out.push_str("Not measured. See coverage for replay support.\n\n");
    }

    let (k, width) = STAGED;
    let missing = k.div_ceil(10);
    let _ = writeln!(
        out,
        "### Observed encode and decode stages (L01, L04–L10)\n\nK={k}, B={width}. Encoding generates 8 repairs. Decoding starts with {missing} missing sources (source-first, seed {}). Table times are means in µs from instrumented lifecycles. Each combined interval is counted once. Displayed sums may differ by rounding.\n",
        crate::content::SEEDS[0]
    );
    for encode in [true, false] {
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
                let direction = if encode { "Encode" } else { "Recovery decode" };
                let _ = writeln!(out, "**C{} · {direction}**\n", c.component_id);
                table(out, &["Interval", "Mean µs", "Share"], rows);
            }
        }
        if !any {
            let direction = if encode { "Encode" } else { "Recovery decode" };
            let _ = writeln!(out, "{direction} stages: Not measured.\n");
        }
    }
}

fn stage_rows(c: &Component, id: &str) -> Option<Vec<Vec<String>>> {
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
            Some((links, row(label, mean)))
        })
        .collect();
    stages.sort_by(|(a, _), (b, _)| (a.is_empty(), a).cmp(&(b.is_empty(), b)));
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

fn workflow_results(out: &mut String, components: &[Component]) {
    out.push_str("## Workflow tests (W)\n\n");
    family_coverage(out, components, "W");
    if !components.iter().any(|c| accepts(c, "workflow/")) {
        out.push_str("Not measured. See coverage for applicability and execution status.\n\n");
        return;
    }
    out.push_str("K is source symbols and B is bytes per symbol. Rates use the median output-ready time and decimal Gbit/s.\n\n");
    out.push_str("### Codec throughput (W02, W05)\n\nThe encoding columns show source-equivalent / repair-output Gbit/s for 8 repairs. The recovery columns show source-equivalent Gbit/s.\n\n");
    let geometry = |k: usize, width: usize| vec![k.to_string(), width.to_string()];
    let encode: Vec<_> = HEADLINE
        .into_iter()
        .map(|(k, width)| (geometry(k, width), (k, width)))
        .collect();
    matrix(out, components, &["K", "B"], &encode, |c, &(k, width)| {
        let id = format!(
            "workflow/encode-repairs/{}/k{k}/b{width}/r8",
            c.request.scheme.name
        );
        let source = metric(c, &id, Metric::SourceRate);
        let repair = metric(c, &id, Metric::RepairRate);
        if source == repair && matches!(source.as_str(), "Skipped" | "Not measured") {
            source
        } else {
            format!("{source} / {repair}")
        }
    });
    let _ = writeln!(
        out,
        "**Recovery decode · source-first, seed {}**\n",
        crate::content::SEEDS[0]
    );
    let decode: Vec<_> = HEADLINE
        .into_iter()
        .map(|(k, width)| {
            let missing = k.div_ceil(10);
            (
                vec![k.to_string(), width.to_string(), missing.to_string()],
                (k, width, missing),
            )
        })
        .collect();
    matrix(
        out,
        components,
        &["K", "B", "Missing"],
        &decode,
        |c, &(k, width, missing)| {
            metric(
                c,
                &recovery_id(&c.request.scheme.name, k, width, missing),
                Metric::SourceRate,
            )
        },
    );

    out.push_str("### Round trip (W06)\n\nMedian ms for encoding, in-memory delivery in mixed order and decoding.\n\n");
    matrix(
        out,
        components,
        &["K", "B", "Missing"],
        &decode,
        |c, &(k, width, _)| {
            metric(
                c,
                &format!(
                    "workflow/round-trip/{}/k{k}/b{width}",
                    c.request.scheme.name
                ),
                Metric::Milliseconds,
            )
        },
    );
    let (k, width) = STAGED;
    let _ = writeln!(
        out,
        "### First use, reuse, all-source decode and lifecycle\n\nK={k}, B={width}. The directly timed elapsed medians and the derived lifecycle median are in ms.\n"
    );
    let rows = [
        (
            "W01 First encode, 8 repairs",
            "first-encode",
            Metric::Milliseconds,
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

#[derive(Default)]
struct Counts {
    validations: usize,
    validation_fatals: usize,
    validation_warnings: usize,
    validation_skips: usize,
    validation_pending: usize,
    accepted: usize,
    partial: usize,
    rejected: usize,
    measurement_pending: usize,
    measurement_skips: usize,
    diagnosed: usize,
    diagnosis_pending: usize,
}
impl Counts {
    fn collect<'a>(component: &Component, rows: impl IntoIterator<Item = &'a Value>) -> Self {
        let mut c = Self::default();
        for r in rows {
            match r["validation"]["status"].as_str() {
                Some("skipped") => c.validation_skips += 1,
                Some("not_executed") => c.validation_pending += 1,
                Some("passed" | "failed") => {
                    c.validations += 1;
                    if r["validation"]["status"] == "failed" {
                        if r["validation"]["severity"] == "fatal" {
                            c.validation_fatals += 1;
                        } else {
                            c.validation_warnings += 1;
                        }
                    }
                }
                _ => {}
            }
            let measured = component
                .plan
                .as_ref()
                .and_then(|p| p.checks.get(r["case"].as_str().unwrap_or("")))
                .is_some_and(|c| c.measured);
            if measured {
                match r["measurement"]["status"].as_str() {
                    Some("recorded") if r["measurement"]["accepted"] == true => c.accepted += 1,
                    Some("recorded") => c.rejected += 1,
                    Some("incomplete") => c.partial += 1,
                    Some("not_executed") => c.measurement_pending += 1,
                    Some("skipped") => c.measurement_skips += 1,
                    _ => {}
                }
            }
            match r["diagnostics"]["status"].as_str() {
                Some("recorded") if component.status == "succeeded" => c.diagnosed += 1,
                Some("recorded" | "incomplete" | "not_executed") => c.diagnosis_pending += 1,
                _ => {}
            }
        }
        c
    }

    fn planned(&self) -> usize {
        self.accepted + self.partial + self.rejected + self.measurement_pending
    }
}

fn coverage(out: &mut String, components: &[Component]) {
    out.push_str("**V check coverage**\n\n");
    table(
        out,
        &[
            "C",
            "Applicable checks",
            "Checked",
            "Not checked",
            "Excluded",
        ],
        components
            .iter()
            .map(|c| {
                let mut row = vec![format!("C{}", c.component_id)];
                if c.coverage["status"] == "not_planned" {
                    row.extend([
                        "Plan unavailable".into(),
                        "—".into(),
                        "—".into(),
                        "—".into(),
                    ]);
                } else {
                    let counts = Counts::collect(
                        c,
                        case_rows(c).filter(|row| {
                            c.plan
                                .as_ref()
                                .and_then(|p| p.checks.get(row["case"].as_str().unwrap_or("")))
                                .is_some_and(|check| check.spec.validation_family().is_some())
                        }),
                    );
                    row.extend([
                        (counts.validations + counts.validation_pending).to_string(),
                        counts.validations.to_string(),
                        counts.validation_pending.to_string(),
                        counts.validation_skips.to_string(),
                    ]);
                }
                row
            })
            .collect(),
    );

    out.push_str("**Performance test coverage**\n\n");
    table(
        out,
        &[
            "C",
            "Applicable tests",
            "Accepted results",
            "Not started",
            "Excluded",
        ],
        components
            .iter()
            .map(|c| {
                let mut row = vec![format!("C{}", c.component_id)];
                if c.coverage["status"] == "not_planned" {
                    row.extend([
                        "Plan unavailable".into(),
                        "—".into(),
                        "—".into(),
                        "—".into(),
                    ]);
                } else {
                    let counts = Counts::collect(c, case_rows(c));
                    row.extend([
                        counts.planned().to_string(),
                        counts.accepted.to_string(),
                        counts.measurement_pending.to_string(),
                        counts.measurement_skips.to_string(),
                    ]);
                }
                row
            })
            .collect(),
    );

    let exceptions: Vec<_> = components
        .iter()
        .filter_map(|c| {
            let n = Counts::collect(c, case_rows(c));
            (n.partial + n.rejected > 0).then(|| {
                vec![
                    format!("C{}", c.component_id),
                    n.partial.to_string(),
                    n.rejected.to_string(),
                ]
            })
        })
        .collect();
    if !exceptions.is_empty() {
        table(
            out,
            &["C", "Incomplete sample sets", "Complete sets not accepted"],
            exceptions,
        );
        out.push_str("Incomplete sample sets ended before all samples were collected. Complete sets from an unsuccessful C are not accepted.\n\n");
    }
    out.push_str("V checks run in family order, V01–V11. A fatal does not stop other independent V cases; after V completes, any fatal blocks O/L/W. An aborted, crashed or timed-out driver can leave V cases not checked. Counts are test cases. Excluded cases do not apply to the selected scheme or API. Performance results require all samples and a successful C outcome.\n\n");

    let mut grouped = BTreeMap::<(String, usize, usize), Vec<String>>::new();
    for c in components {
        let mut reasons = BTreeMap::<String, (usize, usize)>::new();
        for row in case_rows(c) {
            let check = c
                .plan
                .as_ref()
                .and_then(|p| p.checks.get(row["case"].as_str().unwrap_or("")));
            for scope in ["validation", "measurement"] {
                let entry = &row[scope];
                if entry["status"] != "skipped"
                    || (scope == "measurement" && !check.is_some_and(|c| c.measured))
                    || (scope == "validation"
                        && check.is_none_or(|c| c.spec.validation_family().is_none()))
                {
                    continue;
                }
                let code = entry["reason"]["code"].as_str().unwrap_or("");
                let raw = entry["reason"]["reason"]
                    .as_str()
                    .unwrap_or("Reason unavailable");
                let reason = if code == "scheme_not_selected" {
                    check
                        .and_then(|check| check.spec.scheme.as_ref())
                        .map_or_else(
                            || raw.into(),
                            |scheme| {
                                format!(
                                    "{} is selected. {} cases are excluded.",
                                    c.request.scheme.name.to_uppercase(),
                                    scheme.to_uppercase()
                                )
                            },
                        )
                } else if code == "capability_unavailable" && raw.contains("does not expose alias")
                {
                    "The selected interface has no alias-operation entry point.".into()
                } else {
                    raw.to_owned()
                };
                let counts = reasons.entry(reason).or_default();
                if scope == "validation" {
                    counts.0 += 1;
                } else {
                    counts.1 += 1;
                }
            }
        }
        for (reason, (checks, timings)) in reasons {
            grouped
                .entry((reason, checks, timings))
                .or_default()
                .push(format!("C{}", c.component_id));
        }
    }
    if !grouped.is_empty() {
        out.push_str("**Exclusion reasons**\n\n");
        table(
            out,
            &[
                "Applies to",
                "Reason",
                "V checks per C",
                "Performance tests per C",
            ],
            grouped
                .into_iter()
                .map(|((reason, checks, timings), ids)| {
                    vec![
                        ids.join(", "),
                        reason,
                        checks.to_string(),
                        timings.to_string(),
                    ]
                })
                .collect(),
        );
    }
}

fn family_coverage(out: &mut String, components: &[Component], selected: &str) {
    // V06 includes timed boundary cases. The L stage families share W cases,
    // so their diagnostic observations belong to each associated family.
    out.push_str("Family counts apply to the selected scheme. Link stage families count diagnostic cases.\n\n");
    for (prefix, labels) in FAMILIES
        .into_iter()
        .filter(|(prefix, _)| *prefix == selected)
    {
        let rows: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let n = index + 1;
                let family = format!("{prefix}{n:02}");
                (vec![format!("{family} · {label}")], (family, n))
            })
            .collect();
        matrix(out, components, &["Family"], &rows, |c, (family, n)| {
            let relevant = case_rows(c).filter(|r| {
                r["family"]
                    .as_str()
                    .is_some_and(|f| f.split(',').any(|f| f == family))
                    && r["validation"]["reason"]["code"] != "scheme_not_selected"
            });
            let rates = Counts::collect(c, relevant);
            let (recorded, pending, skipped) = match prefix {
                "V" => (
                    rates.validations,
                    rates.validation_pending,
                    rates.validation_skips,
                ),
                "L" if matches!(n, 1 | 4..=10) => (
                    rates.diagnosed,
                    rates.diagnosis_pending,
                    rates.measurement_skips,
                ),
                _ => (
                    rates.accepted,
                    rates.measurement_pending + rates.partial + rates.rejected,
                    rates.measurement_skips,
                ),
            };
            if recorded + pending == 0 {
                return if skipped > 0 {
                    format!("Skipped ({skipped})")
                } else {
                    "Unavailable".into()
                };
            }
            let result = if prefix == "V" {
                "checked"
            } else if prefix == "L" && matches!(n, 1 | 4..=10) {
                "diagnosed"
            } else {
                "accepted"
            };
            let mut text = format!("{recorded}/{}, {result}", recorded + pending);
            if prefix == "V" {
                if rates.validation_fatals > 0 {
                    let _ = write!(text, " · {} fatal", rates.validation_fatals);
                }
                if rates.validation_warnings > 0 {
                    let _ = write!(text, " · {} warnings", rates.validation_warnings);
                }
            }
            if skipped > 0 {
                let _ = write!(text, " · {skipped} skipped");
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
        let _ = writeln!(
            out,
            "**Fatal** · **{}**\n\n{}\n",
            v.failure_kind.label(),
            cell(&v.requirement)
        );
        table(
            out,
            &["Contract ID", "Failure kind"],
            vec![vec![
                v.contract_id.clone(),
                string(&serde_json::to_value(v.failure_kind).unwrap()),
            ]],
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
        out.push_str("**Fatal** · **Unclassified diagnostic**\n\nThis result has no structured contract/failure identity. It is shown separately without inferring a problem from its message.\n\n");
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
        let _ = writeln!(out, "#### Fatal {}\n", index + 1);
        let (c, finding) = findings[0];
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
    out.push_str("### Fatal\n\n");
    if components
        .iter()
        .all(|c| c.correctness.fatal_failures.is_empty())
    {
        out.push_str("None detected. Review the executed coverage in V.\n\n");
    }
    fatal_findings(out, components);
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
    out.push_str("### Warning\n\n");
    if components.iter().all(|c| c.correctness.warnings.is_empty()) {
        out.push_str("None detected. Review the executed coverage in V.\n\n");
    }
    warning_findings(out, components);
    if components
        .iter()
        .any(|c| c.status == "succeeded" && !c.summary.is_empty())
    {
        out.push_str("† marks benchmark timing CV above 10%.\n\n");
    }
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
        let _ = writeln!(out, "#### Warning {} · {}\n", index + 1, label);
        if let Some(v) = violation {
            table(
                out,
                &["Contract", "Meaning"],
                vec![
                    vec!["Contract ID".into(), v.contract_id.clone()],
                    vec![
                        "Failure kind".into(),
                        string(&serde_json::to_value(v.failure_kind).unwrap()),
                    ],
                    vec!["Reference expectation".into(), v.requirement.clone()],
                ],
            );
            let (c, first) = findings[0];
            if let Some(contract) = c
                .plan
                .as_ref()
                .and_then(|p| p.checks.get(first["case"].as_str().unwrap_or("")))
                .and_then(|c| c.spec.boundary.as_ref())
            {
                table(out, &["Warning basis"], vec![vec![contract.basis.clone()]]);
            }
        } else {
            out.push_str("No structured identity is available; this case is not merged based on diagnostic text.\n\n");
        }
        let mut groups = BTreeMap::<u32, (&Component, Vec<&Value>)>::new();
        for (c, warning) in findings {
            groups
                .entry(c.component_id)
                .or_insert_with(|| (c, Vec::new()))
                .1
                .push(warning);
        }
        table(
            out,
            &[
                "C",
                "Affected cases",
                "Validation observations",
                "Timed observations",
                "Example timings",
            ],
            groups
                .into_values()
                .map(|(c, ws)| {
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
                    vec![
                        format!("C{}", c.component_id),
                        ws.len().to_string(),
                        if checks.is_empty() {
                            "No validation deviation of this kind".into()
                        } else {
                            list_variants(checks, "case")
                        },
                        timed,
                        warning_timings(c, &ws),
                    ]
                })
                .collect(),
        );
    }
}

fn warning_timings(c: &Component, warnings: &[&Value]) -> String {
    if c.status != "succeeded" {
        return "Not measured".into();
    }
    let timings: Vec<_> = ["direct", "execute"]
        .into_iter()
        .filter_map(|dispatch| {
            warnings.iter().find_map(|w| {
                let id = w["case"].as_str()?;
                let mut fields = id.split('/').skip(2);
                let width = fields.next()?.strip_prefix('b')?.parse::<usize>().ok()?;
                if fields.next() != Some(dispatch) {
                    return None;
                }
                let size = fields.next()?;
                let sample = c.summary.iter().find(|s| s.case == id)?;
                let ns = sample.ns_per_operation?;
                Some(format!(
                    "B={width} {dispatch} {size} · {}{} ns/call",
                    decimal(ns),
                    mark(sample)
                ))
            })
        })
        .collect();
    if timings.is_empty() {
        "Not measured".into()
    } else {
        timings.join(" / ")
    }
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
