use fountain_eval::protocol::{
    ComponentReference, EvaluationRequest, SourceReference, TestReference,
};

const REV: &str = "d777cad6b512ee17663e129512bae10bca3397b1";

#[test]
fn references_preserve_source_and_implementation_selection() {
    let local: ComponentReference = "path:../local checkout/Cargo.toml#arena".parse().unwrap();
    assert_eq!(local.implementation.as_deref(), Some("arena"));
    assert_eq!(
        local.source,
        SourceReference::Path {
            path: "../local checkout/Cargo.toml".into()
        }
    );

    let registry: ComponentReference = "registry:fountain_engine@3.0.0-dev".parse().unwrap();
    assert_eq!(
        registry.source,
        SourceReference::Registry {
            registry: "crates-io".into(),
            package: "fountain_engine".into(),
            version: "3.0.0-dev".into(),
        }
    );

    for url in [
        "https://example.org/team/mono.git",
        "ssh://git@example.org/team/mono.git",
    ] {
        let reference: ComponentReference = format!(
            "git+{url}?package=fountain_operators&rev={}#slab",
            REV.to_uppercase()
        )
        .parse()
        .unwrap();
        assert_eq!(
            reference.source,
            SourceReference::Git {
                url: url.into(),
                revision: REV.into(),
                package: "fountain_operators".into()
            }
        );
        assert_eq!(reference.implementation.as_deref(), Some("slab"));
    }
    let sha256 = format!(
        "git+https://example.org/repo?rev={}&package=engine",
        "a".repeat(64)
    );
    assert!(sha256.parse::<ComponentReference>().is_ok());
}

#[test]
fn mutable_ambiguous_and_credential_bearing_sources_are_rejected() {
    let invalid = [
        "../engine".to_owned(),
        "path:".into(),
        "path:bad\0path".into(),
        "path:engine#".into(),
        "path:engine#one#two".into(),
        "registry:engine@latest".into(),
        "registry:engine@^2.1.0".into(),
        "registry:engine@2.1".into(),
        "registry:@2.1.0".into(),
        "git+https://example.org/repo?rev=main&package=engine".into(),
        "git+https://example.org/repo?rev=cd504f5&package=engine".into(),
        format!("git+https://example.org/repo?rev={REV}"),
        format!("git+https://example.org/repo?rev={REV}&package=engine&package=other"),
        format!("git+https://example.org/repo?rev={REV}&package=engine&branch=main"),
        format!("git+https://user:secret@example.org/repo?rev={REV}&package=engine"),
        format!("git+ssh://git:secret@example.org/repo?rev={REV}&package=engine"),
        format!("git+http://example.org/repo?rev={REV}&package=engine"),
        format!("git+file:///repo?rev={REV}&package=engine"),
    ];
    for input in invalid {
        assert!(
            input.parse::<ComponentReference>().is_err(),
            "accepted {input}"
        );
    }
}

#[test]
fn test_versions_are_exact_and_requests_have_only_two_component_roles() {
    for invalid in ["core", "core@latest", "core@^1.0.0", "core@1.0", "@1.0.0"] {
        assert!(invalid.parse::<TestReference>().is_err());
    }
    let request = EvaluationRequest::parse(
        "registry:engine@3.0.0-dev",
        "path:../operator#slab",
        "example-unregistered@0.1.0",
    )
    .unwrap();
    let json = serde_json::to_value(request).unwrap();
    let components = json["components"].as_object().unwrap();
    assert_eq!(components.len(), 2);
    assert!(components.contains_key("engine") && components.contains_key("operator"));
    assert!(json.get("interface").is_none() && json.get("environment").is_none());
}
