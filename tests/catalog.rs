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
