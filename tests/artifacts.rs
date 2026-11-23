mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, write::ZlibEncoder};
use fountain_eval::{artifact, batch, config::Configuration, evidence, report, util, validation};
use std::{collections::BTreeMap, fs, path::PathBuf};
use support::Temp;

fn raw(t: &Temp) -> PathBuf {
    let mut r = support::record("native-2", "rq");
    let config =
        Configuration::parse(t.0.join("eval.toml"), r.configuration.original.clone()).unwrap();
    r.configuration = config.selection(&config.selections[0]);
    r.correctness = validation::report(&r);
    r.coverage = fountain_eval::coverage::report(&r);
    let dir = t.0.join("runs/fixture-batch");
    let child = dir.join("C0/fixture");
    fs::create_dir_all(&child).unwrap();
    evidence::save(&child, &r).unwrap();
    evidence::seal(&child).unwrap();
    let entry = batch::Entry {
        component_id: 0,
        request: r.request.clone(),
        status: r.status.clone(),
        directory: Some("C0/fixture".into()),
        elapsed_ns: Some(1),
        phases_ns: BTreeMap::new(),
        validation_observations: r.validation.len(),
        warnings: r.correctness.warnings.len(),
        samples: r.samples.len(),
        diagnostics: r.diagnostics.len(),
        skipped: r.coverage["skipped_validation"].as_u64().unwrap() as usize,
        error: None,
    };
    let b = batch::Record {
        format_version: 1,
        run_id: "fixture-batch".into(),
        action: "run".into(),
        status: "succeeded".into(),
        interrupted: false,
        started_unix_ms: 0,
        elapsed_ns: 1,
        configuration: config,
        entries: vec![entry],
    };
    util::write_json(&dir.join("batch.json"), &b).unwrap();
    let cache = t.0.join(".fountain-eval/fixture-batch");
    fs::create_dir_all(&cache).unwrap();
    util::write_json(
        &cache.join("owner.json"),
        &serde_json::json!({"run_id":"fixture-batch","directory":dir.canonicalize().unwrap()}),
    )
    .unwrap();
    util::write_json(
        &dir.join("workspace.json"),
        &serde_json::json!({"cache":cache}),
    )
    .unwrap();
    t.write(
        ".fountain-eval/fixture-batch/builds/driver",
        "binary fixture",
    );
    evidence::seal(&dir).unwrap();
    dir
}
#[test]
fn report_survives_cleanup_and_comparison_keeps_all_cases() {
    let t = Temp::new();
    let raw = raw(&t);
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    let body = report::snapshot(&saved);
    let path = artifact::publish(&t.0.join("reports/fixture-batch"), &body, &saved).unwrap();
    let mut old_index = ZlibEncoder::new(Vec::new(), Compression::default());
    serde_json::to_writer(&mut old_index, &saved).unwrap();
    let payload = STANDARD.encode(old_index.finish().unwrap());
    let checksum = util::hash(format!("{body}{payload}"));
    let legacy = t.write(
        "legacy.md",
        &format!("{body}\n<!-- fountain-eval-result\n{checksum}\n{payload}\n-->\n"),
    );
    let (old_body, old_snapshot) = artifact::load(&legacy).unwrap();
    assert_eq!(old_body, body);
    assert_eq!(util::digest(&old_snapshot), util::digest(&saved));
    let cache = t.write(".fountain-eval/keep", "cache");
    let index = path.join("index.json.lzma");
    let away = t.0.join("index-held");
    fs::rename(&index, &away).unwrap();
    assert!(
        artifact::cleanup(&raw, &path)
            .unwrap_err()
            .contains("report index missing")
    );
    assert!(raw.is_dir() && t.0.join(".fountain-eval/fixture-batch").is_dir());
    fs::rename(away, index).unwrap();
    artifact::cleanup(&raw, &path).unwrap();
    assert!(!raw.exists());
    assert!(cache.exists()); // An unrelated sibling is not owned by this run.
    assert!(!t.0.join(".fountain-eval/fixture-batch").exists());
    let (_, stored) = artifact::load(&path).unwrap();
    let c = artifact::select(&path).unwrap();
    let comparison = evidence::compare_summaries(&c, &c).unwrap();
    assert_eq!(
        comparison["rows"].as_array().unwrap().len(),
        c.summary.len()
    );
    assert_eq!(stored.components[0].coverage, saved.components[0].coverage);
    assert!(
        serde_json::to_value(&stored.components[0])
            .unwrap()
            .get("samples")
            .is_none()
    );
    fs::remove_dir_all(t.0.join(".fountain-eval")).unwrap();
    assert!(artifact::load(&path).is_ok());
    let mut different = c.clone();
    different.quadruple.as_mut().unwrap().e.conditions["runtime"]["cpu"] = "different".into();
    assert!(evidence::compare_summaries(&c, &different).is_err());
}
#[test]
fn publication_failure_changed_evidence_and_overlap_never_delete_raw_input() {
    let t = Temp::new();
    let raw = raw(&t);
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    let body = "# Report\n\nRun: `fixture-batch`\n";
    let directory = t.0.join("reports/fixture-batch");
    assert!(artifact::publish(&directory, "", &saved).is_err());
    assert!(!directory.exists());
    assert!(raw.exists());
    let path = artifact::publish(&directory, body, &saved).unwrap();
    assert!(artifact::publish(&directory, body, &saved).is_err());
    assert!(raw.exists());
    let inside = artifact::publish(&raw.join("nested-report"), body, &saved).unwrap();
    assert!(artifact::cleanup(&raw, &inside).is_err());
    assert!(raw.exists());
    fs::write(raw.join("batch.json"), "changed").unwrap();
    assert!(artifact::cleanup(&raw, &path).is_err());
    assert!(raw.exists());
    assert!(artifact::load(&path).is_ok());
}
#[test]
fn report_checksums_bind_both_files_and_publication_is_immutable() {
    let t = Temp::new();
    let raw = raw(&t);
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    let body = "# Report\n\nRun: `fixture-batch`\n";
    let path = artifact::publish(&t.0.join("reports"), body, &saved).unwrap();
    let markdown = path.join("report.md");
    let index = path.join("index.json.lzma");
    assert_eq!(fs::read_dir(&path).unwrap().count(), 2);
    assert_eq!(fs::read_to_string(&markdown).unwrap(), body);
    assert_eq!(artifact::load(&markdown).unwrap().0, body);
    let original = fs::read(&index).unwrap();

    fs::write(&markdown, body.replace("# Report", "# Changed")).unwrap();
    assert!(artifact::load(&path).unwrap_err().contains("checksum"));
    fs::write(&markdown, body).unwrap();
    fs::write(&index, &original[..original.len() / 2]).unwrap();
    assert!(artifact::load(&path).is_err());
    fs::write(&index, &original).unwrap();
    assert!(artifact::publish(&path, "# Replacement\n", &saved).is_err());
    assert_eq!(fs::read_to_string(&markdown).unwrap(), body);
    assert_eq!(fs::read(&index).unwrap(), original);
}

#[test]
fn failed_invalid_sample_prefix_still_has_a_publishable_failure_report() {
    let t = Temp::new();
    let raw = raw(&t);
    let child = raw.join("C0/fixture");
    let mut r = evidence::load(&child).unwrap();
    r.status = "failed".into();
    r.stage = "measure".into();
    r.samples[0].elapsed_ns = 0;
    r.error = Some("invalid sample clock".into());
    r.summary.clear();
    r.correctness = validation::report(&r);
    evidence::save(&child, &r).unwrap();
    evidence::seal(&child).unwrap();
    let mut b: batch::Record = util::read_json(&raw.join("batch.json")).unwrap();
    b.status = "completed_with_failures".into();
    b.entries[0].status = "failed".into();
    b.entries[0].error = r.error.clone();
    util::write_json(&raw.join("batch.json"), &b).unwrap();
    evidence::seal(&raw).unwrap();
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    assert!(saved.components[0].aggregation_error.is_some());
    assert!(saved.components[0].summary.is_empty());
    let body = report::snapshot(&saved);
    assert!(body.contains("Sample aggregation issue"));
    let path = artifact::publish(&t.0.join("reports"), &body, &saved).unwrap();
    artifact::cleanup(&raw, &path).unwrap();
    assert_eq!(
        artifact::load(&path).unwrap().1.components[0].status,
        "failed"
    );
}

#[test]
fn cache_ownership_and_active_workspace_block_cleanup() {
    let t = Temp::new();
    let raw = raw(&t);
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    let path = artifact::publish(&t.0.join("reports"), "# Report\n", &saved).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(raw.join("lease.tmp"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(
        artifact::cleanup(&raw, &path)
            .unwrap_err()
            .contains("in use")
    );
    drop(lock);
    t.write(".fountain-eval/fixture-batch/owner.json", "{}");
    assert!(
        artifact::cleanup(&raw, &path)
            .unwrap_err()
            .contains("ownership")
    );
    assert!(raw.exists());
    assert!(
        t.0.join(".fountain-eval/fixture-batch/builds/driver")
            .exists()
    );
}
