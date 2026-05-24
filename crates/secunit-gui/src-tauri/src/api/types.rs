//! Hand-rolled IPC payload types. We keep a small, explicit boundary
//! between `secunit_core` and the webview: the GUI is read-only and
//! doesn't need every internal field across the wire. Drift is caught
//! by the integration test in `tests/api_smoke.rs`.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LoadSummary {
    pub name: String,
    pub root: String,
    pub controls_count: usize,
    pub inventory_count: usize,
    pub has_state: bool,
    pub has_config: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlStatus {
    /// Control has run cleanly within its grace window for `next_due`.
    Sealed,
    /// Last run sealed but reported `status=failed`.
    Failed,
    /// A run is currently prepared but not yet sealed.
    InProgress,
    /// `next_due` is in the future and ≤ 7 days away.
    DueSoon,
    /// Past `next_due + grace` with no fresher seal.
    Overdue,
    /// No history yet.
    NeverRun,
    /// Cadence does not produce a calendar firing (continuous), nothing
    /// pending, nothing recent — used as a calm default.
    Idle,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlSummary {
    pub id: String,
    pub title: String,
    pub cadence: String,
    pub owner: String,
    pub status: ControlStatus,
    pub next_due: Option<NaiveDate>,
    pub overdue: bool,
    pub last_run_id: Option<String>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedSystemView {
    pub name: String,
    pub kind: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlDetail {
    pub summary: ControlSummary,
    pub policy: String,
    pub nist: Vec<String>,
    pub skill: String,
    pub references: Vec<ReferenceView>,
    pub recent_runs: Vec<RunRow>,
    pub resolved_scope_today: Vec<ResolvedSystemView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceView {
    pub title: String,
    pub path: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    Sealed,
    Pending,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRow {
    pub control_id: String,
    pub run_id: String,
    pub run_dir: String,
    pub state: RunState,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub manifest_sha256: Option<String>,
    pub year: i32,
    pub quarter: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DueRowView {
    pub control_id: String,
    pub cadence: String,
    pub next_due: Option<NaiveDate>,
    pub overdue: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PeriodStatusView {
    Satisfied,
    Failed,
    Gap,
    Skipped,
    Future,
    Open,
}

impl From<secunit_core::registry::coverage::PeriodStatus> for PeriodStatusView {
    fn from(s: secunit_core::registry::coverage::PeriodStatus) -> Self {
        use secunit_core::registry::coverage::PeriodStatus as Core;
        match s {
            Core::Satisfied => PeriodStatusView::Satisfied,
            Core::Failed => PeriodStatusView::Failed,
            Core::Gap => PeriodStatusView::Gap,
            Core::Skipped => PeriodStatusView::Skipped,
            Core::Future => PeriodStatusView::Future,
            Core::Open => PeriodStatusView::Open,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentPeriodStatus {
    pub control_id: String,
    pub cadence: String,
    /// `None` for continuous controls; otherwise the period containing
    /// `today`.
    pub period_id: Option<String>,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub status: PeriodStatusView,
    pub satisfied_by_run_id: Option<String>,
    pub late: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodCoverageView {
    pub period_id: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub status: PeriodStatusView,
    pub satisfied_by_run_id: Option<String>,
    pub late: bool,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReportView {
    pub control_id: String,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub periods: Vec<PeriodCoverageView>,
    pub unclassified_runs: Vec<UnclassifiedRunView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnclassifiedRunView {
    pub run_id: String,
    pub period_id: Option<String>,
    pub completed_at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleReason {
    Cadence,
    OverrideDue,
    OverrideInsert,
    OverrideWeekday,
    OverrideSkip,
}

impl From<secunit_core::registry::resolver::DueReason> for ScheduleReason {
    fn from(r: secunit_core::registry::resolver::DueReason) -> Self {
        use secunit_core::registry::resolver::DueReason as Core;
        match r {
            Core::Cadence => ScheduleReason::Cadence,
            Core::OverrideDue => ScheduleReason::OverrideDue,
            Core::OverrideInsert => ScheduleReason::OverrideInsert,
            Core::OverrideWeekday => ScheduleReason::OverrideWeekday,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleEntryView {
    pub control_id: String,
    pub cadence: String,
    pub date: NaiveDate,
    pub reason: ScheduleReason,
    pub note: Option<String>,
    pub overdue: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryView {
    pub kinds: Vec<InventoryKindView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryKindView {
    pub kind: String,
    pub entries: Vec<InventoryEntryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryEntryView {
    pub name: String,
    pub tags: Vec<String>,
    pub in_scope_since: Option<NaiveDate>,
    pub retired_on: Option<NaiveDate>,
    pub aliases: Vec<String>,
    pub active_today: bool,
    pub extras: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    pub row: RunRow,
    pub manifest: Option<serde_json::Value>,
    pub prepare: Option<serde_json::Value>,
    pub tree: Vec<RunTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTreeNode {
    pub name: String,
    pub path: String,
    pub kind: RunTreeKind,
    pub size: Option<u64>,
    pub children: Vec<RunTreeNode>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunTreeKind {
    Dir,
    File,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingsRow {
    pub control_id: String,
    pub run_id: String,
    pub path: String,
    pub year: i32,
    pub quarter: u32,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub run_state: RunState,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingsHtml {
    pub control_id: String,
    pub run_id: String,
    pub path: String,
    pub html: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Markdown,
    Json,
    Yaml,
    Text,
    Binary,
    TooLarge,
    Image,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactView {
    pub path: String,
    pub bytes: u64,
    pub kind: ArtifactKind,
    /// `Some` for text-shaped kinds; `None` for `Binary`, `TooLarge`, `Image`.
    pub text: Option<String>,
    /// `Some(html)` if `kind == Markdown`, sanitised through the same
    /// path as `read_findings`.
    pub html: Option<String>,
}
