//! One explicit configuration file. Paths are relative to that file, never to a hidden overlay.
use crate::{
    protocol::{EvaluationRequest, SchemeSelection, SourceReference},
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    engine: String,
    operator: String,
    scheme: SchemeInput,
    test: String,
    #[serde(default)]
    run: RunSettings,
}
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SchemeInput {
    Name(String),
    Detailed(SchemeDetail),
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemeDetail {
    name: String,
    #[serde(default = "systematic")]
    mode: String,
    source: Option<String>,
}
fn systematic() -> String {
    "systematic".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSettings {
    #[serde(default = "output_default")]
    pub output: PathBuf,
    #[serde(default = "cache_default")]
    pub cache: PathBuf,
}
fn output_default() -> PathBuf {
    "runs".into()
}
fn cache_default() -> PathBuf {
    ".fountain-eval".into()
}
impl Default for RunSettings {
    fn default() -> Self {
        Self {
            output: output_default(),
            cache: cache_default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub path: PathBuf,
    pub original: String,
    pub digest: String,
    pub request: EvaluationRequest,
    pub run: RunSettings,
}
impl Configuration {
    pub fn load(path: &Path) -> Result<Self> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("configuration {}: {e}", path.display()))?;
        let original = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Self::parse(path, original)
    }
    pub fn parse(path: PathBuf, original: String) -> Result<Self> {
        if !path.is_absolute() {
            return Err("configuration location must be absolute".into());
        }
        let input: Input = toml::from_str(&original).map_err(|e| format!("configuration: {e}"))?;
        let scheme = match input.scheme {
            SchemeInput::Name(name) => SchemeDetail {
                name,
                mode: systematic(),
                source: None,
            },
            SchemeInput::Detailed(detail) => detail,
        };
        let mut request =
            EvaluationRequest::parse(&input.engine, &input.operator, &scheme.name, &input.test)
                .map_err(|e| e.to_string())?;
        request.scheme = SchemeSelection {
            name: scheme.name,
            mode: scheme.mode,
            source: scheme
                .source
                .map(|s| s.parse())
                .transpose()
                .map_err(|e: crate::protocol::ReferenceError| e.to_string())?,
        };
        let base = path.parent().ok_or("configuration directory missing")?;
        for reference in [
            &mut request.components.engine,
            &mut request.components.operator,
        ]
        .into_iter()
        .chain(request.scheme.source.iter_mut())
        {
            if let SourceReference::Path { path } = &mut reference.source {
                *path = resolve(base, path)?;
            }
        }
        if request
            .scheme
            .source
            .as_ref()
            .is_some_and(|s| s.implementation.is_some())
        {
            return Err(
                "scheme.source cannot contain an implementation selector; use scheme.name".into(),
            );
        }
        let run = RunSettings {
            output: resolve(base, &input.run.output)?,
            cache: resolve(base, &input.run.cache)?,
        };
        Ok(Self {
            path,
            digest: util::digest(&original),
            original,
            request,
            run,
        })
    }
}
fn resolve(base: &Path, path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err("configuration paths cannot be empty".into());
    }
    Ok(if path.is_absolute() {
        path.into()
    } else {
        base.join(path)
    })
}
