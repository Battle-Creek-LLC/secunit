//! Integration tests for `secunit registry import` and `secunit inventory`.

use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;
use tempfile::TempDir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Minimal live registry: empty controls/, a stub _config.yaml, a single
/// inventory entry. Just enough that `secunit -C <root>` can canonicalize
/// and load successfully.
fn empty_registry() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("controls")).unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();
    write(
        &root.join("inventory.yaml"),
        "source_repos:\n  - name: app-api\n    in_scope_since: 2026-01-01\n    tags: [production]\n",
    );
    dir
}

fn run(args: &[&str]) -> (std::process::Output, String, String) {
    let output = Command::cargo_bin("secunit")
        .unwrap()
        .args(args)
        .output()
        .expect("run secunit");
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    (output, stdout, stderr)
}

const SAMPLE_CONTROL: &str = r#"id: aa-weekly-audit-review
title: Weekly audit log review
policy: security/audit.md
nist: [AU-6]
owner: cto
cadence: weekly
weekday: monday
skill: aa-weekly-audit-review

scope:
  kind: cloud_account
  has_tags: [production]

evidence_required:
  - kind: summary
    description: Review summary
"#;

const SAMPLE_SCHEDULE: &str = "overrides: []\n";
const SAMPLE_CONFIG: &str = "schema_version: 1\nweekly_default_weekday: monday\n";

const SAMPLE_INVENTORY_DRAFT: &str = r#"source_repos:
  - name: app-ui
    in_scope_since: 2026-05-01
    tags: [production]
cloud_accounts:
  - name: prod
    in_scope_since: 2026-05-01
    tags: [production]
"#;

fn write_drafts(run_dir: &Path) {
    let raw = run_dir.join("raw");
    fs::create_dir_all(raw.join("controls")).unwrap();
    write(
        &raw.join("controls/aa-weekly-audit-review.yaml"),
        SAMPLE_CONTROL,
    );
    write(&raw.join("inventory.yaml"), SAMPLE_INVENTORY_DRAFT);
    write(&raw.join("schedule.yaml"), SAMPLE_SCHEDULE);
    write(&raw.join("_config.yaml"), SAMPLE_CONFIG);
}

#[test]
fn registry_import_promotes_drafts() {
    let live = empty_registry();
    let draft = TempDir::new().unwrap();
    write_drafts(draft.path());

    let (output, stdout, stderr) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "registry",
        "import",
        draft.path().to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "import failed: stdout={stdout} stderr={stderr}"
    );

    // Control was promoted.
    let promoted = live.path().join("controls/aa-weekly-audit-review.yaml");
    assert!(promoted.exists(), "control not promoted");
    assert_eq!(fs::read_to_string(&promoted).unwrap(), SAMPLE_CONTROL);

    // schedule.yaml + _config.yaml are top-level files we did not have:
    // import should have added them.
    assert!(live.path().join("schedule.yaml").exists());
    // _config.yaml may or may not have existed; assert it does now.
    assert!(live.path().join("_config.yaml").exists());

    // Inventory: live had `source_repos` with `app-api`; draft has `app-ui`
    // and a new `cloud_accounts` kind. Merge should append `app-ui`,
    // preserve `app-api`, and add the `cloud_accounts` kind wholesale.
    let inv_text = fs::read_to_string(live.path().join("inventory.yaml")).unwrap();
    let inv: serde_yaml::Value = serde_yaml::from_str(&inv_text).unwrap();
    let repos = inv
        .get("source_repos")
        .and_then(|v| v.as_sequence())
        .expect("source_repos");
    let names: Vec<&str> = repos
        .iter()
        .filter_map(|e| e.get("name"))
        .filter_map(|v| v.as_str())
        .collect();
    assert!(names.contains(&"app-api"), "app-api preserved");
    assert!(names.contains(&"app-ui"), "app-ui appended");
    assert!(
        inv.get("cloud_accounts").is_some(),
        "cloud_accounts kind added"
    );
    assert!(stdout.contains("app-ui"), "summary mentions added entry");
}

#[test]
fn registry_import_is_idempotent() {
    let live = empty_registry();
    let draft = TempDir::new().unwrap();
    write_drafts(draft.path());

    // First run.
    let (out1, _, _) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "registry",
        "import",
        draft.path().to_str().unwrap(),
    ]);
    assert!(out1.status.success());

    // Second run — nothing new should be added.
    let (out2, stdout2, _) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "--json",
        "registry",
        "import",
        draft.path().to_str().unwrap(),
    ]);
    assert!(out2.status.success());
    let summary: serde_json::Value = serde_json::from_str(&stdout2).unwrap();
    assert!(
        summary["added_controls"].as_array().unwrap().is_empty(),
        "second run should not add controls again: {stdout2}"
    );
    assert!(
        summary["inventory_added"].as_array().unwrap().is_empty(),
        "second run should not re-add inventory entries: {stdout2}"
    );
    assert_eq!(summary["inventory_created"], false);
    // schedule/_config were created on first run; second run should skip them.
    assert!(summary["added_files"].as_array().unwrap().is_empty());
}

#[test]
fn registry_import_rejects_invalid_drafts() {
    let live = empty_registry();
    let draft = TempDir::new().unwrap();
    let raw = draft.path().join("raw");
    fs::create_dir_all(raw.join("controls")).unwrap();
    // Missing required `cadence` field.
    write(
        &raw.join("controls/bad.yaml"),
        "id: bad\ntitle: Bad\npolicy: x.md\nowner: cto\nskill: bad\n",
    );

    let (output, stdout, _) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "registry",
        "import",
        draft.path().to_str().unwrap(),
    ]);
    // Exit 1 = validation failure per docs/cli.md.
    assert_eq!(output.status.code(), Some(1), "stdout: {stdout}");
    assert!(stdout.contains("Drafts rejected"), "stdout: {stdout}");
    assert!(
        !live.path().join("controls/bad.yaml").exists(),
        "rejected draft must not be promoted"
    );
}

#[test]
fn inventory_list_filters_by_kind() {
    let live = empty_registry();
    let (output, stdout, _) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "inventory",
        "list",
        "--kind",
        "source_repo",
    ]);
    assert!(output.status.success());
    assert!(stdout.contains("source_repos"), "stdout: {stdout}");
    assert!(stdout.contains("app-api"), "stdout: {stdout}");
}

#[test]
fn inventory_add_appends_entry() {
    let live = empty_registry();
    let (output, _, stderr) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "--today",
        "2026-05-01",
        "inventory",
        "add",
        "--kind",
        "source_repo",
        "--name",
        "app-ui",
        "--tags",
        "production",
        "has-sca",
        "--url",
        "github.com/example/app-ui",
    ]);
    assert!(output.status.success(), "stderr: {stderr}");

    let inv: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(live.path().join("inventory.yaml")).unwrap())
            .unwrap();
    let repos = inv["source_repos"].as_sequence().unwrap();
    assert_eq!(repos.len(), 2);
    let added = &repos[1];
    assert_eq!(added["name"].as_str().unwrap(), "app-ui");
    assert_eq!(added["in_scope_since"].as_str().unwrap(), "2026-05-01");
    assert_eq!(added["url"].as_str().unwrap(), "github.com/example/app-ui");
    let tags: Vec<&str> = added["tags"]
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(tags, vec!["production", "has-sca"]);
}

#[test]
fn inventory_add_rejects_duplicate() {
    let live = empty_registry();
    let (output, _, stderr) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "inventory",
        "add",
        "--kind",
        "source_repo",
        "--name",
        "app-api", // already exists in the fixture
    ]);
    assert!(!output.status.success());
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
}

#[test]
fn inventory_retire_sets_retired_on() {
    let live = empty_registry();
    let (output, _, stderr) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "inventory",
        "retire",
        "--kind",
        "source_repo",
        "--name",
        "app-api",
        "--on",
        "2026-09-01",
        "--reason",
        "decommissioned",
    ]);
    assert!(output.status.success(), "stderr: {stderr}");

    let inv: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(live.path().join("inventory.yaml")).unwrap())
            .unwrap();
    let entry = &inv["source_repos"][0];
    assert_eq!(entry["retired_on"].as_str().unwrap(), "2026-09-01");
    assert_eq!(entry["retired_reason"].as_str().unwrap(), "decommissioned");
}

#[test]
fn inventory_retire_unknown_entry_fails() {
    let live = empty_registry();
    let (output, _, stderr) = run(&[
        "-C",
        live.path().to_str().unwrap(),
        "inventory",
        "retire",
        "--kind",
        "source_repo",
        "--name",
        "does-not-exist",
        "--on",
        "2026-09-01",
        "--reason",
        "test",
    ]);
    assert!(!output.status.success());
    assert!(stderr.contains("no entry named"), "stderr: {stderr}");
}

#[test]
fn inventory_check_passes_clean_fixture() {
    let live = empty_registry();
    let (output, stdout, _) = run(&["-C", live.path().to_str().unwrap(), "inventory", "check"]);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("inventory ok"), "stdout: {stdout}");
}

#[test]
fn inventory_check_flags_lifecycle_inversion() {
    let live = empty_registry();
    fs::write(
        live.path().join("inventory.yaml"),
        "source_repos:\n  - name: bad\n    in_scope_since: 2026-09-01\n    retired_on: 2026-01-01\n",
    )
    .unwrap();
    let (output, stdout, _) = run(&["-C", live.path().to_str().unwrap(), "inventory", "check"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout.contains("must be after"), "stdout: {stdout}");
}
