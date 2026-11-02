//! Implementation-independent inputs and inspection records.

use std::{fmt, path::PathBuf, str::FromStr};

use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;

/// Output format metadata, independent of interface and test versions.
pub const FORMAT_VERSION: u32 = 1;

/// A source selection. Inspection does not fetch or freeze its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceReference {
    Path {
        path: PathBuf,
    },
    Git {
        url: String,
        revision: String,
        package: String,
    },
    Registry {
        registry: String,
        package: String,
        version: String,
    },
}

/// One component reference, optionally selecting an implementation in its crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentReference {
    #[serde(flatten)]
    pub source: SourceReference,
    pub implementation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestReference {
    pub version: String,
}

/// Engine and operator references; scheme selection is recorded separately.
/// Support crates are resolved later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentPair {
    pub engine: ComponentReference,
    pub operator: ComponentReference,
}

/// The normalized configuration request. This is not a resolved (C, I, T, E) identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationRequest {
    pub components: ComponentPair,
    pub test: TestReference,
    pub scheme: SchemeSelection,
}

impl EvaluationRequest {
    pub fn parse(
        engine: &str,
        operator: &str,
        scheme: &str,
        test: &str,
    ) -> Result<Self, ReferenceError> {
        Ok(Self {
            components: ComponentPair {
                engine: engine.parse()?,
                operator: operator.parse()?,
            },
            test: test.parse()?,
            scheme: SchemeSelection {
                name: scheme.into(),
                mode: "systematic".into(),
                source: None,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceError(pub String);

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReferenceError {}

fn invalid(message: &str) -> ReferenceError {
    ReferenceError(message.to_owned())
}

fn valid_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn split_implementation(input: &str) -> Result<(&str, Option<String>), ReferenceError> {
    match input.split_once('#') {
        Some((source, name)) if valid_name(name) => Ok((source, Some(name.to_owned()))),
        Some(_) => Err(invalid("implementation must be an identifier after '#'")),
        None => Ok((input, None)),
    }
}

impl FromStr for ComponentReference {
    type Err = ReferenceError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (input, implementation) = split_implementation(input)?;
        let source = if let Some(path) = input.strip_prefix("path:") {
            if path.is_empty() || path.contains('\0') {
                return Err(invalid("path reference requires a nonempty path"));
            }
            SourceReference::Path { path: path.into() }
        } else if let Some(reference) = input.strip_prefix("registry:") {
            let (package, version) = reference
                .split_once('@')
                .ok_or_else(|| invalid("registry reference requires package@exact-version"))?;
            if !valid_name(package) || Version::parse(version).is_err() {
                return Err(invalid(
                    "registry reference requires a package name and exact SemVer",
                ));
            }
            SourceReference::Registry {
                registry: "crates-io".into(),
                package: package.into(),
                version: version.into(),
            }
        } else if let Some(reference) = input.strip_prefix("git+") {
            let mut url = Url::parse(reference).map_err(|_| invalid("invalid Git URL"))?;
            if !matches!(url.scheme(), "https" | "ssh")
                || url.host_str().is_none()
                || url.path().trim_matches('/').is_empty()
                || url.password().is_some()
                || (url.scheme() == "https" && !url.username().is_empty())
            {
                return Err(invalid(
                    "Git requires an HTTPS or SSH repository URL without embedded credentials",
                ));
            }
            let mut revision = None;
            let mut package = None;
            for (key, value) in url.query_pairs() {
                let slot = match key.as_ref() {
                    "rev" => &mut revision,
                    "package" => &mut package,
                    _ => {
                        return Err(invalid(
                            "Git URL accepts only 'rev' and 'package' query keys",
                        ));
                    }
                };
                if slot.replace(value.into_owned()).is_some() {
                    return Err(invalid("Git URL query keys must be unique"));
                }
            }
            let revision = revision.ok_or_else(|| invalid("Git reference requires rev"))?;
            if !matches!(revision.len(), 40 | 64)
                || !revision.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(invalid(
                    "Git rev must be a full 40- or 64-digit hexadecimal commit ID",
                ));
            }
            let package = package
                .filter(|v| valid_name(v))
                .ok_or_else(|| invalid("Git reference requires a valid package name"))?;
            url.set_query(None);
            SourceReference::Git {
                url: url.into(),
                revision: revision.to_ascii_lowercase(),
                package,
            }
        } else {
            return Err(invalid(
                "component reference must start with path:, registry:, or git+",
            ));
        };
        Ok(Self {
            source,
            implementation,
        })
    }
}

impl FromStr for TestReference {
    type Err = ReferenceError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if Version::parse(input).is_err() {
            return Err(invalid(
                "test requires an exact content version, for example 0.1.0; individual tests cannot be selected",
            ));
        }
        Ok(Self {
            version: input.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemeSelection {
    pub name: String,
    pub mode: String,
    pub source: Option<ComponentReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageDeclaration {
    pub manifest: PathBuf,
    pub package: String,
    pub version: Option<String>,
    /// A preliminary, unconditional, non-optional fountain_engine requirement.
    pub engine_requirement: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalDeclarations {
    pub engine: Option<PackageDeclaration>,
    pub operator: Option<PackageDeclaration>,
    pub scheme: Option<PackageDeclaration>,
}

/// Declaration inspection never implies build compatibility or codec correctness.
#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub format_version: u32,
    pub status: InspectionStatus,
    pub request: EvaluationRequest,
    pub local_declarations: LocalDeclarations,
    pub diagnostics: Vec<Diagnostic>,
    pub execution_available: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    InspectionOnly,
}

impl Inspection {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}
