//! Generated build views bind sources without editing upstream worktrees.
use crate::{
    catalog::{self, Identity, Recipe},
    process,
    source::{Package, Resolver, package_at, workspace_root},
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Built {
    pub key: String,
    pub directory: PathBuf,
    pub binary: PathBuf,
    pub artifacts: BTreeMap<PathBuf, String>,
    pub interface: Identity,
    pub family: String,
    pub implementation: String,
    pub components: Value,
    pub dependency_graph: Value,
    pub bindings: Value,
    pub product_library: Option<PathBuf>,
}
impl Built {
    pub fn export_evidence(&self, directory: &Path) -> Result<()> {
        let mut locks = BTreeMap::new();
        for rel in self.artifacts.keys() {
            if rel.file_name().is_some_and(|v| v == "Cargo.lock") {
                locks.insert(
                    rel.clone(),
                    fs::read_to_string(self.directory.join(rel)).map_err(|e| e.to_string())?,
                );
            }
            if rel.starts_with("provenance/logs") {
                let dest = directory.join("build-evidence").join(rel);
                fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::copy(self.directory.join(rel), dest).map_err(|e| e.to_string())?;
            }
        }
        util::write_json(&directory.join("dependency-locks.json"), &locks)?;
        util::write_json(&directory.join("bindings.json"), &self.bindings)?;
        util::write_json(
            &directory.join("dependency-graphs.json"),
            &self.dependency_graph,
        )
    }
    pub fn verify(&self) -> Result<()> {
        if self.directory.join("build.json").is_file()
            && util::file_hash(&self.directory.join("build.json"))?
                != fs::read_to_string(self.directory.join("build.sha256"))
                    .map_err(|e| format!("cache index: {e}"))?
        {
            return Err("build cache metadata checksum mismatch".into());
        }
        if !self.binary.starts_with(&self.directory)
            || !self.artifacts.contains_key(
                self.binary
                    .strip_prefix(&self.directory)
                    .map_err(|e| e.to_string())?,
            )
        {
            return Err("unindexed driver binary".into());
        }
        for (p, expected) in &self.artifacts {
            if p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("invalid artifact path".into());
            }
            if util::file_hash(&self.directory.join(p))? != *expected {
                return Err(format!("artifact checksum mismatch: {}", p.display()));
            }
        }
        Ok(())
    }
}

struct Composer<'a> {
    resolver: &'a mut Resolver,
    root: PathBuf,
    overrides: BTreeMap<String, Package>,
    seen: BTreeSet<PathBuf>,
    copied: BTreeSet<String>,
    bindings: Vec<Value>,
}
impl Composer<'_> {
    fn add(&mut self, package: &Package) -> Result<PathBuf> {
        let destination = self.root.join("sources").join(&package.source.digest);
        if self.copied.insert(package.source.digest.clone()) {
            if util::tree_hash(&package.source.root)? != package.source.digest {
                return Err("source cache checksum mismatch".into());
            }
            for rel in util::files(&package.source.root)? {
                let target = destination.join(&rel);
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::copy(package.source.root.join(rel), target).map_err(|e| e.to_string())?;
            }
        }
        let output = destination.join(&package.manifest);
        if !self.seen.insert(output.clone()) {
            return Ok(output.parent().unwrap().into());
        }
        let original = util::read_toml(&package.path())?;
        let mut effective = original.clone();
        let workspace =
            workspace_root(&package.path()).filter(|p| p.starts_with(&package.source.root));
        let workspace_value = workspace
            .as_ref()
            .map(|p| util::read_toml(&p.join("Cargo.toml")))
            .transpose()?
            .unwrap_or(toml::Value::Table(Default::default()));
        if let Some(table) = effective
            .get_mut("package")
            .and_then(toml::Value::as_table_mut)
        {
            for (key, value) in table.iter_mut() {
                if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                    *value = workspace_value
                        .get("workspace")
                        .and_then(|v| v.get("package"))
                        .and_then(|v| v.get(key))
                        .ok_or_else(|| format!("missing inherited package.{key}"))?
                        .clone();
                    if matches!(key.as_str(), "readme" | "license-file")
                        && let Some(s) = value.as_str()
                    {
                        let p = workspace.as_ref().unwrap().join(s);
                        let rel = p
                            .strip_prefix(&package.source.root)
                            .map_err(|e| e.to_string())?;
                        *value =
                            toml::Value::String(destination.join(rel).to_string_lossy().into());
                    }
                }
            }
            table.remove("workspace");
        }
        if effective
            .get("lints")
            .and_then(|v| v.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true)
        {
            let inherited_lints = workspace_value
                .get("workspace")
                .and_then(|v| v.get("lints"))
                .ok_or("missing workspace.lints")?
                .clone();
            util::toml_set(&mut effective, "lints", inherited_lints);
        }
        let mut groups = vec![Vec::<String>::new()];
        if let Some(targets) = effective.get("target").and_then(toml::Value::as_table) {
            groups.extend(
                targets
                    .keys()
                    .map(|name| vec!["target".into(), name.clone()]),
            );
        }
        for group in groups {
            let mut container = &mut effective;
            for key in &group {
                container = &mut container[key];
            }
            container.as_table_mut().unwrap().remove("dev-dependencies");
            for section in ["dependencies", "build-dependencies"] {
                let Some(dependencies) = container
                    .get_mut(section)
                    .and_then(toml::Value::as_table_mut)
                else {
                    continue;
                };
                for (alias, decl) in dependencies.iter_mut() {
                    let mut base = package.path().parent().unwrap().to_owned();
                    if decl.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                        let local = decl.clone();
                        *decl = workspace_value
                            .get("workspace")
                            .and_then(|v| v.get("dependencies"))
                            .and_then(|v| v.get(alias))
                            .ok_or_else(|| format!("missing inherited dependency {alias}"))?
                            .clone();
                        if decl.is_str() {
                            *decl = toml::Value::Table(toml::map::Map::from_iter([(
                                "version".into(),
                                decl.clone(),
                            )]));
                        }
                        if let Some(features) =
                            local.get("features").and_then(toml::Value::as_array)
                        {
                            let slot = decl
                                .as_table_mut()
                                .unwrap()
                                .entry("features")
                                .or_insert(toml::Value::Array(Vec::new()));
                            slot.as_array_mut()
                                .ok_or("invalid inherited features")?
                                .extend(features.clone());
                        }
                        for key in ["optional", "default-features"] {
                            if let Some(v) = local.get(key) {
                                util::toml_set(decl, key, v.clone());
                            }
                        }
                        base = workspace
                            .clone()
                            .ok_or("dependency inheritance requires workspace")?;
                    }
                    let name = decl
                        .get("package")
                        .and_then(toml::Value::as_str)
                        .unwrap_or(alias)
                        .to_owned();
                    let replacement = if let Some(p) = self.overrides.get(&name) {
                        Some(p.clone())
                    } else if let Some(relative) = decl.get("path").and_then(toml::Value::as_str) {
                        Some(self.resolver.path_dependency(package, &base, relative)?)
                    } else {
                        None
                    };
                    if let Some(replacement) = replacement {
                        let requirement = decl
                            .as_str()
                            .or_else(|| decl.get("version").and_then(toml::Value::as_str));
                        if let Some(requirement) = requirement
                            && !semver::VersionReq::parse(requirement)
                                .map_err(|e| e.to_string())?
                                .matches(
                                    &semver::Version::parse(&replacement.version)
                                        .map_err(|e| e.to_string())?,
                                )
                        {
                            return Err(format!(
                                "dependency conflict: {} requires {name} {requirement}; selected {}",
                                package.name, replacement.version
                            ));
                        }
                        if replacement.name != name {
                            return Err(format!("path dependency package mismatch: {name}"));
                        }
                        let path = self.add(&replacement)?;
                        if decl.is_str() {
                            *decl = toml::Value::Table(toml::map::Map::from_iter([(
                                "version".into(),
                                decl.clone(),
                            )]));
                        }
                        let table = decl
                            .as_table_mut()
                            .ok_or("invalid dependency declaration")?;
                        for key in ["git", "branch", "tag", "rev", "registry", "registry-index"] {
                            table.remove(key);
                        }
                        table.insert(
                            "path".into(),
                            toml::Value::String(path.to_string_lossy().into()),
                        );
                    }
                }
            }
        }
        if workspace_value.get("patch").is_some() || workspace_value.get("replace").is_some() {
            return Err(
                "unsupported: workspace patches require an explicit integration mapping".into(),
            );
        }
        effective.as_table_mut().unwrap().remove("workspace");
        util::toml_set(
            &mut effective,
            "workspace",
            toml::Value::Table(Default::default()),
        );
        util::toml_set(&mut effective, "profile", profile());
        self.bindings.push(json!({"package":package.name,"source":package.source.digest,"manifest":package.manifest,"original":original,"effective":effective}));
        util::write_toml(&output, &effective)?;
        Ok(output.parent().unwrap().into())
    }
}
fn profile() -> toml::Value {
    toml::Value::try_from(json!({"release":{"opt-level":3,"codegen-units":1,"lto":false,"debug":false,"incremental":false}})).unwrap()
}
fn sibling(package: &Package, name: &str) -> Result<Package> {
    package_at(
        package.source.clone(),
        PathBuf::from(name).join("Cargo.toml"),
    )
}
fn cargo(root: &Path, logs: &Path, label: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new("cargo");
    command.args(args).current_dir(root);
    process::controlled(&mut command);
    process::run(&mut command, logs, label, Duration::from_secs(1800))
}

pub fn prepare(
    resolver: &mut Resolver,
    engine: &Package,
    operator: &Package,
    implementation: &str,
    recipe: &Recipe,
    environment: &Value,
) -> Result<Built> {
    let family = catalog::family(engine)?;
    if family == "product-abi" && recipe.identity.id.starts_with("operator-smoke") {
        return Err(
            "unsupported: product API does not expose the complete operator operation surface"
                .into(),
        );
    }
    let interface = catalog::interface(&family);
    let mut overrides = BTreeMap::new();
    if family != "product-abi" {
        overrides.insert("fountain_engine".into(), engine.clone());
    }
    let (scheme, utility) = if family == "native-2" {
        (
            Some(resolver.registry("fountain_scheme", "2.1.0")?),
            Some(resolver.registry("fountain_utility", "2.1.0")?),
        )
    } else if family == "native-3" {
        (
            Some(sibling(engine, "fountain_scheme")?),
            Some(sibling(engine, "fountain_utility")?),
        )
    } else {
        (None, None)
    };
    if let Some(p) = &scheme {
        overrides.insert(p.name.clone(), p.clone());
    }
    if let Some(p) = &utility {
        overrides.insert(p.name.clone(), p.clone());
    }
    let mut initial = vec![engine.clone(), operator.clone()];
    initial.extend(overrides.values().cloned());
    if family == "product-abi" {
        for name in ["product/fountain_raptor_q", "product/fountain_raptor_10"] {
            initial.push(sibling(engine, name)?);
        }
    }
    resolver.capture_paths(&initial, &overrides)?;
    let input_sources = resolver.identities();
    let key = util::digest(&(
        engine.identity(),
        operator.identity(),
        implementation,
        &interface,
        &recipe.identity,
        environment,
        &input_sources,
    ));
    let directory = resolver.cache.join("builds").join(&key);
    if directory.join("build.json").exists() {
        let built: Built = util::read_json(&directory.join("build.json"))?;
        if built.key != key {
            return Err("build cache identity mismatch".into());
        }
        built.verify()?;
        return Ok(built);
    }
    if directory.exists() {
        return Err(format!(
            "incomplete build retained at {}; use a new cache directory or remove this failed build explicitly",
            directory.display()
        ));
    }
    fs::create_dir_all(directory.join("src")).map_err(|e| e.to_string())?;
    let logs = resolver.logs.clone();
    let mut composer = Composer {
        resolver,
        root: directory.clone(),
        overrides,
        seen: BTreeSet::new(),
        copied: BTreeSet::new(),
        bindings: Vec::new(),
    };
    let engine_dir = composer.add(engine)?;
    let operator_dir = composer.add(operator)?;
    let mut dependencies = toml::map::Map::new();
    let mut add = |name: &str, package: &str, path: &Path, features: Vec<&str>| {
        dependencies.insert(
            name.into(),
            toml::Value::try_from(
                json!({"package":package,"path":path,"default-features":false,"features":features}),
            )
            .unwrap(),
        );
    };
    add(
        "tested_operator",
        &operator.name,
        &operator_dir,
        if operator.name == "fountain_operators" {
            vec!["simd"]
        } else {
            vec![]
        },
    );
    let mut product_library = None;
    let mut product_graph = None;
    if family == "product-abi" {
        cargo(&engine_dir, &logs, "product-lock", &["generate-lockfile"])?;
        cargo(
            &engine_dir,
            &logs,
            "product-build",
            &["build", "--release", "--locked", "--lib"],
        )?;
        product_graph = Some(
            serde_json::from_str::<Value>(&cargo(
                &engine_dir,
                &logs,
                "product-metadata",
                &["metadata", "--format-version", "1", "--locked"],
            )?)
            .map_err(|e| e.to_string())?,
        );
        let filename = if cfg!(target_os = "macos") {
            "libfountain_raptor_q_bin.dylib"
        } else {
            "libfountain_raptor_q_bin.so"
        };
        let lib_dir = directory.join("lib");
        fs::create_dir_all(&lib_dir).map_err(|e| e.to_string())?;
        let library = lib_dir.join(filename);
        fs::copy(engine_dir.join("target/release").join(filename), &library)
            .map_err(|e| format!("product library: {e}"))?;
        #[cfg(target_os = "linux")]
        std::os::unix::fs::symlink(filename, lib_dir.join("libfountain_raptor_q_bin.so.1"))
            .map_err(|e| e.to_string())?;
        product_library = Some(library);
        for name in ["fountain_raptor_q", "fountain_raptor_10"] {
            let package = sibling(engine, &format!("product/{name}"))?;
            let path = composer.add(&package)?;
            add(name, name, &path, vec![]);
        }
        // Callback-side engine identity belongs to the operator dependency graph.
        let op_manifest = util::read_toml(&operator_dir.join("Cargo.toml"))?;
        let dependency = op_manifest["dependencies"]
            .as_table()
            .ok_or("operator dependencies missing")?
            .iter()
            .find(|(key, value)| {
                value
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(key)
                    == "fountain_engine"
            })
            .ok_or("operator engine dependency missing")?
            .1;
        if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) {
            add(
                "fountain_engine",
                "fountain_engine",
                Path::new(path),
                vec![],
            );
        } else {
            let p = composer.resolver.registry("fountain_engine", "2.1.0")?;
            let path = composer.add(&p)?;
            add("fountain_engine", "fountain_engine", &path, vec![]);
        }
    } else {
        add("fountain_engine", "fountain_engine", &engine_dir, vec![]);
        let scheme = scheme.as_ref().unwrap();
        let path = composer.add(scheme)?;
        add("fountain_scheme", "fountain_scheme", &path, vec![]);
        let utility = utility.as_ref().unwrap();
        let path = composer.add(utility)?;
        add("fountain_utility", "fountain_utility", &path, vec![]);
    }
    dependencies.insert("serde_json".into(), toml::Value::String("=1.0.154".into()));
    dependencies.insert("sha2".into(), toml::Value::String("=0.10.9".into()));
    let mut manifest=toml::Value::try_from(json!({"package":{"name":"fountain-eval-driver","version":"0.0.0","edition":"2024","rust-version":"1.89","publish":false},"workspace":{"resolver":"3"}})).unwrap();
    util::toml_set(
        &mut manifest,
        "dependencies",
        toml::Value::Table(dependencies),
    );
    util::toml_set(&mut manifest, "profile", profile());
    let excluded: Vec<_> = composer
        .seen
        .iter()
        .map(|p| {
            p.parent()
                .unwrap()
                .strip_prefix(&directory)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    util::toml_set(
        &mut manifest["workspace"],
        "exclude",
        toml::Value::try_from(excluded).unwrap(),
    );
    // Unify registry references to the selected public engine, including product callbacks.
    if let Some(path) = manifest["dependencies"]["fountain_engine"]
        .get("path")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
    {
        let version = util::read_toml(&Path::new(&path).join("Cargo.toml"))?["package"]["version"]
            .as_str()
            .unwrap()
            .to_owned();
        if version.starts_with("2.1.") {
            let patch =
                toml::Value::try_from(json!({"crates-io":{"fountain_engine":{"path":path}}}))
                    .unwrap();
            util::toml_set(&mut manifest, "patch", patch);
        }
    }
    util::write_toml(&directory.join("Cargo.toml"), &manifest)?;
    let selected = match implementation {
        "arena" => "pub type Selected = tested_operator::MemoryDataOperator;",
        "slab" => "pub type Selected = tested_operator::SlabDataOperator;",
        "simd" => "pub type Selected = tested_operator::SimdDataOperator;",
        _ => return Err("unknown implementation".into()),
    };
    for (name, contents) in [
        ("main.rs", catalog::COMMON),
        ("operator.rs", catalog::OPERATOR),
        (
            "api.rs",
            if family == "product-abi" {
                catalog::PRODUCT
            } else {
                catalog::NATIVE
            },
        ),
        ("selected.rs", selected),
    ] {
        fs::write(directory.join("src").join(name), contents).map_err(|e| e.to_string())?;
    }
    fs::write(directory.join("src/config.rs"),format!("pub const BUILD:&str={key:?};\npub const INTERFACE:&str={:?};\npub const TEST:&str={:?};\npub const OPERATOR:&str={implementation:?};\npub const FAMILY:&str={family:?};\n",interface.id,recipe.identity.id)).map_err(|e|e.to_string())?;
    if family == "product-abi" {
        fs::write(directory.join("build.rs"),"fn main(){let d=std::env::var(\"FOUNTAIN_ENGINE_LIB_DIR\").unwrap();println!(\"cargo:rustc-link-arg=-Wl,-rpath,{}\",d);}").map_err(|e|e.to_string())?;
    }
    let mut command = Command::new("cargo");
    command.args(["generate-lockfile"]).current_dir(&directory);
    process::controlled(&mut command);
    process::run(
        &mut command,
        &logs,
        "driver-lock",
        Duration::from_secs(1800),
    )?;
    let mut command = Command::new("cargo");
    command
        .args(["build", "--release", "--locked"])
        .current_dir(&directory);
    process::controlled(&mut command);
    if family == "product-abi" {
        command.env("FOUNTAIN_ENGINE_LIB_DIR", directory.join("lib"));
    }
    process::run(
        &mut command,
        &logs,
        "driver-build",
        Duration::from_secs(1800),
    )?;
    let graph: Value = serde_json::from_str(&cargo(
        &directory,
        &logs,
        "driver-metadata",
        &["metadata", "--locked", "--format-version", "1"],
    )?)
    .map_err(|e| e.to_string())?;
    let engines: Vec<_> = graph["packages"]
        .as_array()
        .ok_or("metadata packages missing")?
        .iter()
        .filter(|p| p["name"] == "fountain_engine")
        .collect();
    if engines.len() != 1 {
        return Err(format!(
            "native trait graph has {} engine identities",
            engines.len()
        ));
    }
    if family != "product-abi"
        && engines[0]["manifest_path"] != engine_dir.join("Cargo.toml").to_string_lossy().as_ref()
    {
        return Err("built engine is not the selected engine".into());
    }
    let binary = directory.join("target/release/fountain-eval-driver");
    let mut artifacts = BTreeMap::new();
    for rel in [
        "Cargo.toml",
        "Cargo.lock",
        "src/main.rs",
        "src/operator.rs",
        "src/api.rs",
        "src/selected.rs",
        "src/config.rs",
        "target/release/fountain-eval-driver",
    ] {
        artifacts.insert(PathBuf::from(rel), util::file_hash(&directory.join(rel))?);
    }
    if let Some(p) = &product_library {
        artifacts.insert(
            p.strip_prefix(&directory).unwrap().to_owned(),
            util::file_hash(p)?,
        );
        artifacts.insert(
            engine_dir
                .join("Cargo.lock")
                .strip_prefix(&directory)
                .unwrap()
                .to_owned(),
            util::file_hash(&engine_dir.join("Cargo.lock"))?,
        );
    }
    for path in &composer.seen {
        artifacts.insert(
            path.strip_prefix(&directory).unwrap().to_owned(),
            util::file_hash(path)?,
        );
    }
    fn collect_inputs(
        root: &Path,
        here: &Path,
        artifacts: &mut BTreeMap<PathBuf, String>,
    ) -> Result<()> {
        for entry in fs::read_dir(here).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            if path.is_dir() {
                collect_inputs(root, &path, artifacts)?;
            } else {
                artifacts.insert(
                    path.strip_prefix(root).unwrap().into(),
                    util::file_hash(&path)?,
                );
            }
        }
        Ok(())
    }
    collect_inputs(&directory, &directory.join("sources"), &mut artifacts)?;
    if directory.join("build.rs").is_file() {
        artifacts.insert(
            "build.rs".into(),
            util::file_hash(&directory.join("build.rs"))?,
        );
    }
    #[cfg(target_os = "linux")]
    if product_library.is_some() {
        artifacts.insert(
            "lib/libfountain_raptor_q_bin.so.1".into(),
            util::file_hash(&directory.join("lib/libfountain_raptor_q_bin.so.1"))?,
        );
    }
    let mut support = Vec::new();
    for (role, graph) in [
        ("driver", Some(&graph)),
        ("product", product_graph.as_ref()),
    ] {
        if let Some(graph) = graph {
            for p in graph["packages"]
                .as_array()
                .ok_or("missing dependency packages")?
            {
                if p["name"] == engine.name
                    || p["name"] == operator.name
                    || p["name"] == "fountain-eval-driver"
                {
                    continue;
                }
                let path = p["manifest_path"]
                    .as_str()
                    .ok_or("missing dependency manifest")?;
                let source = p["source"].as_str().map(str::to_owned).unwrap_or_else(|| {
                    path.strip_prefix(directory.to_string_lossy().as_ref())
                        .unwrap_or(path)
                        .to_owned()
                });
                let source_digest = if p["source"].is_string() {
                    Some(util::tree_hash(Path::new(path).parent().unwrap())?)
                } else {
                    None
                };
                let features = graph["resolve"]["nodes"]
                    .as_array()
                    .and_then(|nodes| nodes.iter().find(|n| n["id"] == p["id"]))
                    .map(|n| n["features"].clone());
                support.push(json!({"role":role,"name":p["name"],"version":p["version"],"source":source,"source_digest":source_digest,"features":features}));
            }
        }
    }
    support.sort_by_key(Value::to_string);
    let log_copy = directory.join("provenance/logs");
    fs::create_dir_all(&log_copy).map_err(|e| e.to_string())?;
    for rel in util::files(&logs)? {
        let to = log_copy.join(&rel);
        fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(logs.join(&rel), &to).map_err(|e| e.to_string())?;
        artifacts.insert(
            to.strip_prefix(&directory).unwrap().into(),
            util::file_hash(&to)?,
        );
    }
    let closure = json!({"driver_lock":util::file_hash(&directory.join("Cargo.lock"))?,"product_lock":if family=="product-abi"{Some(util::file_hash(&engine_dir.join("Cargo.lock"))?)}else{None},"support_packages":support});
    let built = Built {
        key,
        directory: directory.clone(),
        binary,
        artifacts,
        interface,
        family,
        implementation: implementation.into(),
        components: json!({"engine":engine.identity(),"operator":operator.identity(),"implementation":implementation,"dependencies":closure,"input_sources":input_sources}),
        dependency_graph: json!({"driver":graph,"product":product_graph}),
        bindings: json!(composer.bindings),
        product_library,
    };
    util::write_json(&directory.join("build.json"), &built)?;
    fs::write(
        directory.join("build.sha256"),
        util::file_hash(&directory.join("build.json"))?,
    )
    .map_err(|e| e.to_string())?;
    Ok(built)
}
