use crate::{
    build::{self, Built},
    catalog,
    evidence::{self, Quadruple, Record},
    process,
    protocol::EvaluationRequest,
    source::Resolver,
    util::{self, Result},
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub struct Options {
    pub cache: PathBuf,
    pub output: PathBuf,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            cache: ".fountain-eval".into(),
            output: "runs".into(),
        }
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
    loop {
        match fs2::FileExt::try_lock_exclusive(&f) {
            Ok(()) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if process::interrupted() {
                    return Err("interrupted waiting for cache lock".into());
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
pub fn start(
    request: EvaluationRequest,
    options: &Options,
    prepare_only: bool,
) -> Result<(PathBuf, Record)> {
    process::install_handler()?;
    fs::create_dir_all(&options.cache).map_err(|e| e.to_string())?;
    let cache = options.cache.canonicalize().map_err(|e| e.to_string())?;
    let (id, directory) = new_directory(&options.output)?;
    let mut record = Record {
        format_version: 1,
        run_id: id,
        status: "running".into(),
        stage: "queued".into(),
        request,
        quadruple: None,
        recipe: None,
        built: None,
        validation: Vec::new(),
        samples: Vec::new(),
        summary: Vec::new(),
        error: None,
    };
    evidence::save(&directory, &record)?;
    let result = util::catch(|| {
        let _lock = lock(&cache)?;
        record.stage = "resolve".into();
        evidence::save(&directory, &record)?;
        catalog::verify_lock()?;
        let recipe = catalog::recipe(&record.request.test)?;
        record.recipe = Some(recipe.clone());
        let mut resolver = Resolver::new(&cache, &directory.join("logs"));
        let engine = resolver.resolve(&record.request.components.engine)?;
        let operator = resolver.resolve(&record.request.components.operator)?;
        util::write_json(
            &directory.join("sources.json"),
            &json!({"engine":engine,"operator":operator}),
        )?;
        let implementation = catalog::implementation(
            &operator,
            record.request.components.operator.implementation.as_deref(),
        )?;
        if record.request.components.engine.implementation.is_some() {
            return Err("unsupported: engine implementation selectors are not registered".into());
        }
        let mut environment = evidence::environment(&cache)?;
        environment.conditions["recipe"] = recipe.definition.clone();
        environment.digest = util::digest(&environment.conditions);
        util::write_json(&directory.join("configuration.json"), &environment)?;
        record.stage = "build".into();
        evidence::save(&directory, &record)?;
        let built = build::prepare(
            &mut resolver,
            &engine,
            &operator,
            &implementation,
            &recipe,
            &environment.conditions,
        )?;
        util::write_json(&directory.join("build.json"), &built)?;
        record.built = Some(built);
        record.stage = "probe".into();
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
            execute(&directory, &mut record)?;
        }
        Ok(())
    });
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
    let mut record = old.clone();
    record.run_id = id;
    record.status = "running".into();
    record.stage = "probe".into();
    record.error = None;
    let result = util::catch(|| {
        catalog::verify_lock()?;
        let recipe = catalog::recipe(&record.request.test)?;
        let tuple = record.quadruple.as_mut().ok_or("missing prepared tuple")?;
        if recipe.identity != tuple.t || catalog::interface(&built.family) != tuple.i {
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
        current.digest = util::digest(&current.conditions);
        tuple.e = current;
        util::write_json(&directory.join("configuration.json"), &tuple.e)?;
        util::write_json(
            &directory.join("prepared-from.json"),
            &json!({"run_id":old.run_id,"seal":util::file_hash(&prepared.join("seal.json"))?}),
        )?;
        util::write_json(&directory.join("build.json"), built)?;
        invoke(&directory, &record, "describe")?;
        execute(&directory, &mut record)
    });
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
    evidence::save(directory, record)?;
    evidence::seal(directory)
}
fn execute(directory: &Path, record: &mut Record) -> Result<()> {
    record.stage = "validate".into();
    evidence::save(directory, record)?;
    let events = invoke(directory, record, "validate")?;
    record.validation = events
        .into_iter()
        .filter(|v| v["event"] == "validation")
        .collect();
    let recipe = record.recipe.as_ref().ok_or("missing recipe")?;
    let mut cases = record
        .validation
        .iter()
        .filter_map(|v| v["case"].as_str())
        .collect::<Vec<_>>();
    cases.sort();
    let mut expected = recipe.cases.iter().map(String::as_str).collect::<Vec<_>>();
    expected.sort();
    if cases != expected {
        return Err("correctness validation did not cover the requested cases".into());
    }
    util::write_json(&directory.join("validation.json"), &record.validation)?;
    record.stage = "measure".into();
    evidence::save(directory, record)?;
    let outcome = invoke(directory, record, "measure");
    // Preserve every valid sample prefix even if the driver later fails.
    let raw = fs::read_to_string(directory.join("logs/measure.stdout")).unwrap_or_default();
    for line in raw.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line)
            && value["event"] == "sample"
            && let Ok(sample) = serde_json::from_value(value)
        {
            record.samples.push(sample);
        }
    }
    util::write_json(&directory.join("samples.json"), &record.samples)?;
    outcome?;
    evidence::validate_samples(record)?;
    record.summary = crate::measurement::summarize(&record.samples)?;
    record.stage = "complete".into();
    record.status = "succeeded".into();
    Ok(())
}
fn invoke(directory: &Path, record: &Record, mode: &str) -> Result<Vec<Value>> {
    let built = record.built.as_ref().ok_or("build missing")?;
    built.verify()?;
    if mode == "describe" {
        built.export_evidence(directory)?;
    }
    let mut command = Command::new(&built.binary);
    command.arg(mode).current_dir(&built.directory);
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
    let raw = process::run(
        &mut command,
        &directory.join("logs"),
        mode,
        Duration::from_secs(60),
    )?;
    let events: Vec<Value> = raw
        .lines()
        .map(|s| serde_json::from_str(s).map_err(|e| format!("driver protocol: {e}")))
        .collect::<Result<_>>()?;
    check_events(&events, built, record, mode)?;
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
    if first["event"] != "hello"
        || first["protocol"] != 1
        || first["build"] != built.key
        || first["interface"] != built.interface.id
        || first["test"] != record.recipe.as_ref().ok_or("recipe missing")?.identity.id
        || first["operator"] != built.implementation
        || first["family"] != built.family
    {
        return Err("driver identity or protocol mismatch".into());
    }
    if last["event"] != "done" || last["mode"] != mode {
        return Err("driver completion missing".into());
    }
    let allowed = if mode == "validate" {
        "validation"
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
