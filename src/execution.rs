use crate::{
    build::{self, Built},
    catalog,
    config::RunConfiguration,
    evidence::{self, Quadruple, Record},
    process,
    source::Resolver,
    util::{self, Result},
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
/// Host-side elapsed time. These clocks do not enter measured operation/codec samples.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Timing {
    pub elapsed_ns: u64,
    pub phases_ns: std::collections::BTreeMap<String, u64>,
}
struct PhaseClock {
    start: Instant,
    phase: Instant,
    name: String,
}
impl PhaseClock {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            phase: now,
            name: "setup".into(),
        }
    }
    fn enter(&mut self, record: &mut Record, name: &str) {
        if self.name != "setup" {
            crate::progress::line(format!(
                "[C{}] {} completed in {:.2} s",
                record.configuration.component_id,
                self.name,
                self.phase.elapsed().as_secs_f64()
            ));
        }
        *record
            .timing
            .phases_ns
            .entry(self.name.clone())
            .or_default() += self.phase.elapsed().as_nanos() as u64;
        self.phase = Instant::now();
        self.name = name.into();
        record.stage = name.into();
        crate::progress::phase(record.configuration.component_id, name);
    }
    fn finish(&self, record: &mut Record) {
        *record
            .timing
            .phases_ns
            .entry(self.name.clone())
            .or_default() += self.phase.elapsed().as_nanos() as u64;
        record.timing.elapsed_ns = self.start.elapsed().as_nanos() as u64;
    }
}
fn lock(cache: &Path) -> Result<std::fs::File> {
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("execution.lock"))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut heartbeat = Instant::now();
    loop {
        match fs2::FileExt::try_lock_exclusive(&f) {
            Ok(()) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if process::interrupted() {
                    return Err("interrupted waiting for cache lock".into());
                }
                if heartbeat.elapsed() >= Duration::from_secs(15) {
                    crate::progress::line(format!(
                        "[WAITING] Another run holds the execution lock; {:.0} s elapsed",
                        start.elapsed().as_secs_f64()
                    ));
                    heartbeat = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}
fn new_directory(root: &Path) -> Result<(String, PathBuf)> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let id = format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::process::id()
    );
    let path = root.join(&id);
    fs::create_dir(&path).map_err(|e| e.to_string())?;
    Ok((id, path.canonicalize().map_err(|e| e.to_string())?))
}
pub fn start(configuration: RunConfiguration, prepare_only: bool) -> Result<(PathBuf, Record)> {
    process::install_handler()?;
    let options = configuration.run.clone();
    fs::create_dir_all(&options.cache).map_err(|e| e.to_string())?;
    let cache = options.cache.canonicalize().map_err(|e| e.to_string())?;
    let (id, directory) = new_directory(&options.output)?;
    let mut clock = PhaseClock::new();
    let mut record = Record {
        format_version: 1,
        run_id: id,
        status: "running".into(),
        stage: "queued".into(),
        request: configuration.request.clone(),
        configuration,
        quadruple: None,
        recipe: None,
        built: None,
        validation: Vec::new(),
        samples: Vec::new(),
        diagnostics: Vec::new(),
        summary: Vec::new(),
        error: None,
        timing: Timing::default(),
        correctness: Default::default(),
        coverage: json!({}),
    };
    evidence::save(&directory, &record)?;
    let result = util::catch(|| {
        crate::progress::line(format!(
            "[C{}] Waiting for the execution lock",
            record.configuration.component_id
        ));
        let _lock = lock(&cache)?;
        clock.enter(&mut record, "resolve");
        evidence::save(&directory, &record)?;
        catalog::verify_lock()?;
        let recipe = catalog::recipe(&record.request.test)?;
        record.recipe = Some(recipe.clone());
        let mut resolver = Resolver::new(&cache, &directory.join("logs"));
        let scope = record
            .request
            .components
            .scope()
            .map_err(|e| e.to_string())?;
        let operator = record
            .request
            .components
            .operator
            .as_ref()
            .map(|r| resolver.resolve(r))
            .transpose()?;
        let engine = if let Some(reference) = &record.request.components.engine {
            if reference.implementation.is_some() {
                return Err(
                    "unsupported: engine implementation selectors are not registered".into(),
                );
            }
            resolver.resolve(reference)?
        } else {
            build::operator_engine(
                &mut resolver,
                operator.as_ref().ok_or("empty component selection")?,
            )?
        };
        let family = catalog::family(&engine)?;
        let recipe = catalog::plan(&record.request.test, &record.request.scheme, &family, scope)?;
        record.recipe = Some(recipe.clone());
        crate::progress::plan(&record);
        if recipe.validation_cases().is_empty() {
            return Err("unsupported: no test cases are applicable to this single component; see the complete coverage plan".into());
        }
        let scheme = build::resolve_scheme(&mut resolver, &engine, &record.request.scheme)?;
        util::write_json(
            &directory.join("sources.json"),
            &json!({"engine":if scope==crate::protocol::Scope::Operator{None}else{Some(&engine)},"support_engine":if scope==crate::protocol::Scope::Operator{Some(&engine)}else{None},"operator":operator,"scheme":scheme}),
        )?;
        let implementation = if let Some(operator) = &operator {
            catalog::implementation(
                operator,
                record
                    .request
                    .components
                    .operator
                    .as_ref()
                    .and_then(|r| r.implementation.as_deref()),
            )?
        } else {
            "scalar-validation-only".into()
        };
        let mut environment = evidence::environment(&cache)?;
        environment.conditions["recipe"] = recipe.definition.clone();
        environment.conditions["scheme"] =
            json!({"name":record.request.scheme.name,"mode":record.request.scheme.mode});
        environment.digest = util::digest(&environment.conditions);
        util::write_json(&directory.join("configuration.json"), &environment)?;
        clock.enter(&mut record, "build");
        evidence::save(&directory, &record)?;
        let built = build::prepare(
            &mut resolver,
            build::BuildSelection {
                engine: &engine,
                operator: operator.as_ref(),
                scope,
            },
            &scheme,
            &implementation,
            &recipe,
            &environment.conditions,
        )?;
        util::write_json(&directory.join("build.json"), &built)?;
        record.built = Some(built);
        clock.enter(&mut record, "probe");
        evidence::save(&directory, &record)?;
        invoke(&directory, &record, "describe")?;
        record.quadruple = Some(Quadruple {
            c: record.built.as_ref().unwrap().components.clone(),
            i: record.built.as_ref().unwrap().interface.clone(),
            t: recipe.identity.clone(),
            e: environment,
        });
        if prepare_only {
            record.status = "prepared".into();
            record.stage = "prepared".into();
        } else {
            execute(&directory, &mut record, &mut clock)?;
        }
        Ok(())
    });
    clock.finish(&mut record);
    finish(&directory, &mut record, result)?;
    Ok((directory, record))
}
pub fn from_prepared(prepared: &Path, output: &Path) -> Result<(PathBuf, Record)> {
    process::install_handler()?;
    let old = evidence::load(prepared)?;
    if old.status != "prepared" {
        return Err("expected a prepared run".into());
    }
    let built = old.built.as_ref().ok_or("prepared build missing")?;
    let _lock = lock(
        built
            .directory
            .parent()
            .and_then(Path::parent)
            .ok_or("invalid build location")?,
    )?;
    let (id, directory) = new_directory(output)?;
    let mut clock = PhaseClock::new();
    let mut record = old.clone();
    record.run_id = id;
    record.configuration.run.output = output.into();
    record.timing = Timing::default();
    record.status = "running".into();
    clock.enter(&mut record, "probe");
    record.error = None;
    let result = util::catch(|| {
        catalog::verify_lock()?;
        let recipe = catalog::plan(
            &record.request.test,
            &record.request.scheme,
            &built.family,
            record
                .request
                .components
                .scope()
                .map_err(|e| e.to_string())?,
        )?;
        let tuple = record.quadruple.as_mut().ok_or("missing prepared tuple")?;
        if recipe.identity != tuple.t
            || !catalog::registered_interface(&tuple.i, &recipe.identity.id)
        {
            return Err("prepared I/T identity differs from this installed framework".into());
        }
        let mut current = evidence::environment(
            built
                .directory
                .parent()
                .and_then(Path::parent)
                .ok_or("invalid build location")?,
        )?;
        if current.conditions["runtime"]["arch"] != tuple.e.conditions["runtime"]["arch"]
            || current.conditions["runtime"]["cpu"] != tuple.e.conditions["runtime"]["cpu"]
            || current.conditions["runtime"]["os"] != tuple.e.conditions["runtime"]["os"]
        {
            return Err("prepared native binary requires its original CPU and OS family".into());
        }
        current.conditions["build"] = tuple.e.conditions["build"].clone();
        current.conditions["recipe"] = recipe.definition.clone();
        current.conditions["scheme"] =
            json!({"name":record.request.scheme.name,"mode":record.request.scheme.mode});
        current.digest = util::digest(&current.conditions);
        tuple.e = current;
        util::write_json(&directory.join("configuration.json"), &tuple.e)?;
        util::write_json(
            &directory.join("prepared-from.json"),
            &json!({"run_id":old.run_id,"seal":util::file_hash(&prepared.join("seal.json"))?}),
        )?;
        util::write_json(&directory.join("build.json"), built)?;
        crate::progress::plan(&record);
        invoke(&directory, &record, "describe")?;
        execute(&directory, &mut record, &mut clock)
    });
    clock.finish(&mut record);
    finish(&directory, &mut record, result)?;
    Ok((directory, record))
}
fn finish(directory: &Path, record: &mut Record, result: Result<()>) -> Result<()> {
    if let Err(error) = result {
        record.status = if process::interrupted() {
            "interrupted"
        } else if error.starts_with("unsupported:") {
            "unsupported"
        } else {
            "failed"
        }
        .into();
        record.error = Some(error);
    }
    record.correctness = crate::validation::report(record);
    record.coverage = crate::coverage::report(record);
    util::write_json(&directory.join("validation.json"), &record.validation)?;
    evidence::save(directory, record)?;
    evidence::seal(directory)?;
    crate::progress::component_finished(record);
    Ok(())
}
fn execute(directory: &Path, record: &mut Record, clock: &mut PhaseClock) -> Result<()> {
    clock.enter(record, "validate");
    evidence::save(directory, record)?;
    let outcome = invoke(directory, record, "validate");
    collect_validation(directory, record, "validate");
    outcome?;
    let recipe = record.recipe.as_ref().ok_or("missing recipe")?;
    crate::validation::verify(recipe, &record.validation)?;
    record.correctness = crate::validation::report(record);
    util::write_json(&directory.join("validation.json"), &record.validation)?;
    clock.enter(record, "measure");
    evidence::save(directory, record)?;
    let fresh: Vec<_> = record
        .recipe
        .as_ref()
        .unwrap()
        .checks
        .values()
        .filter(|c| c.skip.is_none() && c.measured && c.spec.fresh_process)
        .map(|c| c.spec.id.clone())
        .collect();
    for worker in 0..crate::content::WORKERS {
        run_measurement(
            directory,
            record,
            "measure",
            &format!("measure-{worker}"),
            worker,
            None,
        )?;
    }
    for (index, case) in fresh.iter().enumerate() {
        crate::progress::line(format!(
            "[C{}][FIRST USE {}/{}] {} | {} independent fresh processes",
            record.configuration.component_id,
            index + 1,
            fresh.len(),
            case,
            crate::content::REPETITIONS
        ));
        for worker in 0..crate::content::REPETITIONS {
            run_measurement(
                directory,
                record,
                "first-use",
                &format!("first-use-{index}-{worker}"),
                worker,
                Some(case),
            )?;
        }
    }
    util::write_json(&directory.join("samples.json"), &record.samples)?;
    clock.enter(record, "diagnose");
    evidence::save(directory, record)?;
    for worker in 0..crate::content::WORKERS {
        run_measurement(
            directory,
            record,
            "diagnose",
            &format!("diagnose-{worker}"),
            worker,
            None,
        )?;
    }
    evidence::validate_samples(record)?;
    record.summary = crate::measurement::summarize(&record.samples)?;
    record.stage = "complete".into();
    record.status = "succeeded".into();
    Ok(())
}
fn run_measurement(
    directory: &Path,
    record: &mut Record,
    mode: &str,
    label: &str,
    worker: usize,
    case: Option<&str>,
) -> Result<()> {
    let outcome = invoke_worker(directory, record, mode, label, worker, case);
    let raw =
        fs::read_to_string(directory.join(format!("logs/{label}.stdout"))).unwrap_or_default();
    for line in raw.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if value["event"] == "sample" {
                if let Ok(sample) = serde_json::from_value(value) {
                    record.samples.push(sample);
                }
            } else if value["event"] == "diagnostic" {
                record.diagnostics.push(value);
            } else if value["event"] == "validation" {
                if let Some(old) = record
                    .validation
                    .iter_mut()
                    .find(|v| v["case"] == value["case"])
                {
                    *old = value;
                } else {
                    record.validation.push(value);
                }
            }
        }
    }
    // Fresh-process events are already durable in their unique raw log. Persist the
    // full aggregate once per series, and immediately on a failed/interrupted child.
    if mode != "first-use" || outcome.is_err() {
        util::write_json(&directory.join("samples.json"), &record.samples)?;
        util::write_json(&directory.join("diagnostics.json"), &record.diagnostics)?;
        evidence::save(directory, record)?;
    }
    outcome.map(|_| ())
}
fn collect_validation(directory: &Path, record: &mut Record, mode: &str) {
    let raw = fs::read_to_string(directory.join(format!("logs/{mode}.stdout"))).unwrap_or_default();
    for line in raw.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line)
            && value["event"] == "validation"
        {
            if mode == "measure"
                && let Some(old) = record
                    .validation
                    .iter_mut()
                    .find(|v| v["case"] == value["case"])
            {
                *old = value;
            } else {
                record.validation.push(value);
            }
        }
    }
}
fn invoke(directory: &Path, record: &Record, mode: &str) -> Result<Vec<Value>> {
    invoke_worker(directory, record, mode, mode, 0, None)
}
fn invoke_worker(
    directory: &Path,
    record: &Record,
    mode: &str,
    label: &str,
    worker: usize,
    case: Option<&str>,
) -> Result<Vec<Value>> {
    let built = record.built.as_ref().ok_or("build missing")?;
    built.verify()?;
    if mode == "describe" {
        built.export_evidence(directory)?;
    }
    let mut command = Command::new(if mode == "diagnose" {
        &built.diagnostic_binary
    } else {
        &built.binary
    });
    command
        .arg(mode)
        .arg(worker.to_string())
        .current_dir(&built.directory);
    if let Some(case) = case {
        command.arg(case);
    }
    process::controlled(&mut command);
    for key in [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "LD_DEBUG",
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "DYLD_PRINT_LIBRARIES",
    ] {
        command.env_remove(key);
    }
    if built.product_library.is_some() && mode == "describe" {
        if cfg!(target_os = "macos") {
            command.env("DYLD_PRINT_LIBRARIES", "1");
        } else {
            command.env("LD_DEBUG", "libs");
        }
    }
    let mut progress = crate::progress::enabled()
        .then(|| crate::progress::DriverProgress::new(record, mode, worker, case));
    if let Some(p) = &progress
        && let Some(message) = p.start()
    {
        crate::progress::line(message);
    }
    let observe = progress.is_some();
    let mut observer = |raw: &str| {
        if let Some(p) = &mut progress
            && let Some(message) = p.observe(raw)
        {
            crate::progress::line(message);
        }
    };
    let raw = process::run_observed(
        &mut command,
        &directory.join("logs"),
        label,
        Duration::from_secs(if mode == "first-use" { 60 } else { 600 }),
        if observe { Some(&mut observer) } else { None },
    )?;
    let events: Vec<Value> = raw
        .lines()
        .map(|s| serde_json::from_str(s).map_err(|e| format!("driver protocol: {e}")))
        .collect::<Result<_>>()?;
    check_events(&events, built, record, mode)?;
    if events[0]["worker"] != worker || events[0]["diagnostic"] != (mode == "diagnose") {
        return Err("driver worker or instrumentation identity mismatch".into());
    }
    for event in events
        .iter()
        .filter(|e| matches!(e["event"].as_str(), Some("sample" | "diagnostic")))
    {
        if event["worker"] != worker || case.is_some_and(|c| event["case"] != c) {
            return Err("event assigned to the wrong worker/case".into());
        }
    }
    if let Some(library) = &built.product_library
        && mode == "describe"
    {
        let loader = fs::read_to_string(directory.join("logs/describe.stderr"))
            .map_err(|e| e.to_string())?;
        if !loader.contains(library.parent().unwrap().to_string_lossy().as_ref()) {
            return Err("product loader did not confirm the recorded library directory".into());
        }
        util::write_json(
            &directory.join("library.json"),
            &json!({"path":library,"sha256":util::file_hash(library)?,"loader_evidence":"logs/describe.stderr"}),
        )?;
    }
    Ok(events)
}
pub fn check_events(events: &[Value], built: &Built, record: &Record, mode: &str) -> Result<()> {
    let first = events.first().ok_or("empty driver output")?;
    let last = events.last().ok_or("empty driver output")?;
    check_hello(first, built, record)?;
    if last["event"] != "done" || last["mode"] != mode {
        return Err("driver completion missing".into());
    }
    let allowed = if mode == "validate" {
        "validation"
    } else if mode == "diagnose" {
        "diagnostic"
    } else {
        "sample"
    };
    for event in &events[1..events.len() - 1] {
        if mode == "describe" || event["event"] != allowed {
            return Err("unexpected driver event".into());
        }
    }
    Ok(())
}
pub(crate) fn check_hello(first: &Value, built: &Built, record: &Record) -> Result<()> {
    if first["event"] != "hello"
        || first["protocol"] != 1
        || first["build"] != built.key
        || first["interface"] != built.interface.id
        || first["test"] != record.recipe.as_ref().ok_or("recipe missing")?.identity.id
        || first["operator"] != built.implementation
        || first["family"] != built.family
        || first["scheme"] != record.request.scheme.name
    {
        return Err("driver identity or protocol mismatch".into());
    }
    Ok(())
}
