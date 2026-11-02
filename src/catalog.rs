use crate::validation::{Check, Severity};
use crate::{
    protocol::TestReference,
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
        self.checks.keys().cloned().collect()
    }
}
pub fn recipe(test: &TestReference) -> Result<Recipe> {
    let id = format!("{}@{}", test.name, test.version);
    let operator = match id.as_str() {
        "operator-smoke@0.1.0" => true,
        "codec-smoke@0.1.0" => false,
        _ => return Err(format!("unsupported: unknown test {id}")),
    };
    let mut checks = BTreeMap::new();
    if operator {
        for width in [1, 31, 128] {
            for mode in ["execute", "direct"] {
                checks.insert(format!("operator/{width}/{mode}"), Check {severity:Severity::Fatal, rule:"operator-core".into(), description:"Distinct-ID scalar reference, copy isolation, move, zero and GF arithmetic".into(), measured:true, payload_bytes:0, operations:14});
            }
        }
    } else {
        for codec in ["rq", "r10"] {
            for missing in [0, 8] {
                for phase in ["encode", "decode"] {
                    checks.insert(format!("codec/{codec}/missing-{missing}/{phase}"), Check {severity:Severity::Fatal, rule:"codec-payload".into(), description:"Payload equality, incomplete state, operator participation and resource release".into(), measured:true, payload_bytes:8192, operations:0});
                }
            }
        }
    }
    for width in [1, 31, 128] {
        for mode in ["execute", "direct"] {
            for rule in ["self-xor", "duplicate-source-xor"] {
                checks.insert(format!("operator/{width}/{mode}/{rule}"),Check {severity:Severity::Warning, rule:rule.into(), description:if rule=="self-xor" {"Same-ID XOR is compared with the zero result of the scalar reference"} else {"A repeated source in AddTwo is compared with cancellation in the scalar reference"}.into(), measured:operator, payload_bytes:0, operations:if operator {256} else {0}});
            }
        }
    }
    let cases = checks
        .iter()
        .filter(|(_, c)| c.measured)
        .map(|(name, _)| name.clone())
        .collect();
    let definition = json!({"purpose":"acceptance","k":64,"symbol_bytes":128,"repairs":128,"operator_widths":[1,31,128],"boundary_calls_per_sample":256,"field_polynomial":285,"correctness":{"fatal":"required core behavior, codec payload, lifecycle and evidence integrity","warning":"self-xor and duplicate-source-xor reference deviations","warning_effect":"reported with performance; no rejection or cross-component output-equivalence requirement","scope":"selected operator; product diagnostics inspect the callback implementation, not the binary operation surface"},"warmups":1,"repetitions":3,"build":{"toolchain":"1.89.0","profile":"release","target_cpu":"native","opt_level":3,"codegen_units":1,"lto":false},"timing":{"operator":"preloaded operations only; boundary samples call 256 independently initialized operators once each","codec":"construction and loading through owned output","excluded":["validation","serialization","drop"]},"timeout_seconds":60});
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
pub fn supports(family: &str, test: &str) -> bool {
    let known = matches!(test, "operator-smoke@0.1.0" | "codec-smoke@0.1.0");
    known
        && (matches!(family, "native-2" | "native-3")
            || family == "product-abi" && test.starts_with("codec-smoke@"))
}
pub fn registered_interface(identity: &Identity, test: &str) -> bool {
    let family = identity.id.split('@').next().unwrap_or("");
    supports(family, test) && identity == &interface(family)
}
pub fn list() -> Value {
    let interfaces = ["native-2", "native-3", "product-abi"].map(interface);
    let tests =
        ["operator-smoke@0.1.0", "codec-smoke@0.1.0"].map(|t| recipe(&t.parse().unwrap()).unwrap());
    json!({"format_version":1,"publication_status":"internal-draft","interfaces":interfaces,"tests":tests})
}
