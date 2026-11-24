#[test]
fn catalog_matches_the_release_lock() {
    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("../specs/catalog-lock.json")).unwrap();
    assert_eq!(
        fountain_eval::catalog::lock_entries(),
        frozen,
        "Catalog content changed under a released interface or test identity"
    );
    assert_eq!(
        fountain_eval::catalog::list()["publication_status"],
        frozen["publication_status"]
    );
}

#[test]
fn measurements_use_direct_calls_with_alternate_routes_retained_for_validation() {
    let cases = fountain_eval::content::cases();
    let measured: Vec<_> = cases
        .iter()
        .filter(|c| c.category == "operation" && c.measured)
        .collect();
    assert_eq!(measured.len(), 504);
    let widths: std::collections::BTreeSet<_> = measured.iter().map(|c| c.width).collect();
    assert_eq!(widths, [128, 256, 512, 1024, 2048, 4096].into());
    assert!(
        measured
            .iter()
            .all(|c| c.direct && c.id.contains("/direct/"))
    );
    assert!(cases.iter().any(|c| c.family == "V05" && !c.measured));
    assert!(
        cases
            .iter()
            .any(|c| c.family == "V06" && !c.measured && c.id.ends_with("/execute"))
    );
}

#[test]
fn plans_and_replay_cover_the_same_block_sizes() {
    let cases = fountain_eval::content::cases();
    for scheme in ["rq", "r10"] {
        let links: Vec<_> = cases
            .iter()
            .filter(|c| c.category == "chain" && c.scheme.as_deref() == Some(scheme))
            .collect();
        assert_eq!(links.len(), 112);
        for k in [64usize, 128, 256, 512, 1024, 2048, 4096, 8192] {
            for (family, widths) in [
                ("L02", &[0][..]),
                ("L03", &[0][..]),
                ("L11", &[128, 256, 512, 1024, 2048, 4096][..]),
                ("L12", &[128, 256, 512, 1024, 2048, 4096][..]),
            ] {
                for width in widths {
                    let mut cases = links
                        .iter()
                        .filter(|c| c.family == family && c.k == k && c.width == *width);
                    let c = cases.next().unwrap();
                    assert!(c.measured && cases.next().is_none());
                    if family == "L11" {
                        assert_eq!(c.repairs, 8);
                    }
                    if family == "L12" {
                        assert_eq!(c.missing, k.div_ceil(10));
                        assert_eq!(c.order, "source-first");
                    }
                }
            }
        }
    }
}

#[test]
fn link_and_lifecycle_share_the_corresponding_workflow_measurement() {
    let cases = fountain_eval::content::cases();
    assert!(
        cases
            .iter()
            .filter(|c| c.category == "workflow")
            .all(|c| c.width.is_power_of_two())
    );
    assert!(cases.iter().all(|c| !matches!(
        c.workload.as_str(),
        "encode-chain" | "decode-chain" | "lifecycle"
    )));
    for scheme in ["rq", "r10"] {
        for k in fountain_eval::content::SOURCE_COUNTS {
            for width in fountain_eval::content::SYMBOL_WIDTHS {
                for workload in [
                    "first-encode",
                    "encode-sources",
                    "encode-repairs",
                    "reuse-encode",
                    "decode",
                    "round-trip",
                    "continuous-rebuild",
                    "continuous-reuse",
                ] {
                    assert!(
                        cases.iter().any(|c| {
                            c.scheme.as_deref() == Some(scheme)
                                && c.category == "workflow"
                                && c.k == k
                                && c.width == width
                                && c.workload == workload
                        }),
                        "{scheme}/{workload}/k{k}/b{width}"
                    );
                }
                assert!(cases.iter().any(|c| {
                    c.scheme.as_deref() == Some(scheme)
                        && c.family == "V07"
                        && c.k == k
                        && c.width == width
                }));
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
