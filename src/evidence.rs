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
    pub quadruple: Option<Quadruple>,
    pub recipe: Option<Recipe>,
    pub built: Option<Built>,
    pub validation: Vec<Value>,
    pub samples: Vec<Sample>,
    pub summary: Vec<Summary>,
    pub error: Option<String>,
}
pub fn stage_states(record: &Record) -> BTreeMap<&'static str, &str> {
    let phases = ["resolve", "build", "probe", "validate", "measure"];
    let completed = match record.stage.as_str() {
        "prepared" => 3,
        "complete" => 5,
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
    util::write_json(
        &directory.join("evaluator.json"),
        &json!({"package_version":env!("CARGO_PKG_VERSION"),"binary_sha256":std::env::current_exe().ok().and_then(|p|util::file_hash(&p).ok())}),
    )?;
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
pub fn load(directory: &Path) -> Result<Record> {
    let hashes: BTreeMap<PathBuf, String> = util::read_json(&directory.join("seal.json"))?;
    if !hashes.contains_key(Path::new("result.json")) {
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
    util::read_json(&directory.join("result.json"))
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
            return Err("test identity collision or unavailable historical content".into());
        }
        let family = q.i.id.split('@').next().unwrap_or("");
        if !catalog::supports(family, &recipe.identity.id) || q.i != catalog::interface(family) {
            return Err("interface identity collision or unavailable interface".into());
        }
        validate_samples(record)?;
    }
    let witnesses = |r: &Record| {
        r.validation
            .iter()
            .map(|v| {
                (
                    v["case"].as_str().unwrap_or("").to_owned(),
                    v["witness"].clone(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    if witnesses(left) != witnesses(right) {
        return Err("not comparable: acceptance validation witnesses differ".into());
    }
    let a_summary = crate::measurement::summarize(&left.samples)?;
    let b_summary = crate::measurement::summarize(&right.samples)?;
    let rows:Vec<_>=a_summary.iter().zip(&b_summary).map(|(a,b)|json!({"case":a.case,"left_median_ns":a.median_ns,"right_median_ns":b.median_ns,"left_over_right_time":a.median_ns as f64/b.median_ns as f64})).collect();
    Ok(
        json!({"status":"comparable","purpose":"acceptance","left":left.run_id,"right":right.run_id,"interface_equivalence":{"contract":a.t,"evidence":"registered timing contract and matching independently checked acceptance witnesses","interfaces":[a.i,b.i]},"attribution":if a.i!=b.i&&a.c!=b.c{"component and interface path together"}else if a.i!=b.i{"interface path"}else if a.c["dependencies"]["support_packages"]!=b.c["dependencies"]["support_packages"]{"component combination including support dependency changes"}else{"selected component combination"},"warning":"Acceptance timings are not a formal performance standard or an optimization conclusion.","rows":rows}),
    )
}
pub fn validate_samples(record: &Record) -> Result<()> {
    let recipe = record.recipe.as_ref().ok_or("missing recipe")?;
    let canonical = catalog::recipe(&record.request.test)?;
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
        let (bytes, operations) = if sample.case.starts_with("operator/") {
            (0, 14)
        } else {
            (64 * 128, 0)
        };
        if sample.payload_bytes != bytes || sample.operations != operations {
            return Err("sample work counter does not match the test recipe".into());
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
    let mut validation = BTreeMap::new();
    for event in &record.validation {
        let case = event["case"].as_str().ok_or("validation case missing")?;
        if validation.insert(case, event).is_some() {
            return Err("duplicate validation case".into());
        }
    }
    if validation.len() != recipe.cases.len() {
        return Err("incomplete correctness evidence".into());
    }
    for case in &recipe.cases {
        let values = cases.get(case).ok_or("missing sample case")?;
        if values.len() != recipe.repetitions {
            return Err("incomplete samples".into());
        }
        let event = validation
            .get(case.as_str())
            .ok_or("missing validation case")?;
        for sample in values.values() {
            if sample.witness
                != event["witness"]
                    .as_str()
                    .ok_or("validation witness missing")?
            {
                return Err("measured result differs from correctness validation".into());
            }
        }
    }
    crate::measurement::summarize(&record.samples)?;
    Ok(())
}
