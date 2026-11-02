use crate::validation::{Check, Severity};
use crate::{
    protocol::{SchemeSelection, Scope, TestReference},
    source::Package,
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const CONTENT: &str = include_str!("content.rs");
pub const REFERENCE: &str = include_str!("../drivers/reference.rs");
pub const WORKLOADS: &str = include_str!("../drivers/workloads.rs");
pub const TELEMETRY: &str = include_str!("../drivers/telemetry.rs");
pub const COMMON: &str = include_str!("../drivers/common.rs");
pub const OPERATOR: &str = include_str!("../drivers/operator.rs");
pub const NATIVE: &str = include_str!("../drivers/native.rs");
pub const PRODUCT: &str = include_str!("../drivers/product.rs");
pub const SCALAR_OPERATOR: &str = include_str!("../drivers/scalar_operator.rs");
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub identity: Identity,
    pub purpose: String,
    pub cases: Vec<String>,
    pub warmups: usize,
    pub repetitions: usize,
    pub definition: Value,
    pub checks: BTreeMap<String, Check>,
}
impl Recipe {
    pub fn check(&self, case: &str) -> Option<Check> {
        self.checks.get(case).cloned()
    }
    pub fn validation_cases(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter(|(_, c)| c.skip.is_none())
            .map(|(name, _)| name.clone())
            .collect()
    }
}
pub fn recipe(test: &TestReference) -> Result<Recipe> {
    if test.version != "0.1.0" {
        return Err(format!(
            "unsupported: unknown test content version {}",
            test.version
        ));
    }
    let checks: BTreeMap<_, _> = crate::content::cases()
        .into_iter()
        .map(|spec| {
            let check = Check {
                severity: if spec.warning {
                    Severity::Warning
                } else {
                    Severity::Fatal
                },
                rule: spec.workload.clone(),
                description: format!("{}: {}", spec.family, spec.workload),
                measured: spec.measured,
                payload_bytes: spec.payload_bytes(),
                operations: spec.calls(),
                skip: None,
                measurement_skip: None,
                spec,
            };
            (check.spec.id.clone(), check)
        })
        .collect();
    let definition = json!({"purpose":"formal","selection":"all applicable cases","workers":crate::content::WORKERS,
        "samples_per_worker":crate::content::SAMPLES_PER_WORKER,"warmups_per_worker":1,
        "seeds":crate::content::SEEDS,"random_generator":"splitmix64-low-byte-v1",
        "build":{"toolchain":"1.89.0","profile":"release","opt_level":3,"codegen_units":1,"lto":false,"target_cpu":"native"},
        "timeout_seconds":600,"fresh_case_timeout_seconds":60,
        "timing":{"ordinary":"public workflow through owned output; validation outside timer","diagnostic":"same workload with stage and resource observation; never pooled with ordinary","operator":"one prepared public operation on each independently initialized instance","first_use":"one fresh process per sample; process startup excluded"},
        "source_counts":crate::content::SOURCE_COUNTS,"operator_widths":crate::content::WIDTHS,
        "case_count":checks.len(),"repetitions":crate::content::REPETITIONS});
    let digest = util::digest(&(
        &test.version,
        &definition,
        &checks,
        CONTENT,
        COMMON,
        REFERENCE,
        WORKLOADS,
        TELEMETRY,
        include_str!("measurement.rs"),
    ));
    Ok(Recipe {
        identity: Identity {
            id: test.version.clone(),
            digest,
        },
        purpose: "formal".into(),
        cases: checks
            .iter()
            .filter(|(_, c)| c.measured)
            .map(|(k, _)| k.clone())
            .collect(),
        warmups: 1,
        repetitions: crate::content::REPETITIONS,
        definition,
        checks,
    })
}
pub fn interface(family: &str) -> Identity {
    Identity {
        id: format!("{family}@0.1.0"),
        digest: util::digest(&(
            family,
            capabilities(family),
            OPERATOR,
            SCALAR_OPERATOR,
            "single-component-v1: engine=plan-build/export; operator=operator-surface",
            if family == "product-abi" {
                PRODUCT
            } else {
                NATIVE
            },
            include_str!("build.rs"),
        )),
    }
}
pub fn family(engine: &Package) -> Result<String> {
    match (
        engine.name.as_str(),
        semver::Version::parse(&engine.version)
            .map_err(|e| e.to_string())?
            .major,
    ) {
        ("fountain_engine", 2) if engine.version.starts_with("2.1.") => Ok("native-2".into()),
        ("fountain_engine", 3) => Ok("native-3".into()),
        ("fountain_bin_ffi", _) => Ok("product-abi".into()),
        _ => Err(format!(
            "unsupported: no interface for {} {}",
            engine.name, engine.version
        )),
    }
}
pub fn implementation(package: &Package, selected: Option<&str>) -> Result<String> {
    match (package.name.as_str(), selected) {
        ("fountain-arena-operator", None | Some("arena")) => Ok("arena".into()),
        ("fountain_operators", Some("slab" | "simd")) => Ok(selected.unwrap().into()),
        ("fountain_operators", None) => {
            Err("unsupported: fountain_operators requires #slab or #simd".into())
        }
        _ => Err("unsupported: unknown operator implementation".into()),
    }
}
/// Reject changed content under a registered identity before executing or comparing it.
pub fn verify_lock() -> Result<()> {
    let locked: Value = serde_json::from_str(include_str!("../specs/catalog-lock.json"))
        .map_err(|e| e.to_string())?;
    if lock_entries() != locked {
        return Err(
            "interface/test identity conflict: catalog content differs from the candidate lock"
                .into(),
        );
    }
    Ok(())
}
pub fn capabilities(family: &str) -> Value {
    json!({"operator_surface":matches!(family,"native-2"|"native-3"),
        "callback_operator":matches!(family,"native-2"|"native-3"|"product-abi"),
        "codec":matches!(family,"native-2"|"native-3"|"product-abi"),
        "plan":matches!(family,"native-2"|"native-3"),"reuse":true,
        "alias":family=="native-3","internal_profile":family=="native-3","source_k_domain":if family=="product-abi"{json!({"rq":[1,56403],"r10":[4,8192]})}else{json!({"r10":[4,8192]})}})
}
pub fn validate_scheme(scheme: &SchemeSelection) -> Result<()> {
    if !matches!(scheme.name.as_str(), "rq" | "r10") || scheme.mode != "systematic" {
        return Err(format!(
            "unsupported: unregistered scheme {} / {}; current integrations provide rq and r10 systematic",
            scheme.name, scheme.mode
        ));
    }
    Ok(())
}
/// Expand the complete catalog. Every inapplicable case remains present with its reason.
pub fn plan(
    test: &TestReference,
    scheme: &SchemeSelection,
    family: &str,
    scope: Scope,
) -> Result<Recipe> {
    validate_scheme(scheme)?;
    if !supports(family, &test.version) {
        return Err("unsupported: unknown interface capability set".into());
    }
    let mut recipe = recipe(test)?;
    for check in recipe.checks.values_mut() {
        if check
            .spec
            .scheme
            .as_ref()
            .is_some_and(|s| s != &scheme.name)
        {
            check.skip = Some(crate::coverage::Skip {
                code: "scheme_not_selected".into(),
                reason: format!(
                    "Configured scheme is {}; this case requires {}",
                    scheme.name,
                    check.spec.scheme.as_deref().unwrap()
                ),
            });
        } else if (scope == Scope::Engine
            && !matches!(check.spec.workload.as_str(), "plan-build" | "plan-export"))
            || (scope == Scope::Operator && check.spec.scheme.is_some())
        {
            check.skip = Some(crate::coverage::Skip {
                code: "component_not_selected".into(),
                reason: if scope == Scope::Engine { "This case requires a selected operator; engine-only C executes standalone planning capabilities" }
                    else { "This case requires a selected engine; the operator's trait dependency is build support, not a tested codec" }.into(),
            });
        } else if check.spec.scheme.as_deref() == Some("r10") && check.spec.k < 4 {
            check.skip=Some(crate::coverage::Skip{code:"outside_api_domain".into(),reason:"The registered R10 scheme accepts source K in 4..=8192; product invalid-input rejection is checked in C11, and native invalid-input behavior has no recoverable API contract".into()});
        } else if let Some(required) = check
            .spec
            .requires
            .iter()
            .find(|name| capabilities(family)[name.as_str()] != true)
        {
            check.skip = Some(crate::coverage::Skip {
                code: "capability_unavailable".into(),
                reason: format!(
                    "{family} does not expose {required}; fused work remains in the corresponding codec parent"
                ),
            });
        }
    }
    recipe.cases = recipe
        .checks
        .iter()
        .filter(|(_, c)| c.skip.is_none() && c.measured)
        .map(|(name, _)| name.clone())
        .collect();
    Ok(recipe)
}
pub fn supports(family: &str, test: &str) -> bool {
    test == "0.1.0" && matches!(family, "native-2" | "native-3" | "product-abi")
}
pub fn registered_interface(identity: &Identity, test: &str) -> bool {
    let family = identity.id.split('@').next().unwrap_or("");
    supports(family, test) && identity == &interface(family)
}
pub fn list() -> Value {
    let interfaces = ["native-2", "native-3", "product-abi"].map(interface);
    json!({"format_version":1,"publication_status":"internal-draft","interfaces":interfaces,"test_versions":[recipe(&"0.1.0".parse().unwrap()).unwrap()],"schemes":[{"name":"rq","modes":["systematic"]},{"name":"r10","modes":["systematic"]}]})
}

/// Compact candidate seal; the T digest covers every expanded case and semantic source.
pub fn lock_entries() -> Value {
    json!({"format_version":1,"publication_status":"internal-draft","interfaces":(["native-2","native-3","product-abi"].map(interface)),"tests":[recipe(&"0.1.0".parse().unwrap()).unwrap().identity]})
}
