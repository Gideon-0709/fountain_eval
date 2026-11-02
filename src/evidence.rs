use crate::{
    build::Built,
    catalog::{self, Identity, Recipe},
    measurement::{Sample, Summary},
    process,
    protocol::EvaluationRequest,
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub digest: String,
    pub conditions: Value,
    pub observations: Value,
}
pub fn environment(cache: &Path) -> Result<Environment> {
    let cpu = if cfg!(target_os = "macos") {
        json!({"model":process::output("sysctl",&["-n","hw.model"] )?,"brand":process::output("sysctl",&["-n","machdep.cpu.brand_string"]).ok(),"memory":process::output("sysctl",&["-n","hw.memsize"] )?,"cpus":process::output("sysctl",&["-n","hw.ncpu"] )?})
    } else if cfg!(target_os = "linux") {
        let text = fs::read_to_string("/proc/cpuinfo").map_err(|e| e.to_string())?;
        let fields: BTreeMap<_, _> = text
            .split("\n\n")
            .next()
            .unwrap_or("")
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(a, b)| (a.trim().to_owned(), b.trim().to_owned()))
            .filter(|(k, _)| {
                [
                    "vendor_id",
                    "model name",
                    "cpu family",
                    "model",
                    "stepping",
                    "flags",
                    "Features",
                    "CPU implementer",
                    "CPU part",
                ]
                .contains(&k.as_str())
            })
            .collect();
        let memory = fs::read_to_string("/proc/meminfo")
            .map_err(|e| e.to_string())?
            .lines()
            .find(|s| s.starts_with("MemTotal:"))
            .unwrap_or("")
            .to_owned();
        json!({"cpu":fields,"memory":memory,"available_cpus":std::thread::available_parallelism().map_err(|e|e.to_string())?.get()})
    } else {
        return Err("unsupported: native execution requires Linux or macOS".into());
    };
    let config = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")));
    let mut cargo_configs = BTreeMap::new();
    if let Some(dir) = config {
        for name in ["config", "config.toml"] {
            if dir.join(name).is_file() {
                cargo_configs.insert(name.to_owned(), util::file_hash(&dir.join(name))?);
            }
        }
    }
    let mut ancestor_configs = Vec::new();
    for root in cache.join("builds").ancestors() {
        for name in ["config", "config.toml"] {
            let path = root.join(".cargo").join(name);
            if path.is_file() {
                ancestor_configs.push(util::file_hash(&path)?);
            }
        }
    }
    let controls: BTreeMap<_, _> = [
        "CC",
        "CXX",
        "CFLAGS",
        "CXXFLAGS",
        "LDFLAGS",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok().map(|v| (*key, v)))
    .collect();
    let power = if cfg!(target_os = "macos") {
        json!({"source":process::output("pmset",&["-g","batt"]).ok().and_then(|v|v.lines().next().map(str::to_owned)),"policy":process::output("pmset",&["-g","custom"]).ok()})
    } else {
        json!({"governor":fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").ok(),"affinity":fs::read_to_string("/proc/self/status").ok().and_then(|v|v.lines().find(|s|s.starts_with("Cpus_allowed_list:")).map(str::to_owned))})
    };
    let conditions = json!({"build":{"rustc":process::output("rustc",&["-vV"] )?,"cargo":process::output("cargo",&["-V"] )?,"cc":process::output("cc",&["--version"] )?,"sdk":if cfg!(target_os="macos"){process::output("xcrun",&["--show-sdk-version"]).ok()}else{None},"profile":"release","opt_level":3,"codegen_units":1,"lto":false,"rustflags":"-C target-cpu=native","cargo_config_hashes":cargo_configs,"ancestor_cargo_configs":ancestor_configs,"compiler_environment":controls},"runtime":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"kernel":process::output("uname",&["-srv"] )?,"cpu":cpu,"test_threads":1,"power_and_affinity":power}});
    let observations = json!({"uptime":process::output("uptime",&[]).ok()});
    Ok(Environment {
        digest: util::digest(&conditions),
        conditions,
        observations,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Quadruple {
    pub c: Value,
    pub i: Identity,
    pub t: Identity,
    pub e: Environment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub format_version: u32,
    pub run_id: String,
    pub status: String,
    pub stage: String,
    pub request: EvaluationRequest,
    pub configuration: crate::config::RunConfiguration,
    pub coverage: Value,
    pub quadruple: Option<Quadruple>,
    pub recipe: Option<Recipe>,
    pub built: Option<Built>,
    pub validation: Vec<Value>,
    pub samples: Vec<Sample>,
    #[serde(default)]
    pub diagnostics: Vec<Value>,
    pub summary: Vec<Summary>,
    pub error: Option<String>,
    pub timing: crate::execution::Timing,
    #[serde(default)]
    pub correctness: crate::validation::Report,
}
pub fn stage_states(record: &Record) -> BTreeMap<&'static str, &str> {
    let phases = [
        "resolve", "build", "probe", "validate", "measure", "diagnose",
    ];
    let completed = match record.stage.as_str() {
        "prepared" => 3,
        "complete" => 6,
        other => phases.iter().position(|v| *v == other).unwrap_or(0),
    };
    phases
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let state = if i < completed {
                "succeeded"
            } else if name == record.stage {
                match record.status.as_str() {
                    "failed" => "failed",
                    "unsupported" => "unsupported",
                    "interrupted" => "interrupted",
                    _ => "running",
                }
            } else {
                "not_executed"
            };
            (name, state)
        })
        .collect()
}
pub fn save(directory: &Path, record: &Record) -> Result<()> {
    util::write_json(&directory.join("stages.json"), &stage_states(record))?;
    util::write_json(&directory.join("request.json"), &record.request)?;
    fs::write(
        directory.join("config.toml"),
        &record.configuration.original,
    )
    .map_err(|e| e.to_string())?;
    util::write_json(
        &directory.join("config-resolved.json"),
        &record.configuration,
    )?;
    util::write_json(
        &directory.join("coverage.json"),
        &crate::coverage::report(record),
    )?;
    util::write_json(&directory.join("test-plan.json"), &record.recipe)?;
    util::write_json(
        &directory.join("evaluator.json"),
        &json!({"package_version":env!("CARGO_PKG_VERSION"),"binary_sha256":std::env::current_exe().ok().and_then(|p|util::file_hash(&p).ok())}),
    )?;
    util::write_json(&directory.join("correctness.json"), &record.correctness)?;
    util::write_json(&directory.join("result.json"), record)
}
pub fn seal(directory: &Path) -> Result<()> {
    let mut hashes = BTreeMap::new();
    for rel in util::files(directory)? {
        if rel != Path::new("seal.json") && rel.extension().is_none_or(|s| s != "tmp") {
            hashes.insert(rel.clone(), util::file_hash(&directory.join(&rel))?);
        }
    }
    util::write_json(&directory.join("seal.json"), &hashes)
}
pub fn verify_seal(directory: &Path, required: &str) -> Result<()> {
    let hashes: BTreeMap<PathBuf, String> = util::read_json(&directory.join("seal.json"))?;
    if !hashes.contains_key(Path::new(required)) {
        return Err("result is not sealed".into());
    }
    for (path, expected) in hashes {
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("invalid evidence path".into());
        }
        if util::file_hash(&directory.join(path))? != expected {
            return Err("evidence checksum mismatch".into());
        }
    }
    Ok(())
}
pub fn load(directory: &Path) -> Result<Record> {
    verify_seal(directory, "result.json")?;
    let mut record: Record = util::read_json(&directory.join("result.json"))?;
    record.correctness = crate::validation::report(&record);
    record.coverage = crate::coverage::report(&record);
    Ok(record)
}
pub fn compare(left: &Record, right: &Record) -> Result<Value> {
    catalog::verify_lock()?;
    if left.status != "succeeded" || right.status != "succeeded" {
        return Err("not comparable: both runs must have succeeded".into());
    }
    let a = left.quadruple.as_ref().ok_or("missing tuple")?;
    let b = right.quadruple.as_ref().ok_or("missing tuple")?;
    if a.t != b.t {
        return Err("not comparable: test version or content differs".into());
    }
    if a.e.digest != b.e.digest || a.e.conditions != b.e.conditions {
        return Err("not comparable: effective experiment conditions differ".into());
    }
    for record in [left, right] {
        let q = record.quadruple.as_ref().unwrap();
        let recipe = catalog::recipe(&record.request.test)?;
        if q.t != recipe.identity {
            return Err("test identity collision or different catalog candidate".into());
        }
        if !catalog::registered_interface(&q.i, &recipe.identity.id) {
            return Err("interface identity collision or unavailable interface".into());
        }
        validate_samples(record)?;
    }
    let left_recipe = left.recipe.as_ref().unwrap();
    let right_recipe = right.recipe.as_ref().unwrap();
    for (case, check) in &left_recipe.checks {
        if check.skip.is_none()
            && check.severity == crate::validation::Severity::Fatal
            && right_recipe.check(case).is_some_and(|c| c.skip.is_none())
        {
            let witness = |r: &Record| {
                r.validation
                    .iter()
                    .find(|v| v["case"] == *case)
                    .map(|v| v["witness"].clone())
            };
            if witness(left) != witness(right) {
                return Err("not comparable: common fatal-check witnesses differ".into());
            }
        }
    }
    let a_summary = crate::measurement::summarize(&left.samples)?;
    let b_summary = crate::measurement::summarize(&right.samples)?;
    let left_correctness = crate::validation::report(left);
    let right_correctness = crate::validation::report(right);
    let rows:Vec<_>=a_summary.iter().filter_map(|a|b_summary.iter().find(|b|b.case==a.case).map(|b|(a,b))).map(|(a,b)|json!({"case":a.case,"left_median_ns":a.median_ns,"right_median_ns":b.median_ns,"left_over_right_time":a.median_ns as f64/b.median_ns as f64,"left_cv":a.coefficient_of_variation,"right_cv":b.coefficient_of_variation,"left_worker_medians_ns":a.worker_medians_ns,"right_worker_medians_ns":b.worker_medians_ns,"left_completed":a.completed,"right_completed":b.completed,"left_operations_per_second":a.operations_per_second,"right_operations_per_second":b.operations_per_second,"left_source_payload_gbps":a.source_payload_gbps,"right_source_payload_gbps":b.source_payload_gbps,"left_warnings":left_correctness.warnings.iter().filter(|w|w["case"]==a.case).collect::<Vec<_>>(),"right_warnings":right_correctness.warnings.iter().filter(|w|w["case"]==b.case).collect::<Vec<_>>()})).collect();
    if rows.is_empty() {
        return Err("not comparable: no common measured cases".into());
    }
    Ok(
        json!({"status":"comparable","comparison_scope":"common measured cases only","coverage":{"left":crate::coverage::report(left),"right":crate::coverage::report(right)},"purpose":"formal","left":left.run_id,"right":right.run_id,"interface_equivalence":{"contract":a.t,"evidence":"registered timing contract and matching fatal-check witnesses; warning outcomes may differ","interfaces":[a.i,b.i]},"attribution":if a.i!=b.i&&a.c!=b.c{"component and interface path together"}else if a.i!=b.i{"interface path"}else if a.c["dependencies"]["support_packages"]!=b.c["dependencies"]["support_packages"]{"component combination including support dependency changes"}else{"selected component combination"},"correctness":{"left":left_correctness,"right":right_correctness},"warning":"Interpret timings with dispersion, semantic warnings, work counters and diagnostic overhead; no performance pass threshold.","rows":rows}),
    )
}
pub fn validate_samples(record: &Record) -> Result<()> {
    let recipe = record.recipe.as_ref().ok_or("missing recipe")?;
    let family = record
        .quadruple
        .as_ref()
        .map(|q| q.i.id.split('@').next().unwrap_or(""))
        .or_else(|| record.built.as_ref().map(|b| b.family.as_str()))
        .ok_or("missing interface identity")?;
    let canonical = catalog::plan(
        &record.request.test,
        &record.request.scheme,
        family,
        record
            .request
            .components
            .scope()
            .map_err(|e| e.to_string())?,
    )?;
    if serde_json::to_value(recipe).map_err(|e| e.to_string())?
        != serde_json::to_value(&canonical).map_err(|e| e.to_string())?
    {
        return Err("recipe expansion differs from its immutable test content".into());
    }
    if let Some(tuple) = &record.quadruple
        && tuple.e.digest != util::digest(&tuple.e.conditions)
    {
        return Err("environment identity mismatch".into());
    }
    let mut cases = BTreeMap::<String, BTreeMap<usize, &Sample>>::new();
    for sample in &record.samples {
        let check = recipe
            .check(&sample.case)
            .ok_or("unregistered sample case")?;
        if check.skip.is_some()
            || !check.measured
            || sample.payload_bytes != check.payload_bytes
            || sample.operations
                != if check.spec.workload.ends_with("-replay") {
                    record
                        .validation
                        .iter()
                        .find(|v| v["case"] == sample.case)
                        .and_then(|v| v["details"]["calls"].as_u64())
                        .ok_or("recorded replay call count missing")?
                } else {
                    check.operations
                }
        {
            return Err("sample work counter does not match the test recipe".into());
        }
        let expected_worker = if check.spec.fresh_process {
            sample.repetition
        } else {
            sample.repetition / crate::content::SAMPLES_PER_WORKER
        };
        if sample.worker != expected_worker
            || !matches!(sample.outcome.as_str(), "completed" | "incomplete_recovery")
            || sample.elapsed_ns == 0
        {
            return Err("sample worker, outcome or clock violates the recipe".into());
        }
        if sample.outcome == "incomplete_recovery"
            && !matches!(
                check.spec.workload.as_str(),
                "decode" | "round-trip" | "decode-chain"
            )
        {
            return Err("incomplete outcome is only defined for bounded recovery workloads".into());
        }
        if !recipe.cases.contains(&sample.case)
            || sample.repetition >= recipe.repetitions
            || cases
                .entry(sample.case.clone())
                .or_default()
                .insert(sample.repetition, sample)
                .is_some()
        {
            return Err("unexpected or duplicate sample".into());
        }
    }
    crate::validation::verify(recipe, &record.validation)?;
    let validation: BTreeMap<_, _> = record
        .validation
        .iter()
        .map(|v| (v["case"].as_str().unwrap(), v))
        .collect();
    for case in &recipe.cases {
        let values = cases.get(case).ok_or("missing sample case")?;
        if values.len() != recipe.repetitions {
            return Err("incomplete samples".into());
        }
        let event = validation
            .get(case.as_str())
            .ok_or("missing validation case")?;
        for sample in values.values() {
            if recipe.check(case).unwrap().severity == crate::validation::Severity::Fatal
                && sample.witness
                    != event["witness"]
                        .as_str()
                        .ok_or("validation witness missing")?
            {
                return Err("measured result differs from fatal correctness validation".into());
            }
            if recipe.check(case).unwrap().severity == crate::validation::Severity::Warning {
                let actual = sample
                    .observation
                    .as_ref()
                    .ok_or("missing measured semantic observation")?;
                crate::validation::verify_observation(actual, sample.operations)?;
                if sample.witness != util::digest(actual) {
                    return Err("measured observation checksum mismatch".into());
                }
            }
        }
    }
    validate_diagnostics(record)?;
    crate::measurement::summarize(&record.samples)?;
    Ok(())
}

/// Instrumented observations are complete, independently identified and internally additive.
pub fn validate_diagnostics(record: &Record) -> Result<()> {
    let recipe = record.recipe.as_ref().ok_or("missing recipe")?;
    let expected: std::collections::BTreeSet<_> = recipe
        .checks
        .iter()
        .filter(|(_, c)| {
            c.skip.is_none()
                && c.measured
                && !c.spec.fresh_process
                && c.spec.category != "operation"
        })
        .flat_map(|(id, _)| (0..crate::content::WORKERS).map(move |w| (id.as_str(), w)))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for event in &record.diagnostics {
        let case = event["case"].as_str().ok_or("diagnostic case missing")?;
        let worker = event["worker"]
            .as_u64()
            .ok_or("diagnostic worker missing")? as usize;
        if !expected.contains(&(case, worker))
            || !seen.insert((case, worker))
            || event["elapsed_ns"].as_u64().is_none_or(|n| n == 0)
        {
            return Err("unexpected, duplicate or invalid diagnostic observation".into());
        }
        let stages = &event["observation"]["stages"];
        if !stages.is_null() {
            let total = stages["total_ns"]
                .as_u64()
                .ok_or("diagnostic total missing")?;
            let residual = stages["unattributed_ns"]
                .as_u64()
                .ok_or("diagnostic residual missing")?;
            let mut sum = residual;
            for value in stages["exclusive_ns"]
                .as_object()
                .ok_or("diagnostic exclusive stages missing")?
                .values()
            {
                sum = sum
                    .checked_add(value.as_u64().ok_or("noninteger stage duration")?)
                    .ok_or("stage duration overflow")?;
            }
            if sum != total {
                return Err(
                    "exclusive stages and residual do not equal the parent duration".into(),
                );
            }
        }
        if event["resources"]["scope"].as_str().is_none() {
            return Err("diagnostic resource scope missing".into());
        }
    }
    if seen != expected {
        return Err("incomplete diagnostic evidence".into());
    }
    Ok(())
}
