use crate::{
    protocol::TestReference,
    source::Package,
    util::{self, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
}
pub fn recipe(test: &TestReference) -> Result<Recipe> {
    let id = format!("{}@{}", test.name, test.version);
    let cases = match id.as_str() {
        "operator-smoke@0.1.0" => [1, 31, 128]
            .into_iter()
            .flat_map(|width| {
                ["execute", "direct"].map(move |mode| format!("operator/{width}/{mode}"))
            })
            .collect(),
        "codec-smoke@0.1.0" => ["rq", "r10"]
            .into_iter()
            .flat_map(|codec| {
                [0, 8].into_iter().flat_map(move |missing| {
                    ["encode", "decode"]
                        .map(move |phase| format!("codec/{codec}/missing-{missing}/{phase}"))
                })
            })
            .collect(),
        _ => return Err(format!("unsupported: unknown test {id}")),
    };
    let definition = json!({"purpose":"acceptance","k":64,"symbol_bytes":128,"repairs":128,"operator_widths":[1,31,128],"field_polynomial":285,"self_target_arithmetic":"excluded pending team contract; distinct IDs and copy isolation are checked","warmups":1,"repetitions":3,"build":{"toolchain":"1.89.0","profile":"release","target_cpu":"native","opt_level":3,"codegen_units":1,"lto":false},"timing":{"operator":"preloaded operations only","codec":"construction and loading through owned output","excluded":["validation","serialization","drop"]},"timeout_seconds":60});
    let digest = util::digest(&(
        id.as_str(),
        &definition,
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
            "interface/test identity conflict: catalog content differs from the published lock"
                .into(),
        );
    }
    Ok(())
}
pub fn supports(family: &str, test: &str) -> bool {
    matches!(family, "native-2" | "native-3")
        && matches!(test, "operator-smoke@0.1.0" | "codec-smoke@0.1.0")
        || family == "product-abi" && test == "codec-smoke@0.1.0"
}
pub fn list() -> Value {
    json!({"format_version":1,"interfaces":[interface("native-2"),interface("native-3"),interface("product-abi")],"tests":[recipe(&"operator-smoke@0.1.0".parse().unwrap()).unwrap(),recipe(&"codec-smoke@0.1.0".parse().unwrap()).unwrap()]})
}
