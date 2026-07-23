//! Golden-file tests for the read-only CLI subcommands. Snapshots live
//! under `crates/secunit-cli/snapshots/` (managed by `cargo insta review`).

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/orgs")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn run_cli(args: &[&str]) -> String {
    let output = Command::cargo_bin("secunit")
        .unwrap()
        .args(args)
        .output()
        .expect("run secunit");
    let stdout = String::from_utf8(output.stdout).unwrap();
    if !output.status.success() {
        panic!(
            "secunit {:?} exited {:?}\nstderr:\n{}",
            args,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    stdout
}

fn args_for(fix: &str, today: &str, extras: &[&str]) -> Vec<String> {
    let mut v = vec![
        "-C".into(),
        fixture(fix).to_string_lossy().into_owned(),
        "--today".into(),
        today.into(),
    ];
    v.extend(extras.iter().map(|s| (*s).to_string()));
    v
}

#[test]
fn due_within_14_days_human() {
    let args = args_for("multi-system", "2026-05-01", &["due", "--within", "14"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("due_within_14_days_human", run_cli(&argv));
}

#[test]
fn due_within_14_days_json() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["--json", "due", "--within", "14"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("due_within_14_days_json", run_cli(&argv));
}

#[test]
fn scope_sca_human() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["scope", "sca-weekly-dependency-scan"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("scope_sca_human", run_cli(&argv));
}

#[test]
fn scope_sca_post_retirement() {
    let args = args_for(
        "multi-system",
        "2026-10-01",
        &["scope", "sca-weekly-dependency-scan"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("scope_sca_post_retirement", run_cli(&argv));
}

#[test]
fn validate_clean_fixture() {
    let args = args_for("multi-system", "2026-05-01", &["validate"]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("validate_clean_fixture", run_cli(&argv));
}

#[test]
fn status_evidence_found_human() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["status", "aa-weekly-audit-review", "--evidence"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("status_evidence_found_human", run_cli(&argv));
}

#[test]
fn status_evidence_missing_file_human() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["status", "ca-quarterly-vuln-scan", "-e"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("status_evidence_missing_file_human", run_cli(&argv));
}

#[test]
fn status_evidence_never_run_human() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["status", "cp-annual-bcp-test", "-e"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("status_evidence_never_run_human", run_cli(&argv));
}

#[test]
fn status_evidence_found_json() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["--json", "status", "aa-weekly-audit-review", "--evidence"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("status_evidence_found_json", run_cli(&argv));
}

#[test]
fn show_sca_human() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["show", "sca-weekly-dependency-scan"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("show_sca_human", run_cli(&argv));
}

#[test]
fn report_data_week_json() {
    let args = args_for(
        "multi-system",
        "2026-05-01",
        &["report", "data", "--week", "2026-W18"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("report_data_week_json", run_cli(&argv));
}

#[test]
fn report_data_month_json() {
    let args = args_for(
        "multi-system",
        "2026-06-01",
        &["report", "data", "--month", "2026-05"],
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    insta::assert_snapshot!("report_data_month_json", run_cli(&argv));
}
