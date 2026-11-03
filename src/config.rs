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
struct Input {
    #[serde(flatten)]
    components: std::collections::BTreeMap<String, String>,
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
    /// Human-readable output, separate from immutable execution evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<PathBuf>,
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
            report: None,
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
    pub selections: Vec<Selection>,
    pub run: RunSettings,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    pub id: u32,
    pub request: EvaluationRequest,
}

/// The frozen configuration consumed by one C, retaining the complete original input.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunConfiguration {
    pub path: PathBuf,
    pub original: String,
    pub digest: String,
    pub component_id: u32,
    pub request: EvaluationRequest,
    pub run: RunSettings,
}
impl Configuration {
    pub fn report_root(&self) -> PathBuf {
        self.run.report.clone().unwrap_or_else(|| {
            self.path
                .parent()
                .expect("absolute configuration path")
                .join("reports")
        })
    }
    pub fn selection(&self, selected: &Selection) -> RunConfiguration {
        RunConfiguration {
            path: self.path.clone(),
            original: self.original.clone(),
            digest: self.digest.clone(),
            component_id: selected.id,
            request: selected.request.clone(),
            run: self.run.clone(),
        }
    }
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
        let base = path.parent().ok_or("configuration directory missing")?;
        let scheme_source = scheme
            .source
            .map(|s| s.parse())
            .transpose()
            .map_err(|e: crate::protocol::ReferenceError| e.to_string())?;
        let mut groups = std::collections::BTreeMap::<u32, (Option<String>, Option<String>)>::new();
        for (key, value) in input.components {
            let (role, suffix) = if let Some(n) = key.strip_prefix("engine") {
                (0, n)
            } else if let Some(n) = key.strip_prefix("operator") {
                (1, n)
            } else {
                return Err(format!(
                    "unknown configuration field {key}; use engine0/operator0, engine1/operator1, ..."
                ));
            };
            let id: u32 = suffix
                .parse()
                .map_err(|_| format!("invalid component key {key}; use a nonnegative index"))?;
            if suffix != id.to_string() {
                return Err(format!("noncanonical component index in {key}"));
            }
            let entry = groups.entry(id).or_default();
            if role == 0 {
                entry.0 = Some(value);
            } else {
                entry.1 = Some(value);
            }
        }
        if groups.is_empty() {
            return Err(
                "configuration must select at least one C with engineN and/or operatorN".into(),
            );
        }
        let mut selections = Vec::new();
        for (id, (engine, operator)) in groups {
            let mut request = EvaluationRequest::parse(
                engine.as_deref(),
                operator.as_deref(),
                &scheme.name,
                &input.test,
            )
            .map_err(|e| e.to_string())?;
            request.scheme = SchemeSelection {
                name: scheme.name.clone(),
                mode: scheme.mode.clone(),
                source: scheme_source.clone(),
            };
            for reference in request
                .components
                .engine
                .iter_mut()
                .chain(request.components.operator.iter_mut())
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
                    "scheme.source cannot contain an implementation selector; use scheme.name"
                        .into(),
                );
            }
            selections.push(Selection { id, request });
        }
        let run = RunSettings {
            report: input
                .run
                .report
                .as_deref()
                .map(|p| resolve(base, p))
                .transpose()?,
            output: resolve(base, &input.run.output)?,
            cache: resolve(base, &input.run.cache)?,
        };
        Ok(Self {
            path,
            digest: util::digest(&original),
            original,
            selections,
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
