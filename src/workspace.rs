//! Per-invocation temporary storage and ownership. No cache is shared across batches.
use crate::{
    batch,
    util::{self, Result},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

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
pub(crate) fn remove(directory: &Path, report: &Path) -> Result<()> {
    fs::remove_dir_all(directory).map_err(|e| {
        format!(
            "report saved at {}; cleanup incomplete at {}: {e}",
            report.display(),
            directory.display()
        )
    })?;
    // Remove the immediate empty storage root only. Never recurse into its parent.
    if let Some(parent) = directory.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}
