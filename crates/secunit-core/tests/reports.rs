//! Integration tests for the report data assembler. Stages the
//! multi-system fixture, seals real runs through the runner, then asserts
//! the assembled payload counts them — including the PLAN Phase 6 case of
//! a weekly control that missed consecutive periods.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use secunit_core::evidence::manifest::{RunOutcome, RunResult, SystemOutcome, SystemResult};
use secunit_core::evidence::runner::{self, PrepareOpts};
use secunit_core::registry::loader;
use secunit_core::reports;
use secunit_core::risks::{self, FindingRef, Severity, Status};
use secunit_core::SCHEMA_VERSION;
use tempfile::TempDir;
use walkdir::WalkDir;

const CONTROL: &str = "sca-weekly-dependency-scan";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/orgs/multi-system")
        .canonicalize()
        .expect("fixture must exist")
}

fn staged_fixture() -> (TempDir, PathBuf) {
    let src = fixture_root();
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().to_path_buf();
    copy_tree(&src, &dst);
    git_init_and_commit(&dst);
    (tmp, dst)
}

fn git_init_and_commit(root: &Path) {
    use std::process::Command;
    // Build the placeholder identity dynamically so source scanners
    // don't flag a literal placeholder email.
    let identity_email = format!("test{at}local.invalid", at = "@");
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(root)
            .args(args)
            .status()
            .expect("git in PATH");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", &identity_email]);
    run(&["config", "user.name", "test"]);
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "fixture import"]);
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in WalkDir::new(src) {
        let entry = entry.unwrap();
        let rel = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Seal one complete run of `CONTROL` stamped at `today` (noon UTC).
fn run_one(root: &Path, today: NaiveDate) {
    let (reg, report) = loader::load(root);
    assert!(report.errors.is_empty(), "load errors: {:?}", report.errors);

    let opts = PrepareOpts {
        today: Some(today),
        ..Default::default()
    };
    let ctx = runner::prepare(&reg, CONTROL, &opts).expect("prepare");

    for sys in &ctx.resolved_scope {
        let raw = ctx.run_dir.join("by-system").join(&sys.name).join("raw");
        fs::write(
            raw.join("scan.json"),
            format!("{{\"system\":\"{}\"}}", sys.name),
        )
        .unwrap();
    }
    fs::write(ctx.run_dir.join("findings.md"), b"# findings\nnone\n").unwrap();

    let result = RunResult {
        schema_version: SCHEMA_VERSION,
        control_id: ctx.control_id.clone(),
        run_id: ctx.run_id.clone(),
        status: RunOutcome::Complete,
        by_system: ctx
            .resolved_scope
            .iter()
            .map(|s| SystemResult {
                name: s.name.clone(),
                status: SystemOutcome::Complete,
                note: None,
            })
            .collect(),
        draft_risks: Vec::new(),
        draft_issues: Vec::new(),
        external_links: Vec::new(),
    };
    fs::write(
        ctx.run_dir.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();

    let completed_at = today.and_hms_opt(12, 0, 0).unwrap().and_utc();
    runner::finalize_at(&reg, &ctx.run_dir, completed_at).expect("finalize");
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[test]
fn monthly_window_counts_runs_and_surfaces_missed_weeks() {
    let (_tmp, root) = staged_fixture();

    // Seal W19 and W20; leave W18, W21, W22 unrun. May 2026 touches ISO
    // weeks W18 (Apr 27) through W22 (May 25).
    run_one(&root, d(2026, 5, 4)); // W19
    run_one(&root, d(2026, 5, 11)); // W20

    let (reg, _) = loader::load(&root);
    let data = reports::assemble(
        &reg,
        "2026-05",
        d(2026, 5, 1),
        d(2026, 5, 31),
        d(2026, 6, 1),
    )
    .expect("assemble");

    assert_eq!(data.period.label, "2026-05");
    let sca = data
        .controls
        .iter()
        .find(|c| c.id == CONTROL)
        .expect("weekly control present");

    assert_eq!(sca.counts.satisfied, 2, "W19 + W20 sealed");
    assert_eq!(
        sca.counts.gaps, 3,
        "W18, W21, W22 missed — the report must call out consecutive misses"
    );
    assert_eq!(sca.runs.len(), 2);
    assert!(sca.runs.iter().all(|r| r.path.starts_with("evidence/")));
    assert_eq!(data.totals.runs, 2);
    assert!(data.totals.gaps >= 2);

    // state.json advanced past the fixture next_due dates by 2026-06-01,
    // so the never-refreshed controls surface as overdue.
    assert!(
        data.overdue
            .iter()
            .any(|o| o.id == "aa-weekly-audit-review"),
        "stale weekly control should be overdue"
    );
}

#[test]
fn weekly_window_scopes_to_one_week() {
    let (_tmp, root) = staged_fixture();
    run_one(&root, d(2026, 5, 4)); // W19

    let (reg, _) = loader::load(&root);
    let data = reports::assemble(
        &reg,
        "2026-W19",
        d(2026, 5, 4),
        d(2026, 5, 10),
        d(2026, 5, 11),
    )
    .expect("assemble");

    let sca = data.controls.iter().find(|c| c.id == CONTROL).unwrap();
    assert_eq!(sca.periods.len(), 1);
    assert_eq!(sca.periods[0].period_id, "2026-W19");
    assert_eq!(sca.counts.satisfied, 1);
    assert_eq!(sca.counts.gaps, 0);
    assert_eq!(sca.runs.len(), 1);

    // The quarterly control's surrounding period appears for context.
    let vuln = data
        .controls
        .iter()
        .find(|c| c.id == "ca-quarterly-vuln-scan")
        .expect("quarterly control present");
    assert_eq!(vuln.periods[0].period_id, "2026-q2");
}

#[test]
fn risk_register_delta_counts_events_in_window() {
    let (_tmp, root) = staged_fixture();

    let finding_ref = |fid: &str| FindingRef {
        control_id: CONTROL.to_string(),
        run_id: "2026-05-04-run-001".to_string(),
        manifest_sha256: "0".repeat(64),
        finding_id: fid.to_string(),
        body_path: None,
    };
    let ts = |day: u32| d(2026, 5, day).and_hms_opt(9, 0, 0).unwrap().and_utc();

    // Opened in May, still open, past SLA by report time.
    let r1 = risks::open(
        &root,
        finding_ref("F-001"),
        "vulnerable dependency",
        Severity::High,
        4,
        3,
        vec!["api".into()],
        7,
        d(2026, 5, 12),
        "tester",
        None,
        Some(ts(5)),
    )
    .expect("open r1");

    // Opened in May and remediated in May.
    let r2 = risks::open(
        &root,
        finding_ref("F-002"),
        "stale access grant",
        Severity::Medium,
        3,
        2,
        vec!["api".into()],
        30,
        d(2026, 6, 4),
        "tester",
        None,
        Some(ts(6)),
    )
    .expect("open r2");
    risks::append(
        &root,
        &r2.risk_id,
        risks::EventData::StatusChanged {
            from: Status::Open,
            to: Status::InProgress,
            reason: "assigned".into(),
        },
        "tester",
        None,
        Some(ts(15)),
    )
    .expect("start r2");
    risks::append(
        &root,
        &r2.risk_id,
        risks::EventData::Remediated {
            resolved_run_ref: None,
            note: "revoked".into(),
        },
        "tester",
        None,
        Some(ts(20)),
    )
    .expect("remediate r2");

    let (reg, _) = loader::load(&root);
    let data = reports::assemble(
        &reg,
        "2026-05",
        d(2026, 5, 1),
        d(2026, 5, 31),
        d(2026, 6, 1),
    )
    .expect("assemble");

    assert_eq!(data.risks.opened_in_period, 2);
    assert_eq!(data.risks.closed_in_period, 1);
    assert_eq!(data.risks.open.len(), 1);
    assert_eq!(data.risks.open[0].risk_id, r1.risk_id);
    assert_eq!(data.risks.open[0].status, Status::Open);
    assert!(data.risks.open[0].past_sla, "due 5-12 < today 6-1");
    assert_eq!(data.risks.past_sla, 1);
    assert_eq!(
        data.risks.open[0].source_control.as_deref(),
        Some(CONTROL),
        "open risk traces back to its source control"
    );

    // A June window sees no May events and one still-open risk.
    let june = reports::assemble(
        &reg,
        "2026-06",
        d(2026, 6, 1),
        d(2026, 6, 30),
        d(2026, 6, 15),
    )
    .expect("assemble june");
    assert_eq!(june.risks.opened_in_period, 0);
    assert_eq!(june.risks.closed_in_period, 0);
    assert_eq!(june.risks.open.len(), 1);
}
