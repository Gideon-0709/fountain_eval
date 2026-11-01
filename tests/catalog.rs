#[test]
fn published_catalog_content_is_immutable() {
    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("../specs/catalog-lock.json")).unwrap();
    assert_eq!(
        fountain_eval::catalog::list(),
        frozen,
        "Registered I/T content changed: introduce a new version rather than repointing an existing identity"
    );
}
