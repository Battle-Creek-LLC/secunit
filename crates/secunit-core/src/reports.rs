//! Report data assembly: the structured JSON behind `secunit report data`.
//!
//! Aggregates per-control coverage, sealed-run activity, and the risk
//! register over one reporting window (week, month, quarter, or year) into
//! a [`ReportData`] payload. The `report` skill renders this to prose —
//! the binary never composes the report itself, and this module never
//! captures or mutates anything.

use std::fs;
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::evidence::manifest::{Manifest, RunOutcome};
use crate::model::{Cadence, LoadedRegistry, RunStatus};
use crate::registry::coverage::{self, PeriodCoverage, PeriodStatus};
use crate::registry::period;
use crate::registry::resolver;
use crate::risks::{self, EventData, RiskState, Severity, Status};

// ---------- payload ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportData {
    pub schema_version: u32,
    /// `org.name` from `_config.yaml`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    pub period: ReportPeriod,
    /// The pinned "today" the assembly ran under — separates `Open` from
    /// `Gap` periods and drives `past_sla` / `overdue`.
    pub generated_on: NaiveDate,
    pub controls: Vec<ControlActivity>,
    pub totals: Totals,
    /// Controls past due (and past their cadence grace window) as of
    /// `generated_on`, per the same resolver `secunit due` uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overdue: Vec<OverdueControl>,
    pub risks: RiskSummary,
    /// Controls due between `generated_on` and the end of the calendar
    /// period after the window, per the same resolver `secunit due` uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upcoming: Vec<UpcomingControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportPeriod {
    /// The selector as given: `2026-W30`, `2026-07`, `2026-q3`, or `2026`.
    pub label: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlActivity {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub cadence: Cadence,
    /// Coverage rows for every period of this control's cadence touching
    /// the window (a weekly report window sits inside one quarterly
    /// period — that period appears here with its current status).
    pub periods: Vec<PeriodCoverage>,
    pub counts: PeriodCounts,
    /// Sealed runs claiming a period in the window or completed inside it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PeriodCounts {
    pub satisfied: usize,
    /// Satisfied, but the run sealed after the period ended.
    pub late: usize,
    pub failed: usize,
    pub gaps: usize,
    pub open: usize,
    pub skipped: usize,
    pub future: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_id: Option<String>,
    pub completed_at: DateTime<Utc>,
    pub status: RunOutcome,
    pub draft_risks: usize,
    pub draft_issues: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<serde_json::Value>,
    /// Run dir relative to the secunit root.
    pub path: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Totals {
    pub controls: usize,
    pub runs: usize,
    pub satisfied: usize,
    pub late: usize,
    pub failed: usize,
    pub gaps: usize,
    pub open: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverdueControl {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub next_due: NaiveDate,
    pub days_overdue: i64,
    pub last_status: RunStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingControl {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub cadence: Cadence,
    pub next_due: NaiveDate,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskSummary {
    /// Risks currently open / in-progress, most severe first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<OpenRisk>,
    /// Risks created (an `opened` event) inside the window.
    pub opened_in_period: usize,
    /// Risks reopened inside the window. Counted separately from
    /// `opened_in_period` so a reopen never reads as "no new risk".
    pub reopened_in_period: usize,
    /// Risks that closed inside the window and are still closed at the
    /// window's end. Per-risk, not per-event: close→reopen→close churn
    /// counts once, and a close undone by a reopen counts zero.
    pub closed_in_period: usize,
    /// Open risks whose SLA due date has passed `generated_on`.
    pub past_sla: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRisk {
    pub risk_id: String,
    pub title: String,
    pub severity: Severity,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<NaiveDate>,
    pub past_sla: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_control: Option<String>,
}

// ---------- assembly --------------------------------------------------------

/// Assemble the report payload for `[start, end]`, labelled `label`.
///
/// `window_cadence` is the cadence of the reporting window itself (weekly
/// report → `Weekly`) and anchors the `upcoming` horizon to the next
/// calendar period. `today` separates open from gap periods and anchors
/// SLA/overdue math — pin it (`--today`) for deterministic output.
pub fn assemble(
    reg: &LoadedRegistry,
    label: &str,
    window_cadence: Cadence,
    start: NaiveDate,
    end: NaiveDate,
    today: NaiveDate,
) -> anyhow::Result<ReportData> {
    let mut controls: Vec<ControlActivity> = Vec::new();
    let mut totals = Totals::default();

    for control in reg.controls.values() {
        let cov = if matches!(control.cadence, Cadence::Continuous) {
            None
        } else {
            Some(coverage::coverage(reg, &control.id, start, end, today)?)
        };
        let periods = cov.map(|c| c.periods).unwrap_or_default();
        let runs = runs_in_window(
            &reg.root,
            &control.id,
            control.cadence,
            &periods,
            start,
            end,
        )?;

        // A control with nothing due and nothing run this window carries
        // no signal for the period — leave it out of the payload.
        if periods.is_empty() && runs.is_empty() {
            continue;
        }

        let mut counts = PeriodCounts::default();
        for p in &periods {
            match p.status {
                PeriodStatus::Satisfied => {
                    counts.satisfied += 1;
                    if p.late {
                        counts.late += 1;
                    }
                }
                PeriodStatus::Failed => counts.failed += 1,
                PeriodStatus::Gap => counts.gaps += 1,
                PeriodStatus::Open => counts.open += 1,
                PeriodStatus::Skipped => counts.skipped += 1,
                PeriodStatus::Future => counts.future += 1,
            }
        }
        totals.controls += 1;
        totals.runs += runs.len();
        totals.satisfied += counts.satisfied;
        totals.late += counts.late;
        totals.failed += counts.failed;
        totals.gaps += counts.gaps;
        totals.open += counts.open;

        controls.push(ControlActivity {
            id: control.id.clone(),
            title: control.title.clone(),
            owner: control.owner.clone(),
            cadence: control.cadence,
            periods,
            counts,
            runs,
        });
    }

    let overdue = overdue_controls(reg, today);
    let upcoming = upcoming_controls(reg, today, window_cadence, start, end);
    let risks = risk_summary(&reg.root, start, end, today)?;

    Ok(ReportData {
        schema_version: crate::SCHEMA_VERSION,
        org: reg.config.org.as_ref().and_then(|o| o.name.clone()),
        period: ReportPeriod {
            label: label.to_string(),
            start,
            end,
        },
        generated_on: today,
        controls,
        totals,
        overdue,
        risks,
        upcoming,
    })
}

/// Sealed runs for `control_id` that belong to the window: they claim a
/// period listed in `periods`, their claimed period overlaps the window,
/// or (continuous / legacy runs) they completed inside it. Mirrors the
/// lenient walk in `registry::coverage` — a corrupt manifest is skipped,
/// not fatal.
fn runs_in_window(
    root: &Path,
    control_id: &str,
    cadence: Cadence,
    periods: &[PeriodCoverage],
    start: NaiveDate,
    end: NaiveDate,
) -> anyhow::Result<Vec<RunSummary>> {
    let mut out: Vec<RunSummary> = Vec::new();
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
                let Ok(bytes) = fs::read(&mpath) else {
                    continue;
                };
                let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
                    continue;
                };
                // A run belongs to the window when it was sealed inside it
                // (catch-up runs claiming a prior period, and runs whose
                // period id predates a cadence change, must not vanish
                // from every report) or when its claimed period touches it.
                let completed_in_window = {
                    let d = manifest.completed_at.date_naive();
                    d >= start && d <= end
                };
                let in_window = completed_in_window
                    || match &manifest.period_id {
                        Some(pid) => {
                            periods.iter().any(|p| &p.period_id == pid)
                                || period::bounds(cadence, pid)
                                    .is_some_and(|(ps, pe)| ps <= end && pe >= start)
                        }
                        None => false,
                    };
                if !in_window {
                    continue;
                }
                let rel = run
                    .strip_prefix(root)
                    .unwrap_or(&run)
                    .to_string_lossy()
                    .into_owned();
                out.push(RunSummary {
                    run_id: manifest.run_id,
                    period_id: manifest.period_id,
                    completed_at: manifest.completed_at,
                    status: manifest.status,
                    draft_risks: manifest.draft_risks.len(),
                    draft_issues: manifest.draft_issues.len(),
                    external_links: manifest.external_links,
                    path: rel,
                });
            }
        }
    }
    out.sort_by_key(|r| r.completed_at);
    Ok(out)
}

/// Overdue per the resolver — the same due dates, schedule overrides,
/// never-run handling, and per-cadence grace `secunit due` applies, so the
/// report can never contradict it.
fn overdue_controls(reg: &LoadedRegistry, today: NaiveDate) -> Vec<OverdueControl> {
    let mut out: Vec<OverdueControl> = Vec::new();
    for row in resolver::due_rows(reg, today) {
        if !row.overdue {
            continue;
        }
        let Some(next_due) = row.next_due else {
            continue;
        };
        let Some(control) = reg.controls.get(&row.control_id) else {
            continue;
        };
        let last_status = reg
            .state
            .controls
            .get(&row.control_id)
            .map(|e| e.last_status)
            .unwrap_or(RunStatus::NeverRun);
        out.push(OverdueControl {
            id: control.id.clone(),
            title: control.title.clone(),
            owner: control.owner.clone(),
            next_due,
            days_overdue: (today - next_due).num_days(),
            last_status,
        });
    }
    out.sort_by_key(|o| std::cmp::Reverse(o.days_overdue));
    out
}

/// Controls due between `today` and the end of the calendar period after
/// the window (the next week/month/quarter/year), resolver-computed like
/// `overdue_controls`. Controls already overdue are listed there instead.
fn upcoming_controls(
    reg: &LoadedRegistry,
    today: NaiveDate,
    window_cadence: Cadence,
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<UpcomingControl> {
    let next_period_start = end + chrono::Duration::days(1);
    let horizon = period::derive(window_cadence, next_period_start)
        .and_then(|pid| period::bounds(window_cadence, &pid))
        .map(|(_, pe)| pe)
        // Continuous has no periods; fall back to one window-length.
        .unwrap_or(end + chrono::Duration::days((end - start).num_days() + 1));
    let mut out: Vec<UpcomingControl> = Vec::new();
    for row in resolver::due_rows(reg, today) {
        if row.overdue {
            continue;
        }
        let Some(next_due) = row.next_due else {
            continue;
        };
        if next_due < today || next_due > horizon {
            continue;
        }
        let Some(control) = reg.controls.get(&row.control_id) else {
            continue;
        };
        out.push(UpcomingControl {
            id: control.id.clone(),
            title: control.title.clone(),
            owner: control.owner.clone(),
            cadence: control.cadence,
            next_due,
        });
    }
    out.sort_by(|a, b| a.next_due.cmp(&b.next_due).then(a.id.cmp(&b.id)));
    out
}

/// Fold every risk log under `risks/` into the report's register view:
/// currently-open risks plus opened/closed counts inside the window.
/// Corrupt logs are fatal here — unlike a missing manifest, a broken risk
/// chain means the register can't be trusted, and the report must not
/// silently understate it.
fn risk_summary(
    root: &Path,
    start: NaiveDate,
    end: NaiveDate,
    today: NaiveDate,
) -> anyhow::Result<RiskSummary> {
    let mut summary = RiskSummary::default();
    for risk_id in risks::risk_ids(root)? {
        let events = risks::load_events(root, &risk_id)?;

        // Count per risk, not per event: a close that a later in-window
        // reopen undoes must not read as a closure, and churn must not
        // double-count. `closed` therefore requires both an in-window
        // closing event and a closed status when the window ends.
        let mut opened = false;
        let mut reopened = false;
        let mut closing_event = false;
        for ev in &events {
            let d = ev.ts.date_naive();
            if d < start || d > end {
                continue;
            }
            match &ev.data {
                EventData::Opened { .. } => opened = true,
                EventData::Reopened { .. } => reopened = true,
                EventData::Remediated { .. } | EventData::ExceptionDocumented { .. } => {
                    closing_event = true
                }
                EventData::StatusChanged { to, .. } => match to {
                    Status::Remediated | Status::FalsePositive | Status::AcceptedException => {
                        closing_event = true
                    }
                    Status::Reopened => reopened = true,
                    _ => {}
                },
                _ => {}
            }
        }
        if opened {
            summary.opened_in_period += 1;
        }
        if reopened {
            summary.reopened_in_period += 1;
        }
        if closing_event {
            let at_end: Vec<risks::RiskEvent> = events
                .iter()
                .filter(|e| e.ts.date_naive() <= end)
                .cloned()
                .collect();
            let closed_at_end = matches!(
                risks::fold(&at_end).status,
                Status::Remediated | Status::FalsePositive | Status::AcceptedException
            );
            if closed_at_end {
                summary.closed_in_period += 1;
            }
        }

        let state: RiskState = risks::fold(&events);
        if matches!(
            state.status,
            Status::Open | Status::InProgress | Status::Reopened
        ) {
            let past_sla = state.due_at.is_some_and(|d| d < today);
            if past_sla {
                summary.past_sla += 1;
            }
            summary.open.push(OpenRisk {
                risk_id,
                title: state.title.clone(),
                severity: state.severity,
                status: state.status,
                owner: state.owner.clone(),
                due_at: state.due_at,
                past_sla,
                source_control: state.source_control().map(str::to_string),
            });
        }
    }
    summary
        .open
        .sort_by(|a, b| a.severity.cmp(&b.severity).then(a.risk_id.cmp(&b.risk_id)));
    Ok(summary)
}

fn dir_children(p: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut v = Vec::new();
    for entry in fs::read_dir(p)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            v.push(entry.path());
        }
    }
    v.sort();
    Ok(v)
}
