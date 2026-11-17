//! Active workspace selection and ownership of per-run data.
use crate::{
    batch,
    config::{Configuration, DEFAULT_CONFIG_NAME, RunSettings},
    util::{self, Result},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_WORKSPACE: &str = "feval_workspace";
const INTERNAL: &str = ".fountain-eval";
const MARKER: &str = "workspace";
const FORMAT: &str = "feval-workspace-1\n";

pub struct Workspace {
    root: PathBuf,
}
impl Workspace {
    pub fn initialize(target: Option<&Path>) -> Result<Self> {
        let reset = target.is_some();
        let target = if let Some(path) = target {
            path.to_owned()
        } else {
            let path = env::current_dir()
                .map_err(|e| e.to_string())?
                .join(DEFAULT_WORKSPACE);
            fs::create_dir(&path)
                .map_err(|e| format!("cannot create workspace {}: {e}", path.display()))?;
            path
        };
        let root = target
            .canonicalize()
            .map_err(|e| format!("workspace {}: {e}", target.display()))?;
        if !root.is_dir() || root.parent().is_none() {
            return Err(format!(
                "expected an existing workspace directory below the filesystem root: {}",
                root.display()
            ));
        }
        let _guard = directory_lock(&root, true)?;
        let workspace = Self { root };
        if reset {
            empty(&workspace.root)?;
        }
        for directory in [
            workspace.results(),
            workspace.reports(),
            workspace.root.join(INTERNAL),
            workspace.prepared(),
            workspace.cache(),
        ] {
            fs::create_dir(&directory)
                .map_err(|e| format!("cannot create {}: {e}", directory.display()))?;
        }
        Configuration::initialize(&workspace.root)?;
        fs::write(workspace.root.join(INTERNAL).join(MARKER), FORMAT).map_err(|e| e.to_string())?;
        workspace.remember()?;
        Ok(workspace)
    }

    pub fn activate(path: &Path) -> Result<Self> {
        let root = path
            .canonicalize()
            .map_err(|e| format!("workspace {}: {e}", path.display()))?;
        let workspace = Self { root };
        let _guard = workspace.lock()?;
        workspace.remember()?;
        Ok(workspace)
    }

    pub fn active() -> Result<Self> {
        let state = state_root()?.join("active.json");
        let root: PathBuf = util::read_json(&state).map_err(|e| {
            format!(
                "No readable active workspace. Run feval init or feval activate <directory>. {e}"
            )
        })?;
        Ok(Self { root })
    }

    fn remember(&self) -> Result<()> {
        let state = state_root()?;
        fs::create_dir_all(&state).map_err(|e| e.to_string())?;
        let directory = fs::File::open(&state).map_err(|e| e.to_string())?;
        fs2::FileExt::lock_exclusive(&directory).map_err(|e| e.to_string())?;
        util::write_json(&state.join("active.json"), &self.root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock(&self) -> Result<fs::File> {
        self.checked_lock(false)
    }

    fn checked_lock(&self, exclusive: bool) -> Result<fs::File> {
        let invalid = || {
            format!(
                "Invalid workspace {}. Run feval init or feval activate <directory>.",
                self.root.display()
            )
        };
        if self.root.canonicalize().ok().as_ref() != Some(&self.root) || !self.root.is_dir() {
            return Err(invalid());
        }
        let guard = directory_lock(&self.root, exclusive)?;
        for directory in [
            self.root.join(INTERNAL),
            self.results(),
            self.reports(),
            self.prepared(),
            self.cache(),
        ] {
            if !fs::symlink_metadata(directory).is_ok_and(|m| m.is_dir()) {
                return Err(invalid());
            }
        }
        if fs::read_to_string(self.root.join(INTERNAL).join(MARKER))
            .as_deref()
            .ok()
            != Some(FORMAT)
        {
            return Err(invalid());
        }
        Ok(guard)
    }

    pub fn configuration(&self, path: Option<&Path>) -> Result<Configuration> {
        let path = self
            .root
            .join(path.unwrap_or_else(|| Path::new(DEFAULT_CONFIG_NAME)));
        let path = path
            .canonicalize()
            .map_err(|e| format!("configuration {}: {e}", path.display()))?;
        if !path.starts_with(&self.root) || !path.is_file() {
            return Err("configuration must be a file inside the active workspace".into());
        }
        Configuration::load_at(&path, Some(self.root.clone()))
    }

    pub fn prepared_input(&self, path: &Path) -> Result<PathBuf> {
        self.batch_input(&self.prepared(), path)
    }

    pub fn result_input(&self, path: &Path) -> Result<PathBuf> {
        self.batch_input(&self.results(), path)
    }

    fn batch_input(&self, directory: &Path, path: &Path) -> Result<PathBuf> {
        let path = if path.file_name() == Some(path.as_os_str()) {
            directory.join(path)
        } else {
            self.root.join(path)
        };
        let resolved = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if resolved.parent() != Some(directory)
            || !fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir())
        {
            return Err(format!("expected an entry in {}", directory.display()));
        }
        Ok(resolved)
    }

    pub fn reports(&self) -> PathBuf {
        self.root.join("reports")
    }

    fn results(&self) -> PathBuf {
        self.root.join("results")
    }

    fn prepared(&self) -> PathBuf {
        prepared_root(&self.root)
    }

    fn cache(&self) -> PathBuf {
        self.root.join(INTERNAL).join("cache")
    }

    pub fn clean(&self) -> Result<()> {
        let _guard = self.checked_lock(true)?;
        for directory in [self.results(), self.prepared(), self.cache()] {
            empty(&directory)?;
        }
        Ok(())
    }
}

fn directory_lock(path: &Path, exclusive: bool) -> Result<fs::File> {
    let file = fs::File::open(path).map_err(|e| format!("workspace {}: {e}", path.display()))?;
    let locked = if exclusive {
        fs2::FileExt::try_lock_exclusive(&file)
    } else {
        fs2::FileExt::try_lock_shared(&file)
    };
    locked.map_err(|e| format!("workspace is in use: {}: {e}", path.display()))?;
    Ok(file)
}

fn empty(directory: &Path) -> Result<()> {
    for entry in
        fs::read_dir(directory).map_err(|e| format!("cannot read {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let removed = if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        removed.map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
    }
    Ok(())
}

fn state_root() -> Result<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::home_dir().map(|p| p.join(".local/state")))
        .map(|p| p.join("feval"))
        .ok_or_else(|| "Cannot locate the user state directory. Set XDG_STATE_HOME.".into())
}

pub(crate) fn storage(root: &Path) -> RunSettings {
    let workspace = Workspace {
        root: root.to_owned(),
    };
    RunSettings {
        report: Some(workspace.reports()),
        output: workspace.results(),
        cache: workspace.cache(),
    }
}

pub(crate) fn prepared_root(root: &Path) -> PathBuf {
    root.join(INTERNAL).join("prepared")
}

pub(crate) struct Lease {
    pub directory: PathBuf,
    _file: fs::File,
}
impl Lease {
    pub fn acquire(directory: &Path) -> Result<Self> {
        if fs::symlink_metadata(directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("refusing a symlinked workspace".into());
        }
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("lease.tmp"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&file)
            .map_err(|e| format!("workspace is in use: {}: {e}", directory.display()))?;
        Ok(Self {
            directory,
            _file: file,
        })
    }
}
pub(crate) fn create_cache(raw: &Path, root: &Path, id: &str) -> Result<PathBuf> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let cache = root.canonicalize().map_err(|e| e.to_string())?.join(id);
    fs::create_dir(&cache).map_err(|e| e.to_string())?;
    util::write_json(
        &cache.join("owner.json"),
        &serde_json::json!({"run_id":id,"directory":raw}),
    )?;
    util::write_json(
        &raw.join("workspace.json"),
        &serde_json::json!({"cache":cache}),
    )?;
    Ok(cache)
}
/// Validate all deletion targets before removing anything. Legacy shared caches are not owned.
pub(crate) fn cache_for_cleanup(
    raw: &Path,
    record: &batch::Record,
    report: &Path,
) -> Result<Option<PathBuf>> {
    if !raw.join("workspace.json").exists() {
        return Ok(None);
    }
    let state: serde_json::Value = util::read_json(&raw.join("workspace.json"))?;
    let cache = PathBuf::from(state["cache"].as_str().ok_or("missing workspace cache")?);
    let expected = record.configuration.run.cache.join(&record.run_id);
    // A previous cleanup may have removed the cache before a filesystem error on raw data.
    if !cache.exists() {
        return Ok(None);
    }
    if fs::symlink_metadata(&cache)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("refusing a symlinked cache".into());
    }
    let cache = cache.canonicalize().map_err(|e| e.to_string())?;
    if cache != expected.canonicalize().map_err(|e| e.to_string())?
        || report.starts_with(&cache)
        || raw.starts_with(&cache)
        || cache.starts_with(raw)
    {
        return Err("unsafe workspace/cache/report overlap".into());
    }
    let owner: serde_json::Value = util::read_json(&cache.join("owner.json"))?;
    if owner != serde_json::json!({"run_id":record.run_id,"directory":raw}) {
        return Err("cache ownership mismatch; retained".into());
    }
    Ok(Some(cache))
}
pub(crate) fn remove(directory: &Path, report: &Path, preserve_root: bool) -> Result<()> {
    fs::remove_dir_all(directory).map_err(|e| {
        format!(
            "report saved at {}; cleanup incomplete at {}: {e}",
            report.display(),
            directory.display()
        )
    })?;
    // Remove the immediate empty storage root only. Never recurse into its parent.
    if !preserve_root && let Some(parent) = directory.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}
