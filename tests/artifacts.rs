mod support;
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
    let path = artifact::publish(&t.0.join("reports/fixture-batch"), &body, &saved, false).unwrap();
    let cache = t.write(".fountain-eval/keep", "cache");
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
    let root = t.0.join("reports");
    let directory = artifact::reserve(&root, "fixture-batch").unwrap();
    assert!(artifact::reserve(&root, "fixture-batch").is_err());
    assert!(artifact::reserve(&root, "../escape").is_err());
    assert!(artifact::publish(&directory, "", &saved, false).is_err());
    assert!(!directory.join("report.md").exists());
    assert!(raw.exists());
    let path = artifact::publish(&directory, body, &saved, false).unwrap();
    assert!(artifact::publish(&directory, body, &saved, false).is_err());
    assert!(raw.exists());
    let inside = artifact::publish(&raw.join("nested-report"), body, &saved, false).unwrap();
    assert!(artifact::cleanup(&raw, &inside).is_err());
    assert!(raw.exists());
    fs::write(raw.join("batch.json"), "changed").unwrap();
    assert!(artifact::cleanup(&raw, &path).is_err());
    assert!(raw.exists());
    assert!(artifact::load(&path).is_ok());
}
#[test]
fn report_checksum_covers_visible_text_and_index_and_replacement_is_atomic() {
    let t = Temp::new();
    let raw = raw(&t);
    let saved = artifact::Snapshot::capture(&raw).unwrap();
    let body = "# Report\n\nRun: `fixture-batch`\n";
    let path = artifact::publish(&t.0.join("reports"), body, &saved, false).unwrap();
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.replace("# Report", "# Changed")).unwrap();
    assert!(artifact::load(&path).is_err());
    fs::write(&path, &original).unwrap();
    let replacement = body.to_owned() + "\nUpdated layout.\n";
    artifact::publish(path.parent().unwrap(), &replacement, &saved, true).unwrap();
    assert_eq!(artifact::load(&path).unwrap().0, replacement);
    let current = fs::read(&path).unwrap();
    assert!(artifact::publish(path.parent().unwrap(), "", &saved, true).is_err());
    assert_eq!(fs::read(&path).unwrap(), current);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    let mut truncated = fs::read_to_string(&path).unwrap();
    truncated.truncate(truncated.len() - 5);
    fs::write(&path, truncated).unwrap();
    assert!(artifact::load(&path).is_err());
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
    assert!(body.contains("Partial samples could not be aggregated"));
    let path = artifact::publish(&t.0.join("reports"), &body, &saved, false).unwrap();
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
    let path = artifact::publish(&t.0.join("reports"), "# Report\n", &saved, false).unwrap();
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
