mod support;
use fountain_eval::{catalog, source::Resolver, util};
use std::{collections::BTreeMap, fs, process::Command};
use support::Temp;
fn package(t: &Temp, dir: &str, name: &str, version: &str) -> std::path::PathBuf {
    t.write(
        &format!("{dir}/Cargo.toml"),
        &format!("[package]\nname='{name}'\nversion='{version}'\nedition='2024'\n"),
    );
    t.write(&format!("{dir}/src/lib.rs"), "pub const VALUE:u8=1;");
    t.0.join(dir)
}
fn git(root: &std::path::Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn dirty_and_untracked_bytes_are_frozen_and_original_edits_do_not_propagate() {
    let t = Temp::new();
    let path = package(&t, "input", "fountain_engine", "2.1.0");
    git(&path, &["init", "--quiet"]);
    git(&path, &["add", "."]);
    git(
        &path,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    fs::write(path.join("src/lib.rs"), "dirty bytes").unwrap();
    fs::write(path.join("untracked.txt"), "untracked").unwrap();
    let frozen = t.resolver().local(&path).unwrap();
    assert_eq!(frozen.source.origin["dirty"], true);
    assert_eq!(
        fs::read_to_string(frozen.source.root.join("src/lib.rs")).unwrap(),
        "dirty bytes"
    );
    assert!(frozen.source.root.join("untracked.txt").exists());
    fs::write(path.join("src/lib.rs"), "later bytes").unwrap();
    assert_eq!(
        fs::read_to_string(frozen.source.root.join("src/lib.rs")).unwrap(),
        "dirty bytes"
    );
    assert_ne!(
        t.resolver().local(&path).unwrap().source.digest,
        frozen.source.digest
    );
    fs::write(frozen.source.root.join("src/lib.rs"), "corrupt cache").unwrap();
    fs::write(path.join("src/lib.rs"), "dirty bytes").unwrap();
    assert!(t.resolver().local(&path).unwrap_err().contains("checksum"));
}
#[test]
fn workspace_inheritance_and_external_paths_are_part_of_the_frozen_closure() {
    let t = Temp::new();
    let helper = package(&t, "helper", "helper", "1.0.0");
    t.write("workspace/Cargo.toml","[workspace]\nmembers=['engine']\n[workspace.package]\nversion='3.0.0-dev'\n[workspace.dependencies]\nhelper={path='../helper',version='1'}\n");
    let manifest=t.write("workspace/engine/Cargo.toml","[package]\nname='fountain_engine'\nversion.workspace=true\n[dependencies]\nhelper.workspace=true\n");
    t.write("workspace/engine/src/lib.rs", "pub use helper::*;");
    let capture = || {
        let mut resolver = t.resolver();
        let p = resolver.local(&manifest).unwrap();
        assert_eq!(p.version, "3.0.0-dev");
        resolver.capture_paths(&[p], &BTreeMap::new()).unwrap();
        resolver.identities()
    };
    let first = capture();
    assert_eq!(first.len(), 2);
    fs::write(helper.join("src/lib.rs"), "pub const VALUE:u8=2;").unwrap();
    assert_ne!(first, capture());
}
#[test]
fn unregistered_and_ambiguous_interfaces_are_rejected() {
    let t = Temp::new();
    let path = package(&t, "engine", "fountain_engine", "4.0.0");
    let engine = t.resolver().local(&path).unwrap();
    assert!(
        catalog::family(&engine)
            .unwrap_err()
            .contains("unsupported")
    );
    let path = package(&t, "operator", "fountain_operators", "2.0.0");
    let op = t.resolver().local(&path).unwrap();
    assert!(catalog::implementation(&op, None).is_err());
    assert_eq!(catalog::implementation(&op, Some("simd")).unwrap(), "simd");
}
#[cfg(unix)]
#[test]
fn escaping_and_directory_symlinks_are_not_silently_snapshotted() {
    let t = Temp::new();
    let path = package(&t, "input", "example", "1.0.0");
    t.write("outside", "not input");
    std::os::unix::fs::symlink(t.0.join("outside"), path.join("escape")).unwrap();
    assert!(t.resolver().local(&path).is_err());
    fs::remove_file(path.join("escape")).unwrap();
    std::os::unix::fs::symlink(&path, path.join("cycle")).unwrap();
    assert!(t.resolver().local(&path).is_err());
}
#[test]
fn canonical_hashes_ignore_map_insertion_order() {
    assert_eq!(
        util::digest(&serde_json::json!({"b":2,"a":1})),
        util::digest(&serde_json::json!({"a":1,"b":2}))
    );
    let _ = std::mem::size_of::<Resolver>();
}

#[test]
fn downloaded_inputs_inside_the_cache_are_not_excluded_from_capture() {
    let t = Temp::new();
    let path = package(&t, "cache/unpacked/package", "fountain_engine", "2.1.0");
    let frozen = t.resolver().local(&path).unwrap();
    assert!(frozen.path().is_file());
    assert!(frozen.source.root.join("src/lib.rs").is_file());
}
