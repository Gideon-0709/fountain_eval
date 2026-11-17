//! The single durable Markdown artifact. Raw evidence is a temporary execution workspace.
use crate::{
    batch, catalog, evidence, measurement,
    util::{self, Result},
    validation,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MARKER: &str = "\n<!-- fountain-eval-result\n";
const LIMIT: u64 = 64 * 1024 * 1024;

fn valid_run_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Reserve a fresh directory for a report's verified publication.
pub fn reserve(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_run_id(id) {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Component {
    pub component_id: u32,
    pub run_id: String,
    pub status: String,
    pub stage: String,
    pub validation_count: usize,
    pub sample_count: usize,
    pub diagnostic_count: usize,
    pub diagnostic_summaries: BTreeMap<String, Value>,
    pub timing: crate::execution::Timing,
    pub planned_measurements: usize,
    pub request: crate::protocol::EvaluationRequest,
    pub quadruple: Option<evidence::Quadruple>,
    pub plan: Option<catalog::Recipe>,
    pub build: Option<crate::build::Built>,
    pub correctness: validation::Report,
    pub coverage: Value,
    pub summary: Vec<measurement::Summary>,
    pub aggregation_error: Option<String>,
    pub fatal_witnesses: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub codec_validation_observations: BTreeMap<String, Value>,
    pub error: Option<String>,
    pub failure_details: Vec<String>,
    /// Prepared metadata contains no samples and references immutable cache inputs.
    pub prepared: Option<evidence::Record>,
}
impl Component {
    pub fn from_record(r: &evidence::Record) -> Result<Self> {
        if r.status == "succeeded" {
            evidence::validate_samples(r)?;
        }
        let mut correctness = validation::report(r);
        for w in &mut correctness.warnings {
            let observations = w
                .as_object_mut()
                .unwrap()
                .remove("measured_observations")
                .unwrap_or(json!([]));
            let observations = observations
                .as_array()
                .ok_or("invalid warning observations")?;
            let mut variants = BTreeMap::<String, (Value, usize)>::new();
            for o in observations {
                let entry = variants
                    .entry(util::digest(&o["actual"]))
                    .or_insert((o["actual"].clone(), 0));
                entry.1 += 1;
            }
            w["measurement_summary"] = json!({"samples":observations.len(),"differing":observations.iter().filter(|v|v["matches_reference"]==false).count(),"variants":variants.into_values().map(|(actual,count)|json!({"actual":actual,"samples":count})).collect::<Vec<_>>()});
        }
        let fatal_witnesses = r
            .validation
            .iter()
            .filter(|v| v["severity"] == "fatal" && v["status"] == "passed")
            .filter_map(|v| {
                v["case"]
                    .as_str()
                    .map(|id| (id.to_owned(), v["witness"].clone()))
            })
            .collect();
        let (summary, aggregation_error) = match measurement::summarize(&r.samples) {
            Ok(values) => (values, None),
            Err(error) if r.status != "succeeded" => (Vec::new(), Some(error)),
            Err(error) => return Err(error),
        };
        Ok(Self {
            component_id: r.configuration.component_id,
            run_id: r.run_id.clone(),
            status: r.status.clone(),
            stage: r.stage.clone(),
            validation_count: r.validation.len(),
            sample_count: r.samples.len(),
            diagnostic_count: r.diagnostics.len(),
            diagnostic_summaries: aggregate_diagnostics(&r.diagnostics),
            timing: r.timing.clone(),
            planned_measurements: r.recipe.as_ref().map_or(0, |p| p.cases.len()),
            request: r.request.clone(),
            quadruple: r.quadruple.clone(),
            plan: r.recipe.clone(),
            build: r.built.clone(),
            correctness,
            coverage: crate::coverage::report(r),
            summary,
            aggregation_error,
            fatal_witnesses,
            codec_validation_observations: r
                .validation
                .iter()
                .filter_map(|event| {
                    let id = event["case"].as_str()?;
                    let check = r.recipe.as_ref()?.checks.get(id)?;
                    (check.spec.category == "correctness"
                        && check.spec.scheme.is_some()
                        && event["status"] == "passed")
                        .then(|| (id.to_owned(), event["details"].clone()))
                })
                .collect(),
            error: r.error.clone(),
            failure_details: Vec::new(),
            prepared: (r.status == "prepared").then(|| r.clone()),
        })
    }
    pub fn verify_comparison(&self) -> Result<()> {
        if self.status != "succeeded" {
            return Err("not comparable: both runs must have succeeded".into());
        }
        let q = self.quadruple.as_ref().ok_or("missing tuple")?;
        let recipe = catalog::recipe(&self.request.test)?;
        if q.t != recipe.identity {
            return Err("test identity collision or different catalog candidate".into());
        }
        if !catalog::registered_interface(&q.i, &recipe.identity.id) {
            return Err("interface identity collision or unavailable interface".into());
        }
        if q.e.digest != util::digest(&q.e.conditions) {
            return Err("environment identity mismatch".into());
        }
        let plan = catalog::plan(
            &self.request.test,
            &self.request.scheme,
            q.i.id.split('@').next().unwrap_or(""),
            self.request.components.scope().map_err(|e| e.to_string())?,
        )?;
        if self.plan.as_ref().map(util::digest) != Some(util::digest(&plan)) {
            return Err("stored test plan differs from its registered content".into());
        }
        let cases: std::collections::BTreeSet<_> =
            self.summary.iter().map(|s| s.case.clone()).collect();
        if cases.len() != self.summary.len()
            || cases != plan.cases.iter().cloned().collect()
            || self.summary.iter().any(|s| {
                s.samples != plan.repetitions
                    || s.completed + s.incomplete != s.samples
                    || s.median_ns == 0
                    || s.min_ns > s.median_ns
                    || s.max_ns < s.median_ns
            })
        {
            return Err("invalid or incomplete aggregate result coverage".into());
        }
        let expected: std::collections::BTreeSet<_> = plan
            .checks
            .iter()
            .filter(|(_, c)| c.skip.is_none() && c.severity == validation::Severity::Fatal)
            .map(|(id, _)| id.clone())
            .collect();
        if expected != self.fatal_witnesses.keys().cloned().collect()
            || !self.correctness.fatal_failures.is_empty()
        {
            return Err("fatal-check witness coverage is incomplete".into());
        }
        Ok(())
    }
}
/// Lossless metric coverage with aggregate values; no per-observation timing array.
fn aggregate_diagnostics(events: &[Value]) -> BTreeMap<String, Value> {
    fn fields(path: &str, value: &Value, out: &mut BTreeMap<String, Vec<Value>>) {
        match value {
            Value::Object(m) => {
                for (k, v) in m {
                    fields(
                        &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                        v,
                        out,
                    );
                }
            }
            Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    fields(&format!("{path}/{i}"), v, out);
                }
            }
            _ => out.entry(path.into()).or_default().push(value.clone()),
        }
    }
    let mut grouped = BTreeMap::<String, Vec<&Value>>::new();
    for event in events {
        grouped
            .entry(event["case"].as_str().unwrap_or("unidentified").into())
            .or_default()
            .push(event);
    }
    grouped.into_iter().map(|(case,events)| {
        let mut values=BTreeMap::new();
        for event in &events {for key in ["elapsed_ns","observation","resources"] {fields(key,&event[key],&mut values);}}
        let metrics:BTreeMap<_,_>=values.into_iter().map(|(key,values)| {
            let value=if let Some(ns)=values.iter().map(Value::as_f64).collect::<Option<Vec<_>>>() {
                json!({"mean":ns.iter().sum::<f64>()/ns.len()as f64,"min":ns.iter().copied().reduce(f64::min),"max":ns.iter().copied().reduce(f64::max),"n":ns.len()})
            } else {
                let mut variants=BTreeMap::<String,(Value,usize)>::new();
                for v in values {let row=variants.entry(util::digest(&v)).or_insert((v,0));row.1+=1;}
                json!({"variants":variants.into_values().map(|(value,count)|json!({"value":value,"n":count})).collect::<Vec<_>>()})
            };(key,value)
        }).collect();
        (case,json!({"observations":events.len(),"metrics":metrics}))
    }).collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub batch: batch::Record,
    pub components: Vec<Component>,
    pub source_seal: String,
    pub prepared_from: Option<Value>,
}
fn log_excerpt(path: &Path, text: &str) -> String {
    let compact = if text
        .lines()
        .any(|line| line.trim_start().starts_with("left:"))
    {
        crate::validation::message_summary(text)
    } else {
        text.to_owned()
    };
    let lines: Vec<_> = compact.lines().collect();
    let mut selected = std::collections::BTreeSet::new();
    selected.extend(0..lines.len().min(12));
    selected.extend(lines.len().saturating_sub(40)..lines.len());
    // Keep primary compiler diagnostics even when a long warning tail follows them.
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("error") || line.contains("panicked at") {
            selected.extend(i.saturating_sub(2)..(i + 12).min(lines.len()));
        }
    }
    let mut out = format!(
        "{}: {} of {} lines (diagnostics and context)\n",
        path.display(),
        selected.len(),
        lines.len()
    );
    let mut previous = None;
    for i in selected {
        if previous.is_some_and(|p| i > p + 1) {
            out.push_str("[... omitted ...]\n");
        }
        out.push_str(&crate::validation::message_summary(lines[i]));
        out.push('\n');
        previous = Some(i);
    }
    out
}

impl Snapshot {
    pub fn capture(directory: &Path) -> Result<Self> {
        let mut components = Vec::new();
        let batch = batch::load_with_children(directory, |_, child, record| {
            let mut c = Component::from_record(&record)?;
            if matches!(c.status.as_str(), "failed" | "interrupted") {
                for path in util::files(&child.join("logs")).unwrap_or_default() {
                    let name = path.to_string_lossy();
                    let relevant = match record.stage.as_str() {
                        "build" => name.contains("build"),
                        "validate" => name.starts_with("validate"),
                        "measure" => name.starts_with("measure") || name.starts_with("first-use"),
                        "diagnose" => name.starts_with("diagnose"),
                        _ => true,
                    };
                    if relevant && path.extension().is_some_and(|v| v == "stderr") {
                        let text =
                            fs::read_to_string(child.join("logs").join(&path)).unwrap_or_default();
                        if !text.trim().is_empty() {
                            c.failure_details.push(log_excerpt(&path, &text));
                        }
                    }
                }
            }
            components.push(c);
            Ok(())
        })?;
        Ok(Self {
            batch,
            components,
            source_seal: util::file_hash(&directory.join("seal.json"))?,
            prepared_from: if directory.join("prepared-from.json").exists() {
                Some(util::read_json(&directory.join("prepared-from.json"))?)
            } else {
                None
            },
        })
    }
    fn verify(&self) -> Result<()> {
        if !valid_run_id(&self.batch.run_id) {
            return Err("invalid report run identity".into());
        }
        if self
            .batch
            .entries
            .iter()
            .map(|e| u128::from(e.elapsed_ns.unwrap_or(0)))
            .sum::<u128>()
            > u128::from(self.batch.elapsed_ns)
        {
            return Err("invalid batch elapsed time".into());
        }
        let parsed = crate::config::Configuration::parse(
            self.batch.configuration.path.clone(),
            self.batch.configuration.original.clone(),
        )?;
        if util::digest(&parsed) != util::digest(&self.batch.configuration)
            || parsed.selections.len() != self.batch.entries.len()
        {
            return Err("report configuration mismatch".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &self.components {
            if !seen.insert(c.component_id) {
                return Err("duplicate component in report".into());
            }
            let e = self
                .batch
                .entries
                .iter()
                .find(|e| e.component_id == c.component_id)
                .ok_or("unexpected report component")?;
            if e.status != c.status
                || e.request != c.request
                || e.warnings != c.correctness.warnings.len()
                || e.validation_observations != c.validation_count
                || e.samples != c.sample_count
                || e.diagnostics != c.diagnostic_count
                || c.diagnostic_count
                    != c.diagnostic_summaries
                        .values()
                        .map(|d| d["observations"].as_u64().unwrap_or(0) as usize)
                        .sum::<usize>()
                || e.error != c.error
                || e.skipped != c.coverage["skipped_validation"].as_u64().unwrap_or(0) as usize
                || e.phases_ns != c.timing.phases_ns
                || e.elapsed_ns.is_none_or(|n| n < c.timing.elapsed_ns)
                || (c.aggregation_error.is_none()
                    && e.samples != c.summary.iter().map(|s| s.samples).sum::<usize>())
                || (c.status == "succeeded" && c.aggregation_error.is_some())
            {
                return Err("report component counts or identity mismatch".into());
            }
        }
        for (entry, selection) in self.batch.entries.iter().zip(&parsed.selections) {
            if entry.component_id != selection.id
                || entry.request != selection.request
                || entry.directory.is_some() != seen.contains(&entry.component_id)
            {
                return Err("report selection mismatch".into());
            }
        }
        Ok(())
    }
}
fn encode(body: &str, snapshot: &Snapshot) -> Result<String> {
    snapshot.verify()?;
    let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
    serde_json::to_writer(&mut compressor, snapshot).map_err(|e| e.to_string())?;
    let encoded = STANDARD.encode(compressor.finish().map_err(|e| e.to_string())?);
    let digest = util::hash(format!("{body}{encoded}"));
    Ok(format!("{body}{MARKER}{digest}\n{encoded}\n-->\n"))
}
/// Read the verified visible Markdown and its result index.
pub fn load(path: &Path) -> Result<(String, Snapshot)> {
    let path = if path.is_dir() {
        path.join("report.md")
    } else {
        path.into()
    };
    let mut text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (body, index) = text
        .rsplit_once(MARKER)
        .ok_or("report has no result index; regenerate it from existing raw evidence")?;
    let mut lines = index.lines();
    let digest = lines.next().ok_or("missing report checksum")?;
    let encoded = lines.next().ok_or("missing report index")?;
    if lines.next() != Some("-->")
        || lines.next().is_some()
        || util::hash(format!("{body}{encoded}")) != digest
    {
        return Err("report checksum mismatch".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    ZlibDecoder::new(bytes.as_slice())
        .take(LIMIT + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > LIMIT {
        return Err("report index exceeds size limit".into());
    }
    let snapshot: Snapshot = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    snapshot.verify()?;
    text.truncate(body.len());
    Ok((text, snapshot))
}
/// Atomic replacement is explicit; verification failure leaves the existing report intact.
pub fn publish(
    directory: &Path,
    body: &str,
    snapshot: &Snapshot,
    replace: bool,
) -> Result<PathBuf> {
    if body.trim().is_empty() {
        return Err("refusing to publish an empty report".into());
    }
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join("report.md");
    if path.exists() {
        if !replace {
            return Err("report already exists".into());
        }
        let old = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if !old.contains(&format!("Run: `{}`", snapshot.batch.run_id)) {
            return Err("replacement report run identity mismatch".into());
        }
    }
    let text = encode(body, snapshot)?;
    let temporary = directory.join(format!(".report-{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        let (_, verified) = load(&temporary)?;
        if util::digest(&verified) != util::digest(snapshot) {
            return Err("report round-trip verification failed".into());
        }
        if replace {
            fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        } else {
            fs::hard_link(&temporary, &path).map_err(|e| e.to_string())?;
            fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        }
        fs::File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(path.clone())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}
/// Delete the verified batch, its owned cache and a consumed preparation workspace.
pub fn cleanup(raw: &Path, report: &Path) -> Result<()> {
    cleanup_held(raw, report, &[])
}
pub(crate) fn cleanup_held(
    raw: &Path,
    report: &Path,
    held: &[&crate::workspace::Lease],
) -> Result<()> {
    let report = report.canonicalize().map_err(|e| e.to_string())?;
    let (_, saved) = load(&report)?;
    let mut leases = Vec::new();
    let mut pending = vec![(
        raw.to_owned(),
        saved.batch.clone(),
        saved.source_seal.clone(),
    )];
    if let Some(origin) = &saved.prepared_from
        && let Some(path) = origin["directory"].as_str()
    {
        let path = PathBuf::from(path);
        // A completed cleanup is idempotent if only final raw removal previously failed.
        if path.exists() {
            let old = batch::load(&path)?;
            if old.action != "prepare"
                || origin["run_id"] != old.run_id
                || origin["entries"]
                    != serde_json::to_value(&old.entries).map_err(|e| e.to_string())?
            {
                return Err("prepared origin differs from published provenance; retained".into());
            }
            pending.push((
                path,
                old,
                origin["seal"]
                    .as_str()
                    .ok_or("missing prepared seal")?
                    .to_owned(),
            ));
        }
    }
    let mut targets = Vec::new();
    for (path, record, seal) in pending {
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("refusing to clean a symlinked batch".into());
        }
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if !held.iter().any(|lease| lease.directory == path) {
            leases.push(crate::workspace::Lease::acquire(&path)?);
        }
        if report.starts_with(&path)
            || path.file_name().and_then(|v| v.to_str()) != Some(&record.run_id)
        {
            return Err("unsafe report/evidence overlap".into());
        }
        if util::file_hash(&path.join("seal.json"))? != seal
            || util::digest(&batch::load(&path)?) != util::digest(&record)
        {
            return Err("raw evidence changed after report publication; retained".into());
        }
        let cache = crate::workspace::cache_for_cleanup(&path, &record, &report)?;
        targets.push((path, cache));
    }
    // Origin first: retain the current batch and its recovery index if cleanup fails partway.
    for (path, cache) in targets.into_iter().rev() {
        if let Some(cache) = cache {
            crate::workspace::remove(&cache, &report)?;
        }
        crate::workspace::remove(&path, &report)?;
    }
    Ok(())
}
pub fn select(input: &Path) -> Result<Component> {
    let text = input.to_str().ok_or("invalid report path")?;
    let (path, id) = if let Some((path, id)) = text.rsplit_once("#C") {
        (
            PathBuf::from(path),
            Some(id.parse::<u32>().map_err(|_| "invalid C selector")?),
        )
    } else {
        (input.into(), None)
    };
    if path.join("result.json").is_file() {
        return Component::from_record(&evidence::load(&path)?);
    }
    let (_, snapshot) = load(&path)?;
    if id.is_none() && snapshot.components.len() != 1 {
        return Err("select a component using report.md#C0 (or its configured index)".into());
    }
    snapshot
        .components
        .into_iter()
        .find(|c| id.is_none_or(|id| id == c.component_id))
        .ok_or("report component not found".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostics_survive_long_compiler_warning_tails() {
        let text = "compile progress\n".repeat(100)
            + "error[E0433]: could not find `portable` in the crate root\n --> src/slab.rs:10:1\n"
            + &"unrelated warning\n".repeat(200);
        let excerpt = super::log_excerpt(std::path::Path::new("driver-build.stderr"), &text);
        assert!(excerpt.contains("error[E0433]") && excerpt.contains("portable"));
        assert!(excerpt.contains("src/slab.rs:10:1") && excerpt.contains("omitted"));
        assert!(excerpt.lines().count() < 80);
    }
}
