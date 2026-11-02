//! Human-readable views derived from the same sealed evidence as JSON output.
use crate::{evidence::Record, util::Result};
use std::{fmt::Write as _, fs, path::Path};
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn seconds(ns: Option<u64>) -> String {
    ns.map(|n| format!("{:.2}", n as f64 / 1e9))
        .unwrap_or_else(|| "—".into())
}
pub fn batch_markdown(r: &crate::batch::Record) -> String {
    let mut text = format!(
        "# Fountain evaluation batch\n\nRun: `{}`. Action: **{}**. Status: **{}**. Elapsed: **{} s**.\n\n",
        r.run_id,
        r.action,
        r.status,
        seconds(Some(r.elapsed_ns))
    );
    text.push_str("Each C runs the complete selected T with its own applicability and evidence. Components execute sequentially in numeric order. A fatal result ends that C; other selections continue. Interruption leaves later selections unexecuted.\n\nElapsed seconds are host wall time, including resolution, build, validation, measurements, diagnostics and per-C report/seal generation. They are not codec timings. The batch total also includes orchestration/checkpoints, but excludes writing its own final report and seal. A failed or unsupported row is time to that outcome, not the time needed to complete T.\n\n| C | Scope | Status | Elapsed s | Resolve s | Build s | Validate s | Measure s | Diagnose s | Checks | Warnings | Samples | Diagnostics | Skipped checks | Report |\n| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for e in &r.entries {
        let scope = e
            .request
            .components
            .scope()
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|_| "Invalid".into());
        let link = e
            .directory
            .as_ref()
            .map(|p| format!("[details]({}/report.md)", p.display()))
            .unwrap_or_else(|| "No child result".into());
        let _ = writeln!(
            text,
            "| C{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            e.component_id,
            scope,
            e.status,
            seconds(e.elapsed_ns),
            seconds(e.phases_ns.get("resolve").copied()),
            seconds(e.phases_ns.get("build").copied()),
            seconds(e.phases_ns.get("validate").copied()),
            seconds(e.phases_ns.get("measure").copied()),
            seconds(e.phases_ns.get("diagnose").copied()),
            e.validation_observations,
            e.warnings,
            e.samples,
            e.diagnostics,
            e.skipped,
            link
        );
    }
    text.push_str("\nPhase columns describe host orchestration, including checks and evidence processing within that phase. Probe, setup and final report/seal work explain the remaining time. Do not add benchmark medians to reproduce these wall times.\n\n## Selected inputs and outcomes\n\n");
    for e in &r.entries {
        let _ = writeln!(
            text,
            "### C{}\n\n```json\n{}\n```\n",
            e.component_id,
            serde_json::to_string_pretty(&e.request).unwrap()
        );
        if let Some(error) = &e.error {
            let _ = writeln!(text, "Outcome detail:\n\n```text\n{error}\n```\n");
        }
    }
    text.push_str("## Reading performance\n\nFollow each C's detailed report for primitive timings, plan generation, codec throughput, dispersion and separate stage/resource observations. These summaries are not an automatic ranking. Use `compare` on two per-C result directories to check T/E/I compatibility before interpreting relative changes. Missing components remain explicit skips; an inferred trait dependency or validation reference is never a second tested component.\n");
    text
}
pub fn batch_html(r: &crate::batch::Record) -> String {
    let mut text = format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Fountain evaluation batch</title><style>body{{font:15px system-ui;margin:32px;color:#172033}}table{{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}}td,th{{padding:9px;border-bottom:1px solid #ddd;text-align:left}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}details{{margin:12px 0}}</style><h1>Fountain evaluation batch</h1><p>Run <code>{}</code> · <strong>{}</strong> · {} s</p><p>Elapsed time includes building and producing each C's evidence. Failed rows show time to failure. Benchmark performance is in the linked reports.</p><table><thead><tr><th>C</th><th>Scope</th><th>Status</th><th>Elapsed s</th><th>Checks</th><th>Warnings</th><th>Samples</th><th>Diagnostics</th><th>Skipped</th><th>Report</th></tr></thead><tbody>",
        escape(&r.run_id),
        escape(&r.status),
        seconds(Some(r.elapsed_ns))
    );
    for e in &r.entries {
        let scope = e
            .request
            .components
            .scope()
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|_| "Invalid".into());
        let link = e
            .directory
            .as_ref()
            .map(|p| {
                format!(
                    "<a href=\"{}/report.html\">Detailed results</a>",
                    escape(&p.to_string_lossy())
                )
            })
            .unwrap_or_else(|| "Not executed".into());
        let _ = write!(
            text,
            "<tr><td>C{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            e.component_id,
            scope,
            escape(&e.status),
            seconds(e.elapsed_ns),
            e.validation_observations,
            e.warnings,
            e.samples,
            e.diagnostics,
            e.skipped,
            link
        );
    }
    text.push_str("</tbody></table><h2>Inputs and failures</h2>");
    for e in &r.entries {
        let _ = write!(
            text,
            "<details><summary>C{} · {}</summary><pre>{}</pre><pre>{}</pre></details>",
            e.component_id,
            escape(&e.status),
            escape(&serde_json::to_string_pretty(&e.request).unwrap()),
            escape(
                e.error
                    .as_deref()
                    .unwrap_or("No failure recorded; review warning and coverage details.")
            )
        );
    }
    let _ = write!(
        text,
        "<details><summary>Phase timings and interpretation</summary><pre>{}</pre></details></html>",
        escape(&batch_markdown(r))
    );
    text
}
pub fn write_batch(directory: &Path, r: &crate::batch::Record) -> Result<()> {
    fs::write(directory.join("report.md"), batch_markdown(r)).map_err(|e| e.to_string())?;
    fs::write(directory.join("report.html"), batch_html(r)).map_err(|e| e.to_string())?;
    let mut csv="component_id,scope,status,elapsed_ns,validation_observations,warnings,samples,diagnostics,skipped_validation\n".to_owned();
    for e in &r.entries {
        let _ = writeln!(
            csv,
            "C{},{:?},{},{},{},{},{},{},{}",
            e.component_id,
            e.request.components.scope().map_err(|e| e.to_string())?,
            e.status,
            e.elapsed_ns.map(|n| n.to_string()).unwrap_or_default(),
            e.validation_observations,
            e.warnings,
            e.samples,
            e.diagnostics,
            e.skipped
        );
    }
    fs::write(directory.join("runs.csv"), csv).map_err(|e| e.to_string())
}
fn number(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.3}")).unwrap_or_else(|| "—".into())
}
pub fn markdown(r: &Record) -> String {
    let mut s = format!(
        "# Fountain evaluation\n\nRun: `{}`. Status: **{}**. Last stage: **{}**.\n\n",
        r.run_id, r.status, r.stage
    );
    if let Some(e) = &r.error {
        let _ = writeln!(s, "Failure: {e}\n");
    }
    let _ = writeln!(
        s,
        "Component selection: C{}. Execution through the last phase: **{} s**, excluding final report/seal generation. Phase durations (host orchestration, not benchmark samples):\n\n```json\n{}\n```\n",
        r.configuration.component_id,
        seconds(Some(r.timing.elapsed_ns)),
        serde_json::to_string_pretty(&r.timing.phases_ns).unwrap()
    );
    let _ = writeln!(
        s,
        "Correctness: **{}**; {} fatal checks, {} boundary checks, {} warning observations.\n",
        r.correctness.status,
        r.correctness.fatal_checks,
        r.correctness.warning_checks,
        r.correctness.warnings.len()
    );
    s.push_str("Warnings preserve their measured performance. An incomplete recovery has no successful source throughput. Skipped and unexecuted cases are not passes.\n\n");
    if !r.correctness.fatal_failures.is_empty() {
        s.push_str("## Fatal correctness failures\n\nThese checks stopped this C before successful measurement.\n\n");
        for failure in &r.correctness.fatal_failures {
            let _ = writeln!(
                s,
                "```json\n{}\n```\n",
                serde_json::to_string_pretty(failure).unwrap()
            );
        }
    }
    s.push_str("## Measurement interpretation\n\nOrdinary samples use uninstrumented drivers. Chain/resource diagnostics use a separate build and separate processes; do not pool their times. Gbit/s is decimal: bytes × 8 / nanoseconds. Source throughput uses K × symbol width × blocks, while repair throughput uses actually generated repair bytes. Operation timings are per public call; logical bytes are not memory-bus traffic. The median is across all recorded samples; worker medians and coefficient of variation expose dispersion. No performance threshold determines pass/fail.\n\n");
    if let Some(q) = &r.quadruple {
        let _ = writeln!(
            s,
            "## Identity\n\n- I: `{}` (`{}`)\n- T: `{}` (`{}`)\n- E: `{}`\n\nFull component identities, compiler settings and dependencies are in `result.json`.\n",
            q.i.id, q.i.digest, q.t.id, q.t.digest, q.e.digest
        );
    }
    s.push_str("## Ordinary measurements\n\n| Case | Samples | Median ns | Min–max ns | CV | ns/call | Source Gbit/s | Repair Gbit/s | Complete/incomplete |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &r.summary {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {}–{} | {:.3} | {} | {} | {} | {}/{} |",
            row.case,
            row.samples,
            row.median_ns,
            row.min_ns,
            row.max_ns,
            row.coefficient_of_variation,
            number(row.ns_per_operation),
            number(row.source_payload_gbps),
            number(row.repair_output_gbps),
            row.completed,
            row.incomplete
        );
    }
    s.push_str("\n## Boundary warnings\n\n");
    if r.correctness.warnings.is_empty() {
        s.push_str("None recorded. Check coverage before inferring that a boundary was tested.\n");
    }
    for w in &r.correctness.warnings {
        let _ = writeln!(
            s,
            "```json\n{}\n```\n",
            serde_json::to_string_pretty(w).unwrap()
        );
    }
    s.push_str("\n## Coverage\n\n| Case | Validation | Measurement | Reason |\n| --- | --- | --- | --- |\n");
    let coverage = crate::coverage::report(r);
    if let Some(rows) = coverage["cases"].as_array() {
        for row in rows {
            let reason = &row["validation"]["reason"];
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} |",
                row["case"].as_str().unwrap_or(""),
                row["validation"]["status"].as_str().unwrap_or(""),
                row["measurement"]["status"].as_str().unwrap_or(""),
                reason["reason"]
                    .as_str()
                    .or_else(|| reason.as_str())
                    .unwrap_or("")
            );
        }
    }
    s.push_str("\n## Chain and resource diagnostics\n\nExclusive stages plus the reported unattributed interval equal that diagnostic sample's lifecycle total. Nested upstream counters overlap and must not be added to their parents. Ordinary total time remains the performance result. Resource scope explicitly distinguishes native allocations from callback-side allocations; a private dynamic library's allocator is not observed by the driver. Missing internal observations remain in their combined parent stage.\n\n");
    for d in &r.diagnostics {
        let _ = writeln!(
            s,
            "### {} / worker {}\n\n```json\n{}\n```\n",
            d["case"].as_str().unwrap_or(""),
            d["worker"],
            serde_json::to_string_pretty(d).unwrap()
        );
    }
    s.push_str("\n## Reading changes\n\nUse `fountain-eval compare LEFT RIGHT` for the T/E/I comparability checks. A time ratio above 1 means the left run took longer. Inspect worker dispersion before interpreting small changes. Compare plan operation counts, actual received and generated symbols, stages and allocation observations to distinguish changed work from changed cost. These are diagnostic leads, not causal proof.\n");
    s
}
pub fn html(r: &Record) -> String {
    // Self-contained, offline-readable report with searchable details and complete evidence.
    let mut s = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Fountain evaluation</title><style>body{font:15px system-ui;max-width:1400px;margin:32px auto;padding:0 24px;color:#172033}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}td,th{padding:7px;border-bottom:1px solid #ddd;text-align:right}td:first-child,th:first-child{text-align:left;word-break:break-all}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f4f6f8;padding:16px}details{margin:12px 0}input{padding:10px;width:90%;margin:16px 0}.note{color:#475569}h2{margin-top:32px}</style><h1>Fountain evaluation</h1>",
    );
    let _ = write!(
        s,
        "<p>Run <code>{}</code> · <strong>{}</strong> · Correctness <strong>{}</strong> · {} warning observations</p>",
        escape(&r.run_id),
        escape(&r.status),
        escape(&r.correctness.status),
        r.correctness.warnings.len()
    );
    if let Some(e) = &r.error {
        let _ = write!(s, "<pre>{}</pre>", escape(e));
    }
    if !r.correctness.fatal_failures.is_empty() {
        s.push_str("<h2>Fatal correctness failures</h2><p>These checks stopped this C before successful measurement.</p>");
        for failure in &r.correctness.fatal_failures {
            let _ = write!(
                s,
                "<details open><summary>{}</summary><pre>{}</pre></details>",
                escape(failure["case"].as_str().unwrap_or("execution integrity")),
                escape(&serde_json::to_string_pretty(failure).unwrap())
            );
        }
    }
    s.push_str("<p class=\"note\">Source and repair throughput use different byte counts. CV describes within-run dispersion. Incomplete recovery has no successful throughput. Diagnostic times are separate from ordinary measurements.</p><label>Filter cases <input id=\"query\" type=\"search\" placeholder=\"e.g. K5008, plan-build, self-xor\"></label><table><thead><tr><th>Case</th><th>Median ns</th><th>Min–max ns</th><th>CV</th><th>ns/call</th><th>Source Gbit/s</th><th>Repair Gbit/s</th><th>Samples</th><th>Complete/incomplete</th></tr></thead><tbody>");
    for row in &r.summary {
        let _ = write!(
            s,
            "<tr data-case><td>{}</td><td>{}</td><td>{}–{}</td><td>{:.3}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}</td></tr>",
            escape(&row.case),
            row.median_ns,
            row.min_ns,
            row.max_ns,
            row.coefficient_of_variation,
            number(row.ns_per_operation),
            number(row.source_payload_gbps),
            number(row.repair_output_gbps),
            row.samples,
            row.completed,
            row.incomplete
        );
    }
    s.push_str("</tbody></table><h2>Warnings</h2>");
    for w in &r.correctness.warnings {
        let _ = write!(
            s,
            "<details data-case><summary>{}</summary><pre>{}</pre></details>",
            escape(w["case"].as_str().unwrap_or("warning")),
            escape(&serde_json::to_string_pretty(w).unwrap())
        );
    }
    s.push_str("<h2>Chain and resource diagnostics</h2><p>Each entry is a separate instrumented observation. Exclusive stages plus unattributed time equal its parent. Nested internal counters cannot be added to parents.</p>");
    for d in &r.diagnostics {
        let case = d["case"].as_str().unwrap_or("");
        let ordinary = r.summary.iter().find(|v| v.case == case);
        let ratio = ordinary.and_then(|v| {
            d["elapsed_ns"]
                .as_u64()
                .map(|n| n as f64 / v.median_ns as f64)
        });
        let _ = write!(
            s,
            "<details data-case><summary>{} / worker {} · {} ns</summary><p>Diagnostic / ordinary median ratio: {}. This ratio includes instrumentation and run variability.</p>",
            escape(case),
            d["worker"],
            d["elapsed_ns"],
            number(ratio)
        );
        let stages = &d["observation"]["stages"];
        if let Some(parts) = stages["exclusive_ns"].as_object() {
            let total = stages["total_ns"].as_u64().unwrap_or(0);
            s.push_str("<table><thead><tr><th>Exclusive stage</th><th>ns</th><th>Share of lifecycle</th></tr></thead><tbody>");
            for (name, value) in parts {
                let n = value.as_u64().unwrap_or(0);
                let percent = if total > 0 {
                    n as f64 * 100.0 / total as f64
                } else {
                    0.0
                };
                let _ = write!(
                    s,
                    "<tr><td>{}</td><td>{}</td><td>{:.2}%</td></tr>",
                    escape(name),
                    n,
                    percent
                );
            }
            let _ = write!(
                s,
                "<tr><td>Unattributed interval</td><td>{}</td><td></td></tr><tr><th>Lifecycle total</th><th>{}</th><th>100%</th></tr></tbody></table>",
                stages["unattributed_ns"], total
            );
        }
        s.push_str("<h3>Allocation observation</h3><table><thead><tr><th>Metric</th><th>Observed value</th></tr></thead><tbody>");
        if let Some(resources) = d["resources"].as_object() {
            for (name, value) in resources {
                let _ = write!(
                    s,
                    "<tr><td>{}</td><td>{}</td></tr>",
                    escape(name),
                    escape(
                        &value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                    )
                );
            }
        }
        let _ = write!(
            s,
            "</tbody></table><details><summary>Operation mix, internal counters and raw evidence</summary><pre>{}</pre></details></details>",
            escape(&serde_json::to_string_pretty(d).unwrap())
        );
    }
    let _ = write!(
        s,
        "<details><summary>Complete coverage, interpretation and identities</summary><pre>{}</pre></details><script>document.getElementById('query').addEventListener('input',e=>{{let q=e.target.value.toLowerCase();document.querySelectorAll('[data-case]').forEach(x=>x.hidden=!x.textContent.toLowerCase().includes(q))}})</script></html>",
        escape(&markdown(r))
    );
    s
}
pub fn write(directory: &Path, r: &Record) -> Result<()> {
    fs::write(directory.join("report.md"), markdown(r)).map_err(|e| e.to_string())?;
    fs::write(directory.join("report.html"), html(r)).map_err(|e| e.to_string())?;
    let mut csv =
        String::from("case,worker,repetition,elapsed_ns,payload_bytes,operations,outcome\n");
    for sample in &r.samples {
        let _ = writeln!(
            csv,
            "\"{}\",{},{},{},{},{},{}",
            sample.case.replace('"', "\"\""),
            sample.worker,
            sample.repetition,
            sample.elapsed_ns,
            sample.payload_bytes,
            sample.operations,
            sample.outcome
        );
    }
    fs::write(directory.join("samples.csv"), csv).map_err(|e| e.to_string())
}

pub fn comparison(value: &serde_json::Value) -> String {
    let mut text = format!(
        "# Fountain comparison\n\n{} → {}\n\nStatus: **{}**. Attribution: {}.\n\n",
        value["left"], value["right"], value["status"], value["attribution"]
    );
    text.push_str("Candidate is the right run. Negative time change means faster. Ratios compare ordinary timings only; warnings remain visible and do not veto comparison. Inspect sample dispersion before interpreting small changes.\n\n| Case | Baseline ns | Candidate ns | Time change | Baseline CV | Candidate CV | Completed L/R | Warnings L/R |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    if let Some(rows) = value["rows"].as_array() {
        for row in rows {
            let a = row["left_median_ns"].as_f64().unwrap();
            let b = row["right_median_ns"].as_f64().unwrap();
            let _ = writeln!(
                text,
                "| {} | {} | {} | {:+.2}% | {} | {} | {}/{} | {}/{} |",
                row["case"].as_str().unwrap(),
                a,
                b,
                100.0 * (b / a - 1.0),
                row["left_cv"],
                row["right_cv"],
                row["left_completed"],
                row["right_completed"],
                row["left_warnings"].as_array().unwrap().len(),
                row["right_warnings"].as_array().unwrap().len()
            );
        }
    }
    let _ = writeln!(
        text,
        "\n## Coverage and warnings\n\n```json\n{}\n```\n\n```json\n{}\n```",
        serde_json::to_string_pretty(&value["coverage"]).unwrap(),
        serde_json::to_string_pretty(&value["correctness"]).unwrap()
    );
    text.push_str("\nCompare each run's report.html diagnostics for plan size, instruction mix, source loading, solving, release and observed allocation changes. A changed instruction count or number of received symbols changes the amount of work; it does not alone prove a faster implementation of the same operation.\n");
    text
}
pub fn comparison_html(value: &serde_json::Value) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Fountain comparison</title><style>body{{font:15px system-ui;margin:32px}}pre{{white-space:pre-wrap}}</style><pre>{}</pre></html>",
        escape(&comparison(value))
    )
}
