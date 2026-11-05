//! Sequential multi-C execution. Each child keeps its own complete T and evidence.
use crate::{
    config::Configuration,
    evidence, execution, process,
    protocol::EvaluationRequest,
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub component_id: u32,
    pub request: EvaluationRequest,
    pub status: String,
    pub directory: Option<PathBuf>,
    pub elapsed_ns: Option<u64>,
    pub phases_ns: BTreeMap<String, u64>,
    pub validation_observations: usize,
    pub warnings: usize,
    pub samples: usize,
    pub diagnostics: usize,
    pub skipped: usize,
    pub error: Option<String>,
}
impl Entry {
    fn complete(
        &mut self,
        root: &Path,
        directory: &Path,
        record: &evidence::Record,
        elapsed: u64,
    ) -> Result<()> {
        self.directory = Some(
            directory
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .into(),
        );
        self.status = record.status.clone();
        self.elapsed_ns = Some(elapsed);
        self.phases_ns = record.timing.phases_ns.clone();
        self.validation_observations = record.validation.len();
        self.warnings = record.correctness.warnings.len();
        self.samples = record.samples.len();
        self.diagnostics = record.diagnostics.len();
        self.skipped = record.coverage["skipped_validation"].as_u64().unwrap_or(0) as usize;
        self.error = record.error.clone();
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub format_version: u32,
    pub run_id: String,
    pub action: String,
    pub status: String,
    pub interrupted: bool,
    pub started_unix_ms: u64,
    pub elapsed_ns: u64,
    pub configuration: Configuration,
    pub entries: Vec<Entry>,
}
pub fn exit_code(record: &Record) -> u8 {
    if record.status == "interrupted" {
        130
    } else if record
        .entries
        .iter()
        .any(|e| matches!(e.status.as_str(), "failed" | "not_executed"))
    {
        4
    } else if record.entries.iter().any(|e| e.status == "unsupported") {
        3
    } else {
        0
    }
}
fn status(entries: &[Entry], prepared: bool, interrupted: bool) -> String {
    if entries.iter().any(|e| e.status == "interrupted") || interrupted {
        "interrupted"
    } else if entries
        .iter()
        .any(|e| matches!(e.status.as_str(), "failed" | "not_executed"))
    {
        "completed_with_failures"
    } else if entries.iter().any(|e| e.status == "unsupported") {
        "completed_with_unsupported"
    } else if prepared {
        "prepared"
    } else {
        "succeeded"
    }
    .into()
}
pub fn run(configuration: Configuration, prepare_only: bool) -> Result<(PathBuf, Record)> {
    execute(configuration, prepare_only, None)
}
pub fn from_prepared(path: &Path) -> Result<(PathBuf, Record)> {
    let lease = crate::workspace::Lease::acquire(path)?;
    let old = load(&lease.directory)?;
    let components = old
        .entries
        .iter()
        .filter(|e| e.status == "prepared")
        .map(|e| {
            let child = member(
                &lease.directory,
                e.directory.as_ref().ok_or("prepared C has no evidence")?,
            )?;
            Ok((e.component_id, evidence::load(&child)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let origin = util::file_hash(&lease.directory.join("seal.json"))?;
    if old.action != "prepare" {
        return Err("expected a prepared batch".into());
    }
    execute(
        old.configuration.clone(),
        false,
        Some((old, components, origin, lease)),
    )
}
fn execute(
    configuration: Configuration,
    prepare_only: bool,
    prepared: Option<(
        Record,
        BTreeMap<u32, evidence::Record>,
        String,
        crate::workspace::Lease,
    )>,
) -> Result<(PathBuf, Record)> {
    process::install_handler()?;
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let id = format!("{}-{}", started.as_nanos(), std::process::id());
    let directory = configuration.run.output.join(&id);
    fs::create_dir_all(&configuration.run.output).map_err(|e| e.to_string())?;
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let lease = crate::workspace::Lease::acquire(&directory)?;
    let cache = if prepared.is_none() {
        Some(crate::workspace::create_cache(
            &directory,
            &configuration.run.cache,
            &id,
        )?)
    } else {
        None
    };
    let clock = Instant::now();
    let entries = configuration
        .selections
        .iter()
        .map(|s| Entry {
            component_id: s.id,
            request: s.request.clone(),
            status: "not_executed".into(),
            directory: None,
            elapsed_ns: None,
            phases_ns: BTreeMap::new(),
            validation_observations: 0,
            warnings: 0,
            samples: 0,
            diagnostics: 0,
            skipped: 0,
            error: Some("Waiting for execution in configured numeric order".into()),
        })
        .collect();
    let mut record = Record {
        format_version: 1,
        run_id: id,
        action: if prepare_only { "prepare" } else { "run" }.into(),
        status: "running".into(),
        interrupted: false,
        started_unix_ms: started.as_millis() as u64,
        elapsed_ns: 0,
        configuration,
        entries,
    };
    if let Some((old, _, origin, prepared_lease)) = &prepared {
        util::write_json(
            &directory.join("prepared-from.json"),
            &serde_json::json!({
                "run_id":old.run_id,"seal":origin,
                "entries":old.entries,"directory":prepared_lease.directory
            }),
        )?;
    }
    checkpoint(&directory, &record)?;
    crate::progress::batch_started(&record);
    for index in 0..record.entries.len() {
        if process::interrupted() {
            for entry in &mut record.entries[index..] {
                entry.error = Some("Not executed because the batch was interrupted".into());
                crate::progress::line(format!(
                    "C{} NOT EXECUTED | batch interrupted",
                    entry.component_id
                ));
            }
            break;
        }
        let selection = &record.configuration.selections[index];
        crate::progress::component_started(index, record.entries.len(), &record.entries[index]);
        let output = directory.join(format!("C{}", selection.id));
        let started = Instant::now();
        let result = if let Some((old, components, origin, _)) = &prepared {
            let previous = &old.entries[index];
            if previous.status != "prepared" {
                record.entries[index].error = Some(format!(
                    "Preparation ended as {}; this C was not retried",
                    previous.status
                ));
                crate::progress::line(format!(
                    "C{} NOT EXECUTED | {}",
                    selection.id,
                    record.entries[index].error.as_deref().unwrap()
                ));
                checkpoint(&directory, &record)?;
                continue;
            }
            let old = components
                .get(&selection.id)
                .ok_or("prepared C metadata missing")?;
            execution::from_prepared_record(old, origin, &output)
        } else {
            let mut input = record.configuration.selection(selection);
            input.run.output = output;
            input.run.cache = cache.as_ref().unwrap().clone();
            execution::start(input, prepare_only)
        };
        let elapsed = started.elapsed().as_nanos() as u64;
        match result {
            Ok((path, child)) => {
                record.entries[index].complete(&directory, &path, &child, elapsed)?
            }
            Err(error) => {
                crate::progress::line(format!(
                    "C{} {} | {}",
                    selection.id,
                    if process::interrupted() {
                        "INTERRUPTED"
                    } else {
                        "FAILED"
                    },
                    error
                ));
                let entry = &mut record.entries[index];
                entry.status = if process::interrupted() {
                    "interrupted"
                } else {
                    "failed"
                }
                .into();
                entry.error = Some(error);
                entry.elapsed_ns = Some(elapsed);
            }
        }
        record.elapsed_ns = clock.elapsed().as_nanos() as u64;
        checkpoint(&directory, &record)?;
    }
    record.interrupted = process::interrupted();
    record.status = status(&record.entries, prepare_only, record.interrupted);
    record.elapsed_ns = clock.elapsed().as_nanos() as u64;
    checkpoint(&directory, &record)?;
    if !prepare_only {
        crate::progress::publication();
    }
    evidence::seal(&directory)?;
    if prepare_only {
        crate::progress::line(
            "READY   Frozen workspace retained for run --prepared; no final report yet",
        );
        return Ok((directory, record));
    }
    crate::progress::line("REPORT  Writing Markdown report and sealing evidence");
    let report_directory =
        crate::report::reserve(&record.configuration.report_root(), &record.run_id)?;
    let saved = crate::artifact::Snapshot::capture(&directory)?;
    let body = crate::report::snapshot(&saved);
    let report = crate::artifact::publish(&report_directory, &body, &saved, false)?;
    let mut held = vec![&lease];
    if let Some((_, _, _, prepared_lease)) = &prepared {
        held.push(prepared_lease);
    }
    crate::artifact::cleanup_held(&directory, &report, &held)?;
    crate::progress::line("REPORT  Verified Markdown report; run records and build cache removed");
    Ok((report, record))
}
fn checkpoint(directory: &Path, record: &Record) -> Result<()> {
    fs::write(
        directory.join("config.toml"),
        &record.configuration.original,
    )
    .map_err(|e| e.to_string())?;
    util::write_json(
        &directory.join("config-resolved.json"),
        &record.configuration,
    )?;
    util::write_json(&directory.join("batch.json"), record)
}
fn member(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("invalid batch evidence path".into());
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("batch evidence escaped its directory".into());
    }
    Ok(path)
}
pub fn load(directory: &Path) -> Result<Record> {
    evidence::verify_seal(directory, "batch.json")?;
    let record: Record = util::read_json(&directory.join("batch.json"))?;
    if record.format_version != 1 || !matches!(record.action.as_str(), "run" | "prepare") {
        return Err("unsupported batch record".into());
    }
    let parsed = Configuration::parse(
        record.configuration.path.clone(),
        record.configuration.original.clone(),
    )?;
    if util::digest(&parsed) != util::digest(&record.configuration)
        || record.entries.len() != parsed.selections.len()
    {
        return Err("batch configuration differs from its recorded input".into());
    }
    for (entry, selection) in record.entries.iter().zip(&parsed.selections) {
        if !matches!(
            entry.status.as_str(),
            "succeeded" | "prepared" | "failed" | "unsupported" | "interrupted" | "not_executed"
        ) {
            return Err("unknown batch entry status".into());
        }
        if entry.component_id != selection.id || entry.request != selection.request {
            return Err("batch C identity mismatch".into());
        }
        if let Some(relative) = &entry.directory {
            let child = evidence::load(&member(directory, relative)?)?;
            if child.request != entry.request
                || child.configuration.component_id != entry.component_id
                || child.status != entry.status
            {
                return Err("batch entry does not match its child evidence".into());
            }
            if entry.samples != child.samples.len()
                || entry.validation_observations != child.validation.len()
                || entry.warnings != child.correctness.warnings.len()
                || entry.diagnostics != child.diagnostics.len()
                || entry.skipped
                    != child.coverage["skipped_validation"].as_u64().unwrap_or(0) as usize
                || entry.error != child.error
                || entry.phases_ns != child.timing.phases_ns
                || entry.elapsed_ns.is_none_or(|n| n < child.timing.elapsed_ns)
            {
                return Err("batch counts or duration do not match child evidence".into());
            }
        } else if matches!(entry.status.as_str(), "succeeded" | "prepared") {
            return Err("successful C is missing evidence".into());
        } else if entry.samples != 0
            || entry.diagnostics != 0
            || entry.validation_observations != 0
            || entry.warnings != 0
            || entry.skipped != 0
            || !entry.phases_ns.is_empty()
            || entry.error.is_none()
        {
            return Err("batch entry claims evidence without a child result".into());
        }
    }
    if record
        .entries
        .iter()
        .map(|e| u128::from(e.elapsed_ns.unwrap_or(0)))
        .sum::<u128>()
        > u128::from(record.elapsed_ns)
    {
        return Err("batch elapsed time is less than its sequential child durations".into());
    }
    if record.status
        != status(
            &record.entries,
            record.action == "prepare",
            record.interrupted,
        )
    {
        return Err("batch status contradicts its entries".into());
    }
    Ok(record)
}
