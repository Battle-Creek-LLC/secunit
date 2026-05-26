//! Integration tests for `secunit doctor`. These avoid golden snapshots on
//! purpose — doctor's output embeds the build's version and compiled feature
//! set, which would make a snapshot host-dependent. Instead they assert on
//! stable structure: an empty, non-git directory must fail the preflight.

use assert_cmd::Command;
use predicates::prelude::*;

/// A directory that is neither a git repo nor a registry fails doctor:
/// `controls/` is missing and root is not a git repo, so it exits non-zero.
#[test]
fn doctor_on_empty_dir_fails() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .arg("doctor")
        .assert()
        .failure()
        .stdout(
            predicate::str::contains("not a git repository")
                .and(predicate::str::contains("controls/"))
                .and(predicate::str::contains("Risk register")),
        );
}

/// `--json` emits a well-formed report whose top-level `ok` is false on a bare
/// directory, with the five named sections present, and — the guarantee an
/// agent relies on — every `fail`/`warn` check carries a non-null `fix`.
#[test]
fn doctor_json_shape_and_fixes() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .arg("--json")
        .arg("doctor")
        .output()
        .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], serde_json::json!(false));

    let sections = value["sections"].as_array().unwrap();
    let titles: Vec<&str> = sections
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    for expected in [
        "Environment",
        "Repo structure",
        "Registry",
        "Evidence integrity",
        "Risk register",
    ] {
        assert!(titles.contains(&expected), "missing section: {expected}");
    }

    // Every actionable line must tell the agent what to do; ok/info must not
    // carry a spurious fix.
    let mut saw_fail = false;
    for check in sections
        .iter()
        .flat_map(|s| s["checks"].as_array().unwrap())
    {
        match check["status"].as_str().unwrap() {
            "fail" | "warn" => {
                saw_fail |= check["status"] == "fail";
                assert!(
                    check["fix"].as_str().is_some_and(|f| !f.is_empty()),
                    "actionable check without a fix: {check}"
                );
            }
            _ => assert!(
                check["fix"].is_null(),
                "ok/info should not carry a fix: {check}"
            ),
        }
    }
    assert!(
        saw_fail,
        "a bare directory should produce at least one failure"
    );
}
