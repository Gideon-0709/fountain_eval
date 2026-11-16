#[test]
fn catalog_matches_the_explicit_internal_candidate() {
    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("../specs/catalog-lock.json")).unwrap();
    assert_eq!(
        fountain_eval::catalog::lock_entries(),
        frozen,
        "Catalog content changed: update this internal candidate deliberately; published identities will be frozen at release"
    );
}

#[test]
fn link_and_lifecycle_share_the_corresponding_workflow_measurement() {
    let cases = fountain_eval::content::cases();
    assert!(cases.iter().all(|c| !matches!(
        c.workload.as_str(),
        "encode-chain" | "decode-chain" | "lifecycle"
    )));
    for scheme in ["rq", "r10"] {
        for k in [64, 512, 5008] {
            for width in [128, 1016] {
                let mut encode = cases.iter().filter(|c| {
                    c.scheme.as_deref() == Some(scheme)
                        && c.k == k
                        && c.width == width
                        && c.family.split(',').any(|f| f == "L04")
                });
                let c = encode.next().unwrap();
                assert_eq!(c.workload, "encode-repairs");
                assert_eq!(c.repairs, 8);
                assert!(c.family.split(',').any(|f| f == "W08"));
                assert!(encode.next().is_none());
                let c = cases
                    .iter()
                    .find(|c| {
                        c.scheme.as_deref() == Some(scheme)
                            && c.k == k
                            && c.width == width
                            && c.family.split(',').any(|f| f == "L07")
                    })
                    .unwrap();
                assert_eq!(c.workload, "decode");
                assert_eq!(c.missing, k.div_ceil(10));
                assert!(c.family.split(',').any(|f| f == "L01"));
            }
        }
    }
    assert!(
        cases
            .iter()
            .filter(|c| c.workload.ends_with("-replay"))
            .all(|c| c.payload_bytes() == 0)
    );
}
