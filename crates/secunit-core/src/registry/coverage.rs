//! Coverage reports tying expected periods to the runs that satisfied them.
//!
//! Two halves: [`expected_periods`] enumerates the periods a control owes
//! over a window (respecting `schedule.yaml` skip directives), and
//! [`coverage`] walks `evidence/<control>/` to find the complete runs that
//! claimed each period. The resulting [`CoverageReport`] is the
//! auditor-shaped answer: every period is `Satisfied`, `Gap`, `Skipped`, or
//! `Future`, and any unclaimed/legacy evidence is surfaced separately.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::period;
use crate::evidence::manifest::{Manifest, RunOutcome};
use crate::model::{Cadence, Control, LoadedRegistry, Schedule};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRef {
    pub run_id: String,
    pub completed_at: DateTime<Utc>,
    pub status: RunOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PeriodStatus {
    /// At least one `complete` run claims this period.
    Satisfied,
    /// Period has ended without a satisfying run.
    Gap,
    /// `schedule.yaml` skip directive removed this period.
    Skipped,
    /// Period hasn't started yet relative to `today`.
    Future,
    /// Period is open: it's in progress and not yet satisfied.
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodCoverage {
    pub period_id: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub status: PeriodStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub satisfied_by: Option<RunRef>,
    /// Set when `satisfied_by.completed_at` falls past `period_end`.
    #[serde(default)]
    pub late: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnclassifiedRun {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_id: Option<String>,
    pub completed_at: DateTime<Utc>,
    pub status: RunOutcome,
    /// Why this run isn't bucketed into the report's expected periods —
    /// e.g. legacy run with no `period_id`, or claims a period outside
    /// the requested window.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub control_id: String,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub periods: Vec<PeriodCoverage>,
    #[serde(default)]
    pub unclassified_runs: Vec<UnclassifiedRun>,
}

/// Periods a control is expected to cover within `[window_start, window_end]`.
///
/// Returns `(period_id, period_start, period_end)` in chronological order,
/// excluding any periods removed by `schedule.yaml` skip directives.
/// Continuous cadence has no periods.
pub fn expected_periods(
    control: &Control,
    schedule: &Schedule,
    window_start: NaiveDate,
    window_end: NaiveDate,
) -> Vec<(String, NaiveDate, NaiveDate)> {
    if matches!(control.cadence, Cadence::Continuous) || window_start > window_end {
        return Vec::new();
    }
    let mut out: Vec<(String, NaiveDate, NaiveDate)> = Vec::new();
    let mut cursor = window_start;
    while let Some(pid) = period::derive(control.cadence, cursor) {
        let Some((start, end)) = period::bounds(control.cadence, &pid) else {
            break;
        };
        // Defensive guard against pathological bounds that don't move
        // forward (would otherwise infinite-loop).
        if out.last().is_none_or(|(p, _, _)| p != &pid) {
            out.push((pid, start, end));
        }
        let Some(next) = end.succ_opt() else { break };
        if next > window_end {
            break;
        }
        cursor = next;
    }
    out.into_iter()
        .filter(|(_, start, _)| !is_skipped(control, schedule, *start).0)
        .collect()
}

/// Build a coverage report for `control_id` over `[window_start, window_end]`.
///
/// `today` separates `Future` from `Open`/`Gap` — periods starting after
/// today are `Future`; current/past periods are `Open` (current,
/// unsatisfied) or `Gap` (past, unsatisfied) when no run claims them.
pub fn coverage(
    reg: &LoadedRegistry,
    control_id: &str,
    window_start: NaiveDate,
    window_end: NaiveDate,
    today: NaiveDate,
) -> anyhow::Result<CoverageReport> {
    let control = reg
        .controls
        .get(control_id)
        .ok_or_else(|| anyhow::anyhow!("control `{control_id}` not found"))?;

    let expected = expected_periods(control, &reg.schedule, window_start, window_end);
    let runs = walk_runs_for_control(&reg.root, control_id)?;

    // Bucket runs by claimed period_id. Multiple runs claiming the same
    // period coexist; we pick the earliest `complete` run as the
    // satisfier, and surface the rest as additional context if needed
    // (PR-2 keeps it minimal — first complete wins).
    let mut by_period: HashMap<String, Vec<RunRef>> = HashMap::new();
    let mut legacy: Vec<UnclassifiedRun> = Vec::new();
    for r in &runs {
        match &r.period_id {
            Some(pid) => by_period.entry(pid.clone()).or_default().push(RunRef {
                run_id: r.run_id.clone(),
                completed_at: r.completed_at,
                status: r.status,
            }),
            None => legacy.push(UnclassifiedRun {
                run_id: r.run_id.clone(),
                period_id: None,
                completed_at: r.completed_at,
                status: r.status,
                reason: "legacy run sealed before period_id was introduced".into(),
            }),
        }
    }

    let expected_ids: std::collections::HashSet<&str> =
        expected.iter().map(|(p, _, _)| p.as_str()).collect();
    let mut periods: Vec<PeriodCoverage> = Vec::with_capacity(expected.len());
    for (pid, start, end) in &expected {
        let runs_here = by_period.get(pid);
        let satisfier = runs_here.and_then(|rs| {
            rs.iter()
                .filter(|r| matches!(r.status, RunOutcome::Complete))
                .min_by_key(|r| r.completed_at)
                .cloned()
        });

        let (status, late) = match &satisfier {
            Some(r) => (PeriodStatus::Satisfied, r.completed_at.date_naive() > *end),
            None => {
                if today < *start {
                    (PeriodStatus::Future, false)
                } else if today >= *start && today <= *end {
                    (PeriodStatus::Open, false)
                } else {
                    (PeriodStatus::Gap, false)
                }
            }
        };

        periods.push(PeriodCoverage {
            period_id: pid.clone(),
            period_start: *start,
            period_end: *end,
            status,
            satisfied_by: satisfier,
            late,
            skipped_reason: None,
        });
    }

    // Add explicitly-skipped periods so the report shows them as such.
    let mut cursor = window_start;
    while cursor <= window_end {
        if let Some(pid) = period::derive(control.cadence, cursor) {
            if let Some((start, end)) = period::bounds(control.cadence, &pid) {
                let (skipped, reason) = is_skipped(control, &reg.schedule, start);
                if skipped && !periods.iter().any(|p| p.period_id == pid) {
                    periods.push(PeriodCoverage {
                        period_id: pid.clone(),
                        period_start: start,
                        period_end: end,
                        status: PeriodStatus::Skipped,
                        satisfied_by: None,
                        late: false,
                        skipped_reason: reason,
                    });
                }
                cursor = end.succ_opt().unwrap_or(end);
                if cursor <= end {
                    break;
                }
                continue;
            }
        }
        break;
    }
    periods.sort_by_key(|p| p.period_start);

    // Runs whose claimed period falls outside the window: report as
    // out-of-window unclassified so auditors can see them.
    for (pid, rs) in by_period {
        if !expected_ids.contains(pid.as_str()) {
            for r in rs {
                legacy.push(UnclassifiedRun {
                    run_id: r.run_id,
                    period_id: Some(pid.clone()),
                    completed_at: r.completed_at,
                    status: r.status,
                    reason: format!("claims period `{pid}` outside requested window"),
                });
            }
        }
    }
    legacy.sort_by_key(|u| u.completed_at);

    Ok(CoverageReport {
        control_id: control_id.to_string(),
        window_start,
        window_end,
        periods,
        unclassified_runs: legacy,
    })
}

/// Test if a period whose start falls on `period_start` is removed by
/// any `schedule.yaml` skip directive for this control. Returns the
/// skip reason when matched.
fn is_skipped(
    control: &Control,
    schedule: &Schedule,
    period_start: NaiveDate,
) -> (bool, Option<String>) {
    for entry in schedule
        .overrides
        .iter()
        .filter(|o| o.control_id == control.id)
    {
        let Some(skip) = &entry.skip else {
            continue;
        };
        if let Some(q) = &skip.quarter {
            let pq = format!(
                "{:04}-q{}",
                period_start.year(),
                quarter_of_month(period_start.month())
            );
            if &pq == q {
                return (true, skip.reason.clone().or_else(|| entry.reason.clone()));
            }
        }
        if let Some(y) = skip.year {
            if period_start.year() == y {
                return (true, skip.reason.clone().or_else(|| entry.reason.clone()));
            }
        }
    }
    (false, None)
}

fn quarter_of_month(month: u32) -> u32 {
    (month - 1) / 3 + 1
}

#[derive(Debug, Clone)]
struct WalkedRun {
    run_id: String,
    completed_at: DateTime<Utc>,
    status: RunOutcome,
    period_id: Option<String>,
}

/// Walk `<root>/evidence/*/*/control_id/*/manifest.json` and return one
/// row per sealed manifest. Skips pending runs (no manifest yet) and
/// silently skips manifests that fail to parse — coverage queries
/// shouldn't take down the whole report on one corrupt manifest. Callers
/// who want stricter checking should use [`crate::evidence::verifier`].
fn walk_runs_for_control(root: &Path, control_id: &str) -> anyhow::Result<Vec<WalkedRun>> {
    let mut out: Vec<WalkedRun> = Vec::new();
    let evidence = root.join("evidence");
    if !evidence.is_dir() {
        return Ok(out);
    }
    for year in dir_children(&evidence)? {
        for quarter in dir_children(&year)? {
            let ctrl_dir = quarter.join(control_id);
            if !ctrl_dir.is_dir() {
                continue;
            }
            for run in dir_children(&ctrl_dir)? {
                let mpath = run.join("manifest.json");
                if !mpath.is_file() {
                    continue;
                }
                let bytes = match fs::read(&mpath) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let manifest: Manifest = match serde_json::from_slice(&bytes) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                out.push(WalkedRun {
                    run_id: manifest.run_id,
                    completed_at: manifest.completed_at,
                    status: manifest.status,
                    period_id: manifest.period_id,
                });
            }
        }
    }
    Ok(out)
}

fn dir_children(p: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut v = Vec::new();
    for entry in fs::read_dir(p)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            v.push(entry.path());
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cadence, Control, Schedule, ScheduleEntry, ScheduleSkip};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn make_control(id: &str, cadence: Cadence) -> Control {
        Control {
            id: id.into(),
            title: "t".into(),
            policy: "p".into(),
            nist: Vec::new(),
            owner: "o".into(),
            cadence,
            weekday: None,
            due: None,
            due_by: None,
            skill: "s".into(),
            skill_args: None,
            scope: None,
            evidence_required: Vec::new(),
            remediation_thresholds: Default::default(),
            outputs: None,
            references: Vec::new(),
        }
    }

    fn weekly_control(id: &str) -> Control {
        make_control(id, Cadence::Weekly)
    }

    fn quarterly_control(id: &str) -> Control {
        make_control(id, Cadence::Quarterly)
    }

    fn continuous_control(id: &str) -> Control {
        make_control(id, Cadence::Continuous)
    }

    #[test]
    fn weekly_window_covers_iso_weeks_in_range() {
        let c = weekly_control("c1");
        let s = Schedule::default();
        // Apr 27 (Mon, W18) through May 17 (Sun, W20): expect W18, W19, W20.
        let got = expected_periods(&c, &s, d(2026, 4, 27), d(2026, 5, 17));
        let ids: Vec<&str> = got.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(ids, ["2026-W18", "2026-W19", "2026-W20"]);
    }

    #[test]
    fn quarterly_window_covers_quarters() {
        let c = quarterly_control("c1");
        let s = Schedule::default();
        let got = expected_periods(&c, &s, d(2026, 1, 1), d(2026, 12, 31));
        let ids: Vec<&str> = got.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(ids, ["2026-q1", "2026-q2", "2026-q3", "2026-q4"]);
    }

    #[test]
    fn continuous_returns_no_periods() {
        let c = continuous_control("c1");
        let s = Schedule::default();
        assert!(expected_periods(&c, &s, d(2026, 1, 1), d(2026, 12, 31)).is_empty());
    }

    #[test]
    fn skip_quarter_directive_excludes_periods_in_that_quarter() {
        let c = weekly_control("c1");
        let s = Schedule {
            overrides: vec![ScheduleEntry {
                control_id: "c1".into(),
                due: None,
                weekday: None,
                note: None,
                reason: None,
                skip: Some(ScheduleSkip {
                    quarter: Some("2026-q2".into()),
                    year: None,
                    reason: Some("audit prep".into()),
                }),
                insert: None,
            }],
        };
        // Window covers Q1 end and Q2 start. Q2 weeks should be excluded.
        let got = expected_periods(&c, &s, d(2026, 3, 23), d(2026, 4, 12));
        let ids: Vec<&str> = got.iter().map(|(p, _, _)| p.as_str()).collect();
        // ISO weeks: Mar 23-29 = W13, Mar 30-Apr 5 = W14 (starts in Q1, but
        // its start date Mar 30 is in Q1 → kept). Apr 6-12 = W15 (Q2 → skipped).
        assert!(ids.contains(&"2026-W13"));
        assert!(!ids.contains(&"2026-W15"));
    }

    #[test]
    fn empty_window_returns_empty() {
        let c = weekly_control("c1");
        let s = Schedule::default();
        assert!(expected_periods(&c, &s, d(2026, 5, 10), d(2026, 5, 4)).is_empty());
    }
}
