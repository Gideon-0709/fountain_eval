use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub type Result<T> = std::result::Result<T, String>;

pub fn catch<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".into());
        Err(format!("internal framework error: {message}"))
    })
}

pub fn toml_set(value: &mut toml::Value, key: &str, item: toml::Value) {
    value
        .as_table_mut()
        .expect("TOML table")
        .insert(key.to_owned(), item);
}

pub fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
pub fn digest(value: &impl Serialize) -> String {
    hash(
        serde_json::to_vec(&serde_json::to_value(value).expect("serializable identity"))
            .expect("canonical JSON"),
    )
}
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
pub fn read_toml(path: &Path) -> Result<toml::Value> {
    toml::from_str(&fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}
pub fn write_toml(path: &Path, value: &toml::Value) -> Result<()> {
    fs::write(
        path,
        toml::to_string_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn file_hash(path: &Path) -> Result<String> {
    Ok(hash(
        fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
    ))
}
pub fn files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    fn walk(root: &Path, here: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
        for entry in fs::read_dir(here).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
                return Err(format!("unresolved source symlink: {}", p.display()));
            }
            if p.is_dir() {
                walk(root, &p, out)?;
            } else {
                out.push(p.strip_prefix(root).unwrap().to_owned());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}
pub fn tree_hash(root: &Path) -> Result<String> {
    let mut entries = Vec::new();
    for p in files(root)? {
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(root.join(&p))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o111
        };
        #[cfg(not(unix))]
        let executable = 0;
        entries.push((
            p.to_string_lossy().replace('\\', "/"),
            file_hash(&root.join(&p))?,
            executable,
        ));
    }
    Ok(digest(&entries))
}
