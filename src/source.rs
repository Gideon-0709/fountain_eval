//! Immutable source snapshots; original worktrees are never used as build roots.
use crate::{
    process,
    protocol::{ComponentReference, SourceReference},
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrozenSource {
    pub root: PathBuf,
    pub original_root: PathBuf,
    pub digest: String,
    pub origin: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub source: FrozenSource,
    pub manifest: PathBuf,
    pub name: String,
    pub version: String,
}
impl Package {
    pub fn path(&self) -> PathBuf {
        self.source.root.join(&self.manifest)
    }
    pub fn identity(&self) -> Value {
        json!({"package":self.name,"version":self.version,"source_digest":self.source.digest,"manifest":self.manifest,"origin":self.source.origin})
    }
}
pub struct Resolver {
    pub cache: PathBuf,
    pub logs: PathBuf,
    sources: BTreeMap<PathBuf, FrozenSource>,
}
impl Resolver {
    pub fn identities(&self) -> Vec<Value> {
        let mut values: Vec<_> = self
            .sources
            .values()
            .map(|s| json!({"digest":s.digest,"origin":s.origin}))
            .collect();
        values.sort_by_key(Value::to_string);
        values.dedup();
        values
    }
    pub fn path_dependency(
        &mut self,
        package: &Package,
        base: &Path,
        relative: &str,
    ) -> Result<Package> {
        let path = base.join(relative).join("Cargo.toml");
        if let Ok(path) = path.canonicalize() {
            if let Ok(rel) = path.strip_prefix(&package.source.root) {
                return package_at(package.source.clone(), rel.into());
            }
            return self.local(&path);
        }
        let rel = base
            .strip_prefix(&package.source.root)
            .map_err(|e| e.to_string())?;
        self.local(&package.source.original_root.join(rel).join(relative))
    }
    /// Capture path dependencies before computing any build-cache key.
    pub fn capture_paths(
        &mut self,
        initial: &[Package],
        overrides: &BTreeMap<String, Package>,
    ) -> Result<()> {
        let mut pending = initial.to_vec();
        let mut visited = std::collections::BTreeSet::new();
        while let Some(package) = pending.pop() {
            if !visited.insert((package.source.digest.clone(), package.manifest.clone())) {
                continue;
            }
            let manifest = util::read_toml(&package.path())?;
            let parent = package.path().parent().unwrap().to_owned();
            let workspace = workspace_root(&package.path());
            let inherited = workspace
                .as_ref()
                .map(|p| util::read_toml(&p.join("Cargo.toml")))
                .transpose()?;
            let mut groups = vec![&manifest];
            if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
                groups.extend(targets.values());
            }
            for group in groups {
                for kind in ["dependencies", "build-dependencies"] {
                    if let Some(deps) = group.get(kind).and_then(toml::Value::as_table) {
                        for (alias, raw) in deps {
                            let (decl, base) = if raw
                                .get("workspace")
                                .and_then(toml::Value::as_bool)
                                == Some(true)
                            {
                                let decl = inherited
                                    .as_ref()
                                    .and_then(|v| v.get("workspace"))
                                    .and_then(|v| v.get("dependencies"))
                                    .and_then(|v| v.get(alias))
                                    .ok_or("missing inherited dependency")?;
                                (
                                    decl,
                                    workspace.as_deref().ok_or("missing dependency workspace")?,
                                )
                            } else {
                                (raw, parent.as_path())
                            };
                            let name = decl
                                .get("package")
                                .and_then(toml::Value::as_str)
                                .unwrap_or(alias);
                            if let Some(selected) = overrides.get(name) {
                                pending.push(selected.clone());
                            } else if let Some(relative) =
                                decl.get("path").and_then(toml::Value::as_str)
                            {
                                pending.push(self.path_dependency(&package, base, relative)?);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn new(cache: &Path, logs: &Path) -> Self {
        Self {
            cache: cache.into(),
            logs: logs.into(),
            sources: BTreeMap::new(),
        }
    }
    pub fn resolve(&mut self, reference: &ComponentReference) -> Result<Package> {
        match &reference.source {
            SourceReference::Path { path } => self.local(path),
            SourceReference::Registry {
                package, version, ..
            } => self.registry(package, version),
            SourceReference::Git {
                url,
                revision,
                package,
            } => {
                let id = util::digest(&(url, revision));
                let dir = self.cache.join("git").join(&id);
                if !dir.join(".git").exists() {
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    process::run(
                        Command::new("git")
                            .args([
                                "init",
                                "--quiet",
                                if revision.len() == 64 {
                                    "--object-format=sha256"
                                } else {
                                    "--object-format=sha1"
                                },
                            ])
                            .current_dir(&dir),
                        &self.logs,
                        &format!("git-init-{id}"),
                        Duration::from_secs(60),
                    )?;
                }
                if !dir.join(".resolved").exists() {
                    process::run(
                        Command::new("git")
                            .args(["fetch", "--depth=1", url, revision])
                            .current_dir(&dir),
                        &self.logs,
                        &format!("git-fetch-{id}"),
                        Duration::from_secs(1800),
                    )?;
                    process::run(
                        Command::new("git")
                            .args(["checkout", "--detach", "--quiet", "FETCH_HEAD"])
                            .current_dir(&dir),
                        &self.logs,
                        &format!("git-checkout-{id}"),
                        Duration::from_secs(60),
                    )?;
                    process::run(
                        Command::new("git")
                            .args(["submodule", "update", "--init", "--recursive"])
                            .current_dir(&dir),
                        &self.logs,
                        &format!("git-submodules-{id}"),
                        Duration::from_secs(1800),
                    )?;
                    fs::write(dir.join(".resolved"), revision).map_err(|e| e.to_string())?;
                }
                let actual = git(&dir, &["rev-parse", "HEAD"])?;
                if actual.trim() != revision {
                    return Err("Git cache commit mismatch".into());
                }
                let status = git(&dir, &["status", "--porcelain", "--untracked-files=all"])?;
                if status.lines().any(|line| line.trim() != "?? .resolved") {
                    return Err("Git cache worktree is modified".into());
                }
                let snapshot =
                    self.freeze(&dir, json!({"kind":"git","url":url,"revision":revision}))?;
                let mut matches = Vec::new();
                for rel in util::files(&snapshot.root)? {
                    if rel.file_name().is_some_and(|v| v == "Cargo.toml") {
                        let manifest = util::read_toml(&snapshot.root.join(&rel))?;
                        if manifest
                            .get("package")
                            .and_then(|p| p.get("name"))
                            .and_then(toml::Value::as_str)
                            == Some(package)
                        {
                            matches.push(rel);
                        }
                    }
                }
                if matches.len() != 1 {
                    return Err(format!(
                        "Git package {package} matched {} manifests",
                        matches.len()
                    ));
                }
                package_at(snapshot, matches.remove(0))
            }
        }
    }
    pub fn local(&mut self, path: &Path) -> Result<Package> {
        let manifest = if path.is_dir() {
            path.join("Cargo.toml")
        } else {
            path.to_owned()
        };
        let manifest = manifest
            .canonicalize()
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        for source in self.sources.values() {
            if let Ok(rel) = manifest.strip_prefix(&source.original_root) {
                return package_at(source.clone(), rel.into());
            }
        }
        let parent = manifest.parent().unwrap();
        let root = git(parent, &["rev-parse", "--show-toplevel"])
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root(&manifest).unwrap_or_else(|| parent.into()));
        let revision = git(&root, &["rev-parse", "HEAD"]).ok();
        let dirty = git(&root, &["status", "--porcelain", "--untracked-files=all"])
            .ok()
            .map(|v| !v.is_empty());
        let source = self.freeze(
            &root,
            json!({"kind":"path","revision":revision,"dirty":dirty}),
        )?;
        let rel = manifest
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_owned();
        package_at(source, rel)
    }
    pub fn registry(&mut self, name: &str, version: &str) -> Result<Package> {
        let id = util::digest(&(name, version));
        let dir = self.cache.join("registry").join(&id);
        fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        let manifest = toml::toml! {[package] name="eval-resolution" version="0.0.0" edition="2024" [workspace] [dependencies]};
        let mut manifest = toml::Value::Table(manifest);
        let selected =
            toml::Value::try_from(json!({"package":name,"version":format!("={version}")}))
                .map_err(|e| e.to_string())?;
        util::toml_set(&mut manifest["dependencies"], "selected", selected);
        util::write_toml(&dir.join("Cargo.toml"), &manifest)?;
        fs::write(dir.join("src/lib.rs"), "").map_err(|e| e.to_string())?;
        let mut command = Command::new("cargo");
        command
            .args(["metadata", "--format-version", "1"])
            .current_dir(&dir);
        process::controlled(&mut command);
        if dir.join("Cargo.lock").exists() {
            command.arg("--locked");
        }
        let data = process::run(
            &mut command,
            &self.logs,
            &format!("registry-{id}"),
            Duration::from_secs(1800),
        )?;
        let data: Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        let p = data["packages"]
            .as_array()
            .ok_or("Cargo metadata has no packages")?
            .iter()
            .find(|p| {
                p["name"] == name
                    && p["version"] == version
                    && p["source"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("registry+"))
            })
            .ok_or("registry package did not resolve exactly")?;
        let path = PathBuf::from(
            p["manifest_path"]
                .as_str()
                .ok_or("missing registry manifest")?,
        );
        let root = path.parent().unwrap();
        let registry_id = root
            .parent()
            .and_then(Path::file_name)
            .ok_or("registry directory missing")?;
        let registry_root = root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("registry cache missing")?;
        let archive = registry_root
            .join("cache")
            .join(registry_id)
            .join(format!("{name}-{version}.crate"));
        let lock = util::read_toml(&dir.join("Cargo.lock"))?;
        let checksum = lock["package"]
            .as_array()
            .ok_or("registry lock missing packages")?
            .iter()
            .find(|p| {
                p.get("name").and_then(toml::Value::as_str) == Some(name)
                    && p.get("version").and_then(toml::Value::as_str) == Some(version)
            })
            .and_then(|p| p.get("checksum"))
            .and_then(toml::Value::as_str)
            .ok_or("registry lock missing checksum")?;
        if util::file_hash(&archive)? != checksum {
            return Err("registry archive checksum mismatch".into());
        }
        let unpacked = dir.join(format!(
            "unpacked-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&unpacked).map_err(|e| e.to_string())?;
        let source_result = (|| {
            let listing = process::run(
                Command::new("tar").arg("-tf").arg(&archive),
                &self.logs,
                &format!("registry-list-{id}"),
                Duration::from_secs(60),
            )?;
            for entry in listing.lines() {
                let p = Path::new(entry);
                if p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                    || !p.starts_with(format!("{name}-{version}"))
                {
                    return Err("invalid registry archive path".into());
                }
            }
            process::run(
                Command::new("tar")
                    .arg("-xf")
                    .arg(&archive)
                    .arg("-C")
                    .arg(&unpacked),
                &self.logs,
                &format!("registry-extract-{id}"),
                Duration::from_secs(60),
            )?;
            self.freeze(&unpacked.join(format!("{name}-{version}")),json!({"kind":"registry","registry":"crates-io","package":name,"version":version,"checksum":checksum}))
        })();
        let _ = fs::remove_dir_all(&unpacked);
        let source = source_result?;
        package_at(source, "Cargo.toml".into())
    }
    fn freeze(&mut self, root: &Path, origin: Value) -> Result<FrozenSource> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if let Some(source) = self.sources.get(&root) {
            return Ok(source.clone());
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let staging = self
            .cache
            .join("staging")
            .join(format!("{}-{nonce}", std::process::id()));
        fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let result = (|| {
            let capture_paths = || -> Result<Vec<PathBuf>> {
                Ok(source_files(&root)?
                    .into_iter()
                    .filter(|p| {
                        let absolute = root.join(p);
                        !(self.cache.starts_with(&root) && absolute.starts_with(&self.cache)
                            || self.logs.starts_with(&root)
                                && absolute.starts_with(self.logs.parent().unwrap_or(&self.logs)))
                    })
                    .collect())
            };
            let paths = capture_paths()?;
            for rel in &paths {
                let from = root.join(rel);
                if from.is_dir() {
                    continue;
                }
                let canonical = from.canonicalize().map_err(|e| e.to_string())?;
                if !canonical.starts_with(&root) {
                    return Err(format!("source link escapes snapshot: {}", from.display()));
                }
                let to = staging.join(rel);
                fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::copy(&from, &to).map_err(|e| e.to_string())?;
            }
            // Detect changes during capture rather than combining two source states.
            if paths != capture_paths()? {
                return Err("source changed during snapshot".into());
            }
            for rel in &paths {
                if root.join(rel).is_file()
                    && util::file_hash(&root.join(rel))? != util::file_hash(&staging.join(rel))?
                {
                    return Err("source changed during snapshot".into());
                }
            }
            let digest = util::tree_hash(&staging)?;
            let destination = self.cache.join("sources").join(&digest);
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            if destination.exists() {
                if util::tree_hash(&destination)? != digest {
                    return Err("source cache checksum mismatch".into());
                }
                fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
            } else {
                fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
            }
            Ok(FrozenSource {
                root: destination,
                original_root: root.clone(),
                digest,
                origin,
            })
        })();
        if staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        let source = result?;
        self.sources.insert(root, source.clone());
        Ok(source)
    }
}
pub fn workspace_root(manifest: &Path) -> Option<PathBuf> {
    manifest
        .parent()?
        .ancestors()
        .find(|p| {
            util::read_toml(&p.join("Cargo.toml"))
                .ok()
                .is_some_and(|v| v.get("workspace").is_some())
        })
        .map(Path::to_owned)
}
pub fn package_at(source: FrozenSource, manifest: PathBuf) -> Result<Package> {
    let path = source.root.join(&manifest);
    let value = util::read_toml(&path)?;
    let package = value
        .get("package")
        .ok_or("select a package, not a virtual workspace root")?;
    let name = package
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or("missing package.name")?
        .to_owned();
    let version = if let Some(v) = package.get("version").and_then(toml::Value::as_str) {
        v.to_owned()
    } else {
        let root = workspace_root(&path).ok_or("inherited version has no workspace")?;
        util::read_toml(&root.join("Cargo.toml"))?["workspace"]["package"]["version"]
            .as_str()
            .ok_or("missing workspace package version")?
            .to_owned()
    };
    semver::Version::parse(&version).map_err(|e| e.to_string())?;
    Ok(Package {
        source,
        manifest,
        name,
        version,
    })
}
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("not a readable Git checkout".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
fn source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let mut paths = if root.join(".git").exists() && out.status.success() {
        String::from_utf8(out.stdout)
            .map_err(|e| e.to_string())?
            .split('\0')
            .filter(|s| !s.is_empty() && *s != ".resolved")
            .map(PathBuf::from)
            .filter(|p| root.join(p).exists())
            .collect()
    } else {
        fn walk(root: &Path, here: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
            for entry in fs::read_dir(here).map_err(|e| e.to_string())? {
                let p = entry.map_err(|e| e.to_string())?.path();
                if p.file_name().is_some_and(|n| {
                    [".git", "target", ".fountain-eval", "runs", ".DS_Store"]
                        .iter()
                        .any(|s| n == *s)
                }) {
                    continue;
                }
                if p.is_dir() {
                    if p.symlink_metadata()
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_symlink()
                    {
                        return Err(format!(
                            "directory source symlinks are unsupported: {}",
                            p.display()
                        ));
                    }
                    walk(root, &p, out)?;
                } else {
                    out.push(p.strip_prefix(root).unwrap().to_owned());
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(root, root, &mut out)?;
        out
    };
    let mut expanded = Vec::new();
    for p in paths.drain(..) {
        if p.components().any(|c|matches!(c,std::path::Component::Normal(n) if [".git","target",".fountain-eval","runs",".DS_Store"].iter().any(|v|n==*v))){continue;}
        let full = root.join(&p);
        if full.is_dir() {
            if full
                .symlink_metadata()
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err(format!(
                    "directory source symlinks are unsupported: {}",
                    full.display()
                ));
            }
            expanded.extend(source_files(&full)?.into_iter().map(|child| p.join(child)));
        } else {
            expanded.push(p);
        }
    }
    expanded.sort();
    expanded.dedup();
    Ok(expanded)
}
