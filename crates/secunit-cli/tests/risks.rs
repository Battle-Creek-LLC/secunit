//! Integration tests for the `secunit risks` command family. Risk verbs
//! mutate the store (append-only logs + a derived index), so these drive the
//! CLI against a temp root rather than a static read-only fixture: open a
//! risk from a sealed manifest, then assert `list` / `show` reflect it.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use assert_cmd::cargo::CommandCargoExt;
use tempfile::TempDir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn run(root: &Path, args: &[&str]) -> (Output, String, String) {
    let mut full = vec!["-C", root.to_str().unwrap()];
    full.extend_from_slice(args);
    let output = Command::cargo_bin("secunit")
        .unwrap()
        .args(&full)
        .output()
        .expect("run secunit");
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    (output, stdout, stderr)
}

/// A live root with one control declaring remediation thresholds, plus a
/// sealed run dir holding a manifest with a single draft risk.
fn staged_root() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("controls")).unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();
    write(
        &root.join("inventory.yaml"),
        "source_repos:\n  - name: app-api\n    in_scope_since: 2026-01-01\n    tags: [production]\n",
    );
    write(
        &root.join("controls/ra-vuln-audit.yaml"),
        "id: ra-vuln-audit\n\
         title: Vulnerability audit\n\
         policy: security/ra.md\n\
         nist: [RA-5]\n\
         owner: cto\n\
         cadence: annual\n\
         skill: ra-vuln-audit\n\
         remediation_thresholds:\n  high: 30\n  critical: 14\n\
         evidence_required:\n  - kind: summary\n    description: Findings\n",
    );
    write(&root.join("skills/ra-vuln-audit.md"), "# stub runbook\n");

    // Sealed manifest with one draft risk, identified by `finding_id`.
    let manifest = serde_json::json!({
        "schema_version": 1,
        "control_id": "ra-vuln-audit",
        "run_id": "2026-05-25-run-001",
        "started_at": "2026-05-25T14:00:00Z",
        "completed_at": "2026-05-25T14:30:00Z",
        "agent": {
            "model": "m",
            "skill": "s",
            "skill_sha256": "a".repeat(64),
            "control_sha256": "b".repeat(64)
        },
        "registry_git_sha": "abcdef0",
        "scope_layout": "flat",
        "resolved_scope": [],
        "artifacts": [],
        "status": "complete",
        "draft_risks": [
            {
                "finding_id": "S032",
                "title": "S032 — pickle deserialization RCE (CWE-502)",
                "severity": "critical",
                "impact": 3,
                "likelihood": 3,
                "affected_systems": ["app-api"],
                "body_path": "findings.md#risk-1"
            }
        ]
    });
    let run_dir = root.join("evidence/2026/q2/ra-vuln-audit/2026-05-25-run-001");
    write(
        &run_dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );
    dir
}

fn run_dir_arg(root: &Path) -> String {
    root.join("evidence/2026/q2/ra-vuln-audit/2026-05-25-run-001")
        .to_str()
        .unwrap()
        .to_string()
}

#[test]
fn open_list_show_round_trip() {
    let dir = staged_root();
    let root = dir.path();
    let rd = run_dir_arg(root);

    // Open the risk from the sealed draft. Severity is critical → SLA 14d
    // from the run's completed_at (2026-05-25) → due 2026-06-08.
    let (out, stdout, stderr) = run(
        root,
        &[
            "risks",
            "open",
            "ra-vuln-audit",
            "--from",
            &rd,
            "--finding",
            "S032",
            "--owner",
            "cto",
        ],
    );
    assert!(out.status.success(), "open failed: {stderr}");
    assert!(stdout.contains("R-0001"), "stdout: {stdout}");
    assert!(stdout.contains("2026-06-08"), "due date wrong: {stdout}");

    // list (human) shows the risk with its severity, status and source.
    let (out, stdout, _) = run(root, &["--today", "2026-05-26", "risks", "list"]);
    assert!(out.status.success());
    assert!(stdout.contains("R-0001"));
    assert!(stdout.contains("critical"));
    assert!(stdout.contains("open"));
    assert!(stdout.contains("cto"));
    assert!(stdout.contains("ra-vuln-audit"));

    // list --json emits the structured index.
    let (out, stdout, _) = run(root, &["--json", "risks", "list"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["risks"]["R-0001"]["severity"], "critical");
    assert_eq!(v["risks"]["R-0001"]["owner"], "cto");
    assert_eq!(v["risks"]["R-0001"]["source_control"], "ra-vuln-audit");

    // show (human) renders the fold + timeline (opened + owner-assigned).
    let (out, stdout, _) = run(root, &["risks", "show", "R-0001"]);
    assert!(out.status.success());
    assert!(stdout.contains("R-0001"));
    assert!(stdout.contains("pickle deserialization"));
    assert!(stdout.contains("opened"));
    assert!(stdout.contains("owner-assigned"));
    assert!(stdout.contains("S032"));

    // show --json folds the log and includes the event list.
    let (out, stdout, _) = run(root, &["--json", "risks", "show", "R-0001"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["state"]["status"], "open");
    assert_eq!(v["state"]["severity"], "critical");
    assert_eq!(v["events"].as_array().unwrap().len(), 2);
}

#[test]
fn status_filter_and_past_sla() {
    let dir = staged_root();
    let root = dir.path();
    let rd = run_dir_arg(root);

    run(
        root,
        &[
            "risks",
            "open",
            "ra-vuln-audit",
            "--from",
            &rd,
            "--finding",
            "S032",
        ],
    );

    // Past SLA: today well after due 2026-06-08.
    let (_o, stdout, _) = run(
        root,
        &["--today", "2026-07-01", "risks", "list", "--past-sla"],
    );
    assert!(stdout.contains("R-0001"), "expected past-sla hit: {stdout}");

    // Before due: no past-sla rows.
    let (_o, stdout, _) = run(
        root,
        &["--today", "2026-05-26", "risks", "list", "--past-sla"],
    );
    assert!(stdout.contains("No risks match"), "stdout: {stdout}");

    // Severity filter that excludes the risk yields no rows.
    let (_o, stdout, _) = run(root, &["risks", "list", "--severity", "low"]);
    assert!(stdout.contains("No risks match"), "stdout: {stdout}");

    // Status filter that matches.
    let (_o, stdout, _) = run(root, &["risks", "list", "--status", "open"]);
    assert!(stdout.contains("R-0001"));
}

/// A live root whose sealed manifest carries two REAL-WORLD legacy drafts:
///
/// - Draft A: has `id`/`severity`/`subject`, but NO impact/likelihood, NO
///   `title`, NO `body_path`.
/// - Draft B: NO `id`/`finding_id`; a `ghsa[]` array; `affected` (not
///   `affected_systems`); capitalised `"High"` severity; `sla_days`; NO
///   impact/likelihood; NO `body_path`.
///
/// Both must promote into the register.
fn staged_root_legacy_drafts() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("controls")).unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();
    write(
        &root.join("inventory.yaml"),
        "source_repos:\n  - name: mdpdf\n    in_scope_since: 2026-01-01\n    tags: [production]\n  - name: brdg\n    in_scope_since: 2026-01-01\n    tags: [production]\n",
    );
    write(
        &root.join("controls/ra-vuln-audit.yaml"),
        "id: ra-vuln-audit\n\
         title: Vulnerability audit\n\
         policy: security/ra.md\n\
         nist: [RA-5]\n\
         owner: cto\n\
         cadence: annual\n\
         skill: ra-vuln-audit\n\
         remediation_thresholds:\n  high: 30\n  critical: 14\n\
         evidence_required:\n  - kind: summary\n    description: Findings\n",
    );
    write(&root.join("skills/ra-vuln-audit.md"), "# stub runbook\n");

    let manifest = serde_json::json!({
        "schema_version": 1,
        "control_id": "ra-vuln-audit",
        "run_id": "2026-05-25-run-001",
        "started_at": "2026-05-25T14:00:00Z",
        "completed_at": "2026-05-25T14:30:00Z",
        "agent": {
            "model": "m",
            "skill": "s",
            "skill_sha256": "a".repeat(64),
            "control_sha256": "b".repeat(64)
        },
        "registry_git_sha": "abcdef0",
        "scope_layout": "flat",
        "resolved_scope": [],
        "artifacts": [],
        "status": "complete",
        "draft_risks": [
            // Draft A — id + severity + subject, no impact/likelihood/title/body_path.
            {
                "affected_systems": ["mdpdf"],
                "evidence": "by-system/mdpdf/raw/dependabot-alerts.json",
                "id": "GHSA-xphw-cqx3-667j",
                "remediation": "Upgrade thin-vec to >=0.2.16",
                "severity": "high",
                "sla_due": "2026-07-22",
                "subject": "thin-vec use-after-free / double-free"
            },
            // Draft B — no id, ghsa[] array, `affected`, capitalised "High", sla_days.
            {
                "affected": ["brdg"],
                "evidence": "by-system/brdg/raw/dependabot-alerts.json",
                "ghsa": [
                    "GHSA-752w-5fwx-jx9f",
                    "GHSA-v92g-xgxw-vvmm",
                    "GHSA-2h4p-vjrc-8xpq",
                    "GHSA-qccp-gfcp-xxvc",
                    "GHSA-mf9v-mfxr-j63j"
                ],
                "severity": "High",
                "sla_days": 60,
                "subject": "brdg — 5 high-severity dependency vulnerabilities (SCA)"
            }
        ]
    });
    let run_dir = root.join("evidence/2026/q2/ra-vuln-audit/2026-05-25-run-001");
    write(
        &run_dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );
    dir
}

#[test]
fn open_promotes_legacy_draft_a() {
    // Draft A: id match, severity high, no impact/likelihood → derive (3,3),
    // title from subject, affected_systems present.
    let dir = staged_root_legacy_drafts();
    let root = dir.path();
    let rd = run_dir_arg(root);

    let (out, stdout, stderr) = run(
        root,
        &[
            "risks",
            "open",
            "ra-vuln-audit",
            "--from",
            &rd,
            "--finding",
            "GHSA-xphw-cqx3-667j",
        ],
    );
    assert!(out.status.success(), "open A failed: {stderr}");
    assert!(stdout.contains("R-0001"), "stdout: {stdout}");
    // High → SLA 30d from 2026-05-25 → due 2026-06-24.
    assert!(stdout.contains("2026-06-24"), "due date wrong: {stdout}");
    // The defaulted-score note is printed.
    assert!(
        stderr.contains("derived (3, 3) from severity"),
        "expected score note: {stderr}"
    );

    let (out, stdout, _) = run(root, &["--json", "risks", "show", "R-0001"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["state"]["status"], "open");
    assert_eq!(v["state"]["severity"], "high");
    // Derived score per the mapping: high = (3, 3).
    assert_eq!(v["state"]["impact"], 3);
    assert_eq!(v["state"]["likelihood"], 3);
    // Title falls back to `subject`.
    assert_eq!(v["state"]["title"], "thin-vec use-after-free / double-free");
    // affected_systems carried through.
    assert_eq!(v["state"]["affected_systems"], serde_json::json!(["mdpdf"]));
    // The recorded finding_id is the matched id (stable).
    let fr = &v["events"][0]["data"]["finding_ref"];
    assert_eq!(fr["finding_id"], "GHSA-xphw-cqx3-667j");
}

#[test]
fn open_promotes_legacy_draft_b() {
    // Draft B: matched via a ghsa[] value; capitalised "High" parsed; no
    // impact/likelihood → derive (3,3); title from subject; affected_systems
    // from `affected`. The recorded finding_id is the ghsa value matched.
    let dir = staged_root_legacy_drafts();
    let root = dir.path();
    let rd = run_dir_arg(root);

    let (out, stdout, stderr) = run(
        root,
        &[
            "risks",
            "open",
            "ra-vuln-audit",
            "--from",
            &rd,
            "--finding",
            "GHSA-752w-5fwx-jx9f",
        ],
    );
    assert!(out.status.success(), "open B failed: {stderr}");
    assert!(stdout.contains("R-0001"), "stdout: {stdout}");
    assert!(stdout.contains("2026-06-24"), "due date wrong: {stdout}");
    assert!(
        stderr.contains("derived (3, 3) from severity"),
        "expected score note: {stderr}"
    );

    let (out, stdout, _) = run(root, &["--json", "risks", "show", "R-0001"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["state"]["status"], "open");
    // "High" parsed case-insensitively.
    assert_eq!(v["state"]["severity"], "high");
    assert_eq!(v["state"]["impact"], 3);
    assert_eq!(v["state"]["likelihood"], 3);
    assert_eq!(
        v["state"]["title"],
        "brdg — 5 high-severity dependency vulnerabilities (SCA)"
    );
    // affected_systems derived from `affected`.
    assert_eq!(v["state"]["affected_systems"], serde_json::json!(["brdg"]));
    // The recorded finding_id is the exact ghsa value matched.
    let fr = &v["events"][0]["data"]["finding_ref"];
    assert_eq!(fr["finding_id"], "GHSA-752w-5fwx-jx9f");
    assert_eq!(fr["control_id"], "ra-vuln-audit");
    // Fingerprint is the matched ghsa value, reproducible across runs.
    assert_eq!(
        v["state"]["finding_refs"][0]["finding_id"],
        "GHSA-752w-5fwx-jx9f"
    );
}

#[test]
fn rebuild_regenerates_index() {
    let dir = staged_root();
    let root = dir.path();
    let rd = run_dir_arg(root);

    run(
        root,
        &[
            "risks",
            "open",
            "ra-vuln-audit",
            "--from",
            &rd,
            "--finding",
            "S032",
        ],
    );
    // Drop the derived index and rebuild it from the log.
    fs::remove_file(root.join("risks/index.json")).unwrap();
    let (out, stdout, stderr) = run(root, &["risks", "rebuild"]);
    assert!(out.status.success(), "rebuild failed: {stderr}");
    assert!(stdout.contains("1 risk"));
    assert!(root.join("risks/index.json").exists());
}
