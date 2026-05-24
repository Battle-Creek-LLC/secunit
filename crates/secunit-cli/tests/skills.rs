//! `secunit skills` — bundled standard library, local overrides, and the
//! resolver `show`/`path` expose. The `multi-system` fixture ships a local
//! `policy-annual-review.md`, so it also exercises local-over-bundled.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;
use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/orgs")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = Command::cargo_bin("secunit")
        .unwrap()
        .args(args)
        .output()
        .expect("run secunit");
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn rooted(fix: &str, extras: &[&str]) -> Vec<String> {
    let mut v = vec![
        "-C".to_string(),
        fixture(fix).to_string_lossy().into_owned(),
    ];
    v.extend(extras.iter().map(|s| s.to_string()));
    v
}

#[test]
fn list_marks_bundled_and_local_override() {
    let args = rooted("multi-system", &["--json", "skills", "list"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, stderr) = run(&argv);
    assert!(ok, "skills list failed: {stderr}");

    let v: Value = serde_json::from_str(&stdout).unwrap();
    let by_name: HashMap<String, String> = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["source"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    // A bundled runbook the fixture has no local copy of.
    assert_eq!(
        by_name.get("capture-sweep").map(String::as_str),
        Some("bundled")
    );
    // The fixture ships its own policy-annual-review — local must win.
    assert_eq!(
        by_name.get("policy-annual-review").map(String::as_str),
        Some("local")
    );
    // A skill that exists only locally.
    assert_eq!(
        by_name.get("aa-weekly-audit-review").map(String::as_str),
        Some("local")
    );
}

#[test]
fn show_resolves_bundled_to_stdout() {
    let args = rooted("multi-system", &["skills", "show", "capture-sweep"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, stderr) = run(&argv);
    assert!(ok, "skills show failed: {stderr}");
    assert!(stdout.contains("name: capture-sweep"));
    assert!(stdout.contains("# Capture sweep"));
}

#[test]
fn show_prefers_local_override() {
    // The bundled policy-annual-review and the fixture's local copy differ;
    // `show` must return the local bytes.
    let local =
        std::fs::read_to_string(fixture("multi-system").join("skills/policy-annual-review.md"))
            .unwrap();
    let args = rooted("multi-system", &["skills", "show", "policy-annual-review"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, _) = run(&argv);
    assert!(ok);
    assert_eq!(stdout.trim_end(), local.trim_end());
}

#[test]
fn unknown_skill_errors() {
    let args = rooted("multi-system", &["skills", "show", "no-such-skill"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, _, stderr) = run(&argv);
    assert!(!ok, "expected failure for unknown skill");
    assert!(stderr.contains("unknown skill"));
}

#[test]
fn path_materialises_bundled_skill() {
    let args = rooted("multi-system", &["skills", "path", "report"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, stderr) = run(&argv);
    assert!(ok, "skills path failed: {stderr}");
    let p = PathBuf::from(stdout.trim());
    assert!(
        p.is_file(),
        "path should point at a real file: {}",
        p.display()
    );
    assert!(std::fs::read_to_string(&p)
        .unwrap()
        .contains("# Program report"));
}
