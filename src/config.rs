//! One explicit configuration file. Paths are relative to that file, never to a hidden overlay.
use crate::{
    protocol::{
        BuildOptions, ComponentReference, ComponentSelection, EvaluationRequest, SchemeSelection,
        SourceReference, TestReference,
    },
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct Input {
    #[serde(flatten)]
    components: BTreeMap<String, ComponentInput>,
    scheme: SchemeInput,
    test: String,
    #[serde(default)]
    run: RunSettings,
}
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ComponentInput {
    Source(String),
    Detailed(ComponentDetail),
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ComponentDetail {
    source: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    default_features: bool,
}
impl ComponentInput {
    fn resolve(self, base: &Path) -> Result<ComponentReference> {
        let (source, mut build) = match self {
            Self::Source(source) => (source, BuildOptions::default()),
            Self::Detailed(detail) => (
                detail.source,
                BuildOptions {
                    features: detail.features,
                    default_features: detail.default_features,
                },
            ),
        };
        build.normalize().map_err(|e| e.to_string())?;
        let mut reference = resolve_reference(base, &source)?;
        reference.build = build;
        Ok(reference)
    }
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
    /// The single permanent Markdown result destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<PathBuf>,
    /// Temporary execution root; completed run directories are removed.
    #[serde(default = "output_default")]
    pub output: PathBuf,
    /// Temporary per-batch cache root; entries are reused only within one batch.
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
        let source = scheme
            .source
            .as_deref()
            .map(|s| resolve_reference(base, s))
            .transpose()?;
        if source.as_ref().is_some_and(|s| s.implementation.is_some()) {
            return Err(
                "scheme.source cannot contain an implementation selector; use scheme.name".into(),
            );
        }
        let scheme = SchemeSelection {
            name: scheme.name,
            mode: scheme.mode,
            source,
        };
        let test = input
            .test
            .parse::<TestReference>()
            .map_err(|e| e.to_string())?;
        let mut groups = BTreeMap::<u32, ComponentSelection>::new();
        for (key, value) in input.components {
            let (engine, suffix) = if let Some(n) = key.strip_prefix("engine") {
                (true, n)
            } else if let Some(n) = key.strip_prefix("operator") {
                (false, n)
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
            let reference = value.resolve(base).map_err(|e| format!("{key}: {e}"))?;
            if engine {
                entry.engine = Some(reference);
            } else {
                entry.operator = Some(reference);
            }
        }
        if groups.is_empty() {
            return Err(
                "configuration must select at least one C with engineN and/or operatorN".into(),
            );
        }
        let selections = groups
            .into_iter()
            .map(|(id, components)| Selection {
                id,
                request: EvaluationRequest {
                    components,
                    test: test.clone(),
                    scheme: scheme.clone(),
                },
            })
            .collect();
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
fn resolve_reference(base: &Path, source: &str) -> Result<ComponentReference> {
    let mut reference = source
        .parse::<ComponentReference>()
        .map_err(|e| e.to_string())?;
    if let SourceReference::Path { path } = &mut reference.source {
        *path = resolve(base, path)?;
    }
    Ok(reference)
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
