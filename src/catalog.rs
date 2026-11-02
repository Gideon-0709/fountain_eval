use crate::validation::{Check, Severity};
use crate::{
    protocol::{SchemeSelection, TestReference},
    source::Package,
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const COMMON: &str = include_str!("../drivers/common.rs");
pub const OPERATOR: &str = include_str!("../drivers/operator.rs");
pub const NATIVE: &str = include_str!("../drivers/native.rs");
pub const PRODUCT: &str = include_str!("../drivers/product.rs");
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
    let id = test.version.clone();
    if id != "0.1.0" {
        return Err(format!("unsupported: unknown test content version {id}"));
    }
    let mut checks = BTreeMap::new();
    {
        for width in [1, 31, 128] {
            for mode in ["execute", "direct"] {
                checks.insert(format!("operator/{width}/{mode}"), Check {skip:None, measurement_skip:None, severity:Severity::Fatal, rule:"operator-core".into(), description:"Distinct-ID scalar reference, copy isolation, move, zero and GF arithmetic".into(), measured:true, payload_bytes:0, operations:14});
            }
        }
    }
    {
        for codec in ["rq", "r10"] {
            for missing in [0, 8] {
                for phase in ["encode", "decode"] {
                    checks.insert(format!("codec/{codec}/missing-{missing}/{phase}"), Check {skip:None, measurement_skip:None, severity:Severity::Fatal, rule:"codec-payload".into(), description:"Payload equality, incomplete state, operator participation and resource release".into(), measured:true, payload_bytes:8192, operations:0});
                }
            }
        }
    }
    for width in [1, 31, 128] {
        for mode in ["execute", "direct"] {
            for rule in ["self-xor", "duplicate-source-xor"] {
                checks.insert(format!("operator/{width}/{mode}/{rule}"),Check {skip:None, measurement_skip:None, severity:Severity::Warning, rule:rule.into(), description:if rule=="self-xor" {"Same-ID XOR is compared with the zero result of the scalar reference"} else {"A repeated source in AddTwo is compared with cancellation in the scalar reference"}.into(), measured:true, payload_bytes:0, operations:256});
            }
        }
    }
    let cases = checks
        .iter()
        .filter(|(_, c)| c.measured)
        .map(|(name, _)| name.clone())
        .collect();
    let definition = json!({"purpose":"acceptance","selection":"all applicable checks; no user test filters","applicability":{"codec":"configured scheme","operator_core":"complete_operator_surface","boundary_validation":"callback_operator","boundary_measurement":"complete_operator_surface"},"k":64,"symbol_bytes":128,"repairs":128,"operator_widths":[1,31,128],"boundary_calls_per_sample":256,"field_polynomial":285,"correctness":{"fatal":"required core behavior, codec payload, lifecycle and evidence integrity","warning":"self-xor and duplicate-source-xor reference deviations","warning_effect":"reported with performance; no rejection or cross-component output-equivalence requirement","scope":"selected operator; product diagnostics inspect the callback implementation, not the binary operation surface"},"warmups":1,"repetitions":3,"build":{"toolchain":"1.89.0","profile":"release","target_cpu":"native","opt_level":3,"codegen_units":1,"lto":false},"timing":{"operator":"preloaded operations only; boundary samples call 256 independently initialized operators once each","codec":"construction and loading through owned output","excluded":["validation","serialization","drop"]},"timeout_seconds":60});
    let digest = util::digest(&(
        id.as_str(),
        &definition,
        &checks,
        COMMON,
        include_str!("measurement.rs"),
    ));
    Ok(Recipe {
        identity: Identity { id, digest },
        purpose: "acceptance".into(),
        cases,
        warmups: 1,
        repetitions: 3,
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
    if list() != locked {
        return Err(
            "interface/test identity conflict: catalog content differs from the candidate lock"
                .into(),
        );
    }
    Ok(())
}
pub fn capabilities(family: &str) -> Value {
    json!({"complete_operator_surface":matches!(family,"native-2"|"native-3"),"callback_operator":matches!(family,"native-2"|"native-3"|"product-abi")})
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
pub fn plan(test: &TestReference, scheme: &SchemeSelection, family: &str) -> Result<Recipe> {
    validate_scheme(scheme)?;
    if !supports(family, &test.version) {
        return Err("unsupported: unknown interface capability set".into());
    }
    let mut recipe = recipe(test)?;
    for (case, check) in &mut recipe.checks {
        if case.starts_with("codec/") && case.split('/').nth(1) != Some(scheme.name.as_str()) {
            check.skip = Some(crate::coverage::Skip {
                code: "scheme_not_selected".into(),
                reason: format!(
                    "Configured scheme is {} / {}; this case requires {}",
                    scheme.name,
                    scheme.mode,
                    case.split('/').nth(1).unwrap()
                ),
            });
        } else if capabilities(family)["complete_operator_surface"] == false
            && case.starts_with("operator/")
        {
            let reason = crate::coverage::Skip {code:"capability_unavailable".into(),reason:"Product ABI does not expose the complete operation surface; callback boundary diagnostics remain available outside codec timing".into()};
            if check.severity == Severity::Fatal {
                check.skip = Some(reason);
            } else {
                check.measured = false;
                check.measurement_skip = Some(reason);
            }
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
