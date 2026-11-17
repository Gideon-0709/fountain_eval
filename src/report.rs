//! Markdown reports: one self-contained reading surface, backed by sealed evidence.
use crate::{artifact::Component, evidence::Record, measurement::Summary};
use serde_json::Value;
use std::{collections::BTreeMap, fmt::Write as _};

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
                            performance_summary(c)
                        },
                    ]
                } else {
                    vec![
                        format!("C{}", e.component_id),
                        format!(
                            "{} + {}",
                            crate::progress::source(e.request.components.engine.as_ref()),
                            crate::progress::source(e.request.components.operator.as_ref())
                        ),
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
fn sections(out: &mut String, components: &[Component]) {
    out.push_str("## Validation and warnings (V)\n\n");
    for c in components {
        findings(out, c);
    }
    if components
        .iter()
        .any(|c| c.status == "succeeded" && !c.summary.is_empty())
    {
        out.push_str("† marks benchmark timing CV above 10%.\n\n");
    }
    operation_results(out, components);
    link_results(out, components);
    workflow_results(out, components);
    coverage(out, components);
}

fn accepts(c: &Component, prefix: &str) -> bool {
    c.status == "succeeded" && c.summary.iter().any(|s| s.case.starts_with(prefix))
}

fn performance_summary(c: &Component) -> String {
    if c.status != "succeeded" {
        return format!("Not measured · {} unaccepted samples", c.sample_count);
    }
    let count = Counts::collect(case_rows(c), true).accepted;
    let incomplete = c.summary.iter().filter(|s| s.incomplete > 0).count();
    if incomplete == 0 {
        format!("{count} accepted cases · {} samples", c.sample_count)
    } else {
        format!("{count} accepted cases · {incomplete} with incomplete recovery")
    }
}

fn operation_results(out: &mut String, components: &[Component]) {
    out.push_str("## Operation tests (O)\n\n");
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
    fn collect<'a>(rows: impl IntoIterator<Item = &'a Value>, succeeded: bool) -> Self {
        let mut c = Self::default();
        for r in rows {
            match r["validation"]["status"].as_str() {
                Some("skipped") => c.validation_skips += 1,
                Some("not_executed") => c.validation_pending += 1,
                Some("passed" | "failed") => c.validations += 1,
                _ => {}
            }
            match r["measurement"]["status"].as_str() {
                Some("recorded") if r["measurement"]["accepted"] == true => c.accepted += 1,
                Some("recorded") => c.rejected += 1,
                Some("incomplete") => c.partial += 1,
                Some("not_executed") => c.measurement_pending += 1,
                Some("skipped") => c.measurement_skips += 1,
                _ => {}
            }
            match r["diagnostics"]["status"].as_str() {
                Some("recorded") if succeeded => c.diagnosed += 1,
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
    out.push_str("## Coverage and measurement context\n\n");
    table(
        out,
        &[
            "C",
            "Validation run / applicable",
            "Accepted / planned timings",
            "Partial / rejected",
            "Timing not run / skipped",
        ],
        components
            .iter()
            .map(|c| {
                let id = format!("C{}", c.component_id);
                if c.coverage["status"] == "not_planned" {
                    return vec![
                        id,
                        "Plan unavailable".into(),
                        "Not measured".into(),
                        "—".into(),
                        "—".into(),
                    ];
                }
                let n = Counts::collect(case_rows(c), c.status == "succeeded");
                vec![
                    id,
                    format!("{}/{}", n.validations, n.validations + n.validation_pending),
                    if n.planned() == 0 {
                        "Not measured (0 planned)".into()
                    } else {
                        format!("{}/{}", n.accepted, n.planned())
                    },
                    format!("{} / {}", n.partial, n.rejected),
                    format!("{} / {}", n.measurement_pending, n.measurement_skips),
                ]
            })
            .collect(),
    );
    out.push_str("Partial timings have fewer than the required samples. Rejected timings have a complete sample set from a run that failed or stopped. An accepted recovery case can contain incomplete attempts.\n\n");

    if components
        .iter()
        .any(|c| c.coverage["status"] != "not_planned")
    {
        family_coverage(out, components);
    }
    for c in components {
        let mut reasons = BTreeMap::<&str, (usize, usize)>::new();
        for row in case_rows(c) {
            for (scope, timing) in [("validation", false), ("measurement", true)] {
                let entry = &row[scope];
                if entry["status"] == "skipped" {
                    let reason = entry["reason"]["reason"]
                        .as_str()
                        .unwrap_or("Reason unavailable");
                    let counts = reasons.entry(reason).or_default();
                    if timing {
                        counts.1 += 1;
                    } else {
                        counts.0 += 1;
                    }
                }
            }
        }
        if !reasons.is_empty() {
            let _ = writeln!(out, "**C{} skip reasons**\n", c.component_id);
            for (reason, (checks, timings)) in reasons {
                let _ = writeln!(
                    out,
                    "- {} · {checks} validations, {timings} timings",
                    cell(reason)
                );
            }
            out.push('\n');
        }
    }
    out.push_str("### Reproduction\n\n");
    table(
        out,
        &["C", "Interface", "Test", "Environment digest"],
        components
            .iter()
            .map(|c| {
                let mut row = vec![format!("C{}", c.component_id)];
                if let Some(q) = &c.quadruple {
                    row.extend([
                        format!("{} · {}", q.i.id, brief(&q.i.digest, 12)),
                        format!("{} · {}", q.t.id, brief(&q.t.digest, 12)),
                        brief(&q.e.digest, 12),
                    ]);
                } else {
                    row.extend([
                        "Unavailable".into(),
                        c.request.test.version.clone(),
                        "Unavailable".into(),
                    ]);
                }
                row
            })
            .collect(),
    );
    if let Some(q) = components.iter().find_map(|c| c.quadruple.as_ref()) {
        let runtime = &q.e.conditions["runtime"];
        let cpu = runtime["cpu"]["brand"]
            .as_str()
            .or_else(|| runtime["cpu"]["model"].as_str())
            .or_else(|| runtime["cpu"].as_str())
            .unwrap_or("recorded in result index");
        let _ = writeln!(
            out,
            "Host: {} / {} · {}. Ordinary cases use {} workers × {} samples and one excluded warmup per worker. First use uses {} fresh processes.\n",
            string(&runtime["os"]),
            string(&runtime["arch"]),
            cell(cpu),
            crate::content::WORKERS,
            crate::content::SAMPLES_PER_WORKER,
            crate::content::REPETITIONS
        );
    }
    out.push_str("From the complete report directory, read all case results, diagnostics and provenance with `feval report REPORT_DIR --json`.\n");
}

fn family_coverage(out: &mut String, components: &[Component]) {
    // V06 includes timed boundary cases. The L stage families share W cases,
    // so their diagnostic observations belong to each associated family.
    out.push_str("### Coverage by test family\n\nEach cell counts cases for the selected scheme. Link stage families count accepted diagnostic cases. A case can belong to several families.\n\n");
    for (prefix, labels) in FAMILIES {
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
            let rates = Counts::collect(relevant, c.status == "succeeded");
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
            if skipped > 0 {
                let _ = write!(text, " · {skipped} skipped");
            }
            text
        });
    }
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
    let _ = writeln!(
        out,
        "**C{} · Validation {}.**\n",
        c.component_id,
        title(&c.correctness.status)
    );
    for f in &c.correctness.fatal_failures {
        let case = f["case"].as_str().unwrap_or("execution integrity");
        let message = f["message"].as_str().unwrap_or("No diagnostic message");
        let _ = writeln!(out, "**Fatal** · `{}`", cell(case));
        if let Some(context) = message
            .lines()
            .find(|line| line.starts_with("V04 ") || line.starts_with("V06 "))
        {
            let _ = writeln!(out, "\n{}\n", cell(context));
            for line in message.lines().filter(|line| {
                line.starts_with("First difference:")
                    || line.starts_with("Component call or observation panicked:")
                    || line.starts_with("Protected-symbol corruption")
                    || line.starts_with("Protected symbols became unreadable:")
                    || line.starts_with("Defined boundary")
                    || line.starts_with("Trace SHA-256:")
            }) {
                let _ = writeln!(out, "{}\n", cell(line));
            }
            if message.contains("Previous steps:") {
                out.push_str("The preceding operations and their input seeds are retained in the result index (`feval report REPORT_DIR --json`).\n\n");
            }
            continue;
        }
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
            "{} samples collected before termination without an accepted performance result.\n",
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
                "Validation observations",
                "Timed observations",
                "Example timings",
            ],
            warnings
                .into_iter()
                .map(|(name, ws)| {
                    let mut checks = BTreeMap::new();
                    let mut calls = BTreeMap::new();
                    for w in &ws {
                        let expected = &w["expected"];
                        *checks
                            .entry(difference(expected, &w["actual"]))
                            .or_default() += 1;
                        if let Some(variants) = w["measurement_summary"]["variants"].as_array() {
                            for variant in variants {
                                count_warning_calls(
                                    &mut calls,
                                    expected,
                                    &variant["actual"],
                                    variant["samples"].as_u64().unwrap_or_default(),
                                );
                            }
                        } else if let Some(observations) = w["measured_observations"].as_array() {
                            for observation in observations {
                                count_warning_calls(
                                    &mut calls,
                                    expected,
                                    &observation["actual"],
                                    1,
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
                        title(&name),
                        ws.len().to_string(),
                        list_variants(checks, "case"),
                        timed,
                        warning_timings(c, &ws),
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
) {
    for outcome in observation["outcomes"].as_array().into_iter().flatten() {
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
