//! Read-only, offline declaration checks preceding future resolution and execution.

use std::{fs, path::Path};

use semver::{Version, VersionReq};
use toml::Value;

use crate::protocol::{
    ComponentReference, Diagnostic, EvaluationRequest, FORMAT_VERSION, Inspection,
    InspectionStatus, LocalDeclarations, PackageDeclaration, Severity, SourceReference,
};

fn diagnostic(severity: Severity, code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity,
        code: code.into(),
        message: message.into(),
    }
}

/// Inspect selected local manifests without invoking Cargo or modifying sources.
pub fn inspect(request: EvaluationRequest) -> Inspection {
    let mut diagnostics = Vec::new();
    let engine = inspect_local("engine", &request.components.engine, &mut diagnostics);
    let operator = inspect_local("operator", &request.components.operator, &mut diagnostics);

    let selected = match (&request.components.engine.source, &engine) {
        (_, Some(declaration)) => declaration
            .version
            .as_deref()
            .filter(|_| declaration.package == "fountain_engine"),
        (
            SourceReference::Registry {
                package, version, ..
            },
            _,
        ) if package == "fountain_engine" => Some(version.as_str()),
        _ => None,
    };
    if let (Some(selected), Some(requirement)) = (
        selected,
        operator
            .as_ref()
            .and_then(|d| d.engine_requirement.as_deref()),
    ) {
        match (Version::parse(selected), VersionReq::parse(requirement)) {
            (Ok(version), Ok(requirement)) if !requirement.matches(&version) => {
                diagnostics.push(diagnostic(Severity::Error, "engine_version_conflict",
                    format!("operator declares fountain_engine {requirement}, but the selected engine declares {version}")));
            }
            (_, Err(_)) => diagnostics.push(diagnostic(
                Severity::Error,
                "invalid_engine_requirement",
                "operator declares an invalid engine version requirement",
            )),
            _ => {}
        }
    }

    diagnostics.push(diagnostic(Severity::Info, "resolution_deferred",
        "No source snapshot, resolved dependency graph, interface selection, environment recipe, build, or codec validation has been produced."));
    diagnostics.push(diagnostic(Severity::Info, "execution_unavailable",
        "This scaffold registers no interface implementations or test versions. Inspection output is not an evaluation result."));
    Inspection {
        format_version: FORMAT_VERSION,
        status: InspectionStatus::InspectionOnly,
        request,
        local_declarations: LocalDeclarations { engine, operator },
        diagnostics,
        execution_available: false,
    }
}

fn inspect_local(
    role: &str,
    reference: &ComponentReference,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PackageDeclaration> {
    let SourceReference::Path { path } = &reference.source else {
        diagnostics.push(diagnostic(
            Severity::Info,
            "remote_source_unresolved",
            format!("{role}: remote reference syntax checked; source has not been fetched"),
        ));
        return None;
    };
    match read_declaration(path) {
        Ok(declaration) => {
            if declaration.version.is_none() {
                diagnostics.push(diagnostic(Severity::Warning, "version_inheritance_deferred",
                    format!("{role}: package version is inherited; workspace resolution is not implemented")));
            }
            Some(declaration)
        }
        Err(message) => {
            diagnostics.push(diagnostic(
                Severity::Error,
                "local_manifest_error",
                format!("{role}: {message}"),
            ));
            None
        }
    }
}

fn read_declaration(path: &Path) -> Result<PackageDeclaration, String> {
    let manifest = if path.is_dir() {
        path.join("Cargo.toml")
    } else {
        path.to_owned()
    };
    if manifest.file_name().is_none_or(|name| name != "Cargo.toml") {
        return Err("select a crate directory or its Cargo.toml".into());
    }
    let manifest = manifest
        .canonicalize()
        .map_err(|e| format!("cannot locate {}: {e}", manifest.display()))?;
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let value: Value =
        toml::from_str(&text).map_err(|_| format!("invalid TOML in {}", manifest.display()))?;
    let package = value.get("package").and_then(Value::as_table).ok_or(
        "manifest has no [package]; select the component subdirectory inside the workspace",
    )?;
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("package.name is missing")?;
    let version = match package.get("version") {
        Some(Value::String(version)) if Version::parse(version).is_ok() => Some(version.clone()),
        Some(Value::Table(table))
            if table.get("workspace").and_then(Value::as_bool) == Some(true) =>
        {
            None
        }
        _ => return Err("package.version must be an exact SemVer or workspace-inherited".into()),
    };
    let engine_requirement = value
        .get("dependencies")
        .and_then(Value::as_table)
        .and_then(|dependencies| {
            dependencies.iter().find_map(|(key, declaration)| {
                let package = declaration
                    .get("package")
                    .and_then(Value::as_str)
                    .unwrap_or(key);
                if package != "fountain_engine"
                    || declaration.get("optional").and_then(Value::as_bool) == Some(true)
                    || declaration.get("workspace").and_then(Value::as_bool) == Some(true)
                {
                    return None;
                }
                declaration
                    .as_str()
                    .or_else(|| declaration.get("version").and_then(Value::as_str))
                    .map(str::to_owned)
            })
        });
    Ok(PackageDeclaration {
        manifest,
        package: name.into(),
        version,
        engine_requirement,
    })
}
