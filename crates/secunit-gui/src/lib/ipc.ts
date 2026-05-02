import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Project config (JOB-02)
// ---------------------------------------------------------------------------

export interface ProjectEntryView {
  name: string;
  path: string;
  resolved_path: string;
  exists: boolean;
}

export interface ProjectsView {
  projects: ProjectEntryView[];
  default: string | null;
  last_selected: string | null;
  config_path: string;
}

export const listProjects = () => invoke<ProjectsView>("list_projects");
export const selectProject = (name: string) =>
  invoke<string>("select_project", { name });
export const currentProject = () => invoke<string | null>("current_project");

// ---------------------------------------------------------------------------
// Registry surface (JOB-03)
// ---------------------------------------------------------------------------

export interface LoadSummary {
  name: string;
  root: string;
  controls_count: number;
  inventory_count: number;
  has_state: boolean;
  has_config: boolean;
  errors: string[];
  warnings: string[];
}

export type ControlStatus =
  | "sealed"
  | "failed"
  | "in-progress"
  | "due-soon"
  | "overdue"
  | "never-run"
  | "idle";

export interface ControlSummary {
  id: string;
  title: string;
  cadence: string;
  owner: string;
  status: ControlStatus;
  next_due: string | null;
  overdue: boolean;
  last_run_id: string | null;
  last_run_at: string | null;
  last_status: string | null;
}

export interface ResolvedSystemView {
  name: string;
  kind: string;
  tags: string[];
}

export interface ReferenceView {
  title: string;
  path: string | null;
  url: string | null;
}

export type RunState = "sealed" | "pending";

export interface RunRow {
  control_id: string;
  run_id: string;
  run_dir: string;
  state: RunState;
  started_at: string | null;
  completed_at: string | null;
  manifest_sha256: string | null;
  year: number;
  quarter: number;
}

export interface ControlDetail {
  summary: ControlSummary;
  policy: string;
  nist: string[];
  skill: string;
  references: ReferenceView[];
  recent_runs: RunRow[];
  resolved_scope_today: ResolvedSystemView[];
}

export interface DueRowView {
  control_id: string;
  cadence: string;
  next_due: string | null;
  overdue: boolean;
}

export interface InventoryEntryView {
  name: string;
  tags: string[];
  in_scope_since: string | null;
  retired_on: string | null;
  aliases: string[];
  active_today: boolean;
  extras: Record<string, unknown>;
}

export interface InventoryKindView {
  kind: string;
  entries: InventoryEntryView[];
}

export interface InventoryView {
  kinds: InventoryKindView[];
}

export type RunTreeKind = "dir" | "file";

export interface RunTreeNode {
  name: string;
  path: string;
  kind: RunTreeKind;
  size: number | null;
  children: RunTreeNode[];
}

export interface RunDetail {
  row: RunRow;
  manifest: unknown | null;
  prepare: unknown | null;
  abort: unknown | null;
  tree: RunTreeNode[];
}

export const loadProject = (name: string) =>
  invoke<LoadSummary>("load_project", { name });

export const listControls = (today?: string | null) =>
  invoke<ControlSummary[]>("list_controls", { today: today ?? null });

export const getControl = (id: string, today?: string | null) =>
  invoke<ControlDetail>("get_control", { id, today: today ?? null });

export const dueRows = (today?: string | null) =>
  invoke<DueRowView[]>("due_rows", { today: today ?? null });

export type PeriodStatus = "satisfied" | "gap" | "skipped" | "future" | "open";

export interface CurrentPeriodStatus {
  control_id: string;
  cadence: string;
  period_id: string | null;
  period_start: string | null;
  period_end: string | null;
  status: PeriodStatus;
  satisfied_by_run_id: string | null;
  late: boolean;
}

export interface PeriodCoverageView {
  period_id: string;
  period_start: string;
  period_end: string;
  status: PeriodStatus;
  satisfied_by_run_id: string | null;
  late: boolean;
  skipped_reason: string | null;
}

export interface UnclassifiedRunView {
  run_id: string;
  period_id: string | null;
  completed_at: string;
  reason: string;
}

export interface CoverageReportView {
  control_id: string;
  window_start: string;
  window_end: string;
  periods: PeriodCoverageView[];
  unclassified_runs: UnclassifiedRunView[];
}

export const currentPeriodStatus = (today?: string | null) =>
  invoke<CurrentPeriodStatus[]>("current_period_status", {
    today: today ?? null,
  });

export const coverage = (
  controlId: string,
  from?: string | null,
  to?: string | null,
  today?: string | null,
) =>
  invoke<CoverageReportView>("coverage", {
    controlId,
    from: from ?? null,
    to: to ?? null,
    today: today ?? null,
  });

export type ScheduleReason =
  | "cadence"
  | "override-due"
  | "override-insert"
  | "override-weekday"
  | "override-skip";

export interface ScheduleEntryView {
  control_id: string;
  cadence: string;
  date: string;
  reason: ScheduleReason;
  note: string | null;
  overdue: boolean;
}

export const scheduleView = (today?: string | null) =>
  invoke<ScheduleEntryView[]>("schedule_view", { today: today ?? null });

export const getInventory = (today?: string | null) =>
  invoke<InventoryView>("get_inventory", { today: today ?? null });

export const listRuns = (control_id?: string | null, quarter?: string | null) =>
  invoke<RunRow[]>("list_runs", {
    controlId: control_id ?? null,
    quarter: quarter ?? null,
  });

export const recentRuns = (limit: number) =>
  invoke<RunRow[]>("recent_runs", { limit });

export const getRun = (control_id: string, run_id: string) =>
  invoke<RunDetail>("get_run", { controlId: control_id, runId: run_id });

export interface FindingsRow {
  control_id: string;
  run_id: string;
  path: string;
  year: number;
  quarter: number;
  completed_at: string | null;
  run_state: RunState;
  bytes: number;
}

export interface FindingsHtml {
  control_id: string;
  run_id: string;
  path: string;
  html: string;
}

export const listFindings = (
  control_id?: string | null,
  quarter?: string | null,
) =>
  invoke<FindingsRow[]>("list_findings", {
    controlId: control_id ?? null,
    quarter: quarter ?? null,
  });

export const readFindings = (control_id: string, run_id: string) =>
  invoke<FindingsHtml>("read_findings", {
    controlId: control_id,
    runId: run_id,
  });

export type ArtifactKind =
  | "markdown"
  | "json"
  | "yaml"
  | "text"
  | "binary"
  | "too-large"
  | "image";

export interface ArtifactView {
  path: string;
  bytes: number;
  kind: ArtifactKind;
  text: string | null;
  html: string | null;
}

export const readArtifact = (path: string) =>
  invoke<ArtifactView>("read_artifact", { path });

export interface SearchHit {
  kind: string;
  id: string;
  title: string;
  path: string;
  status: string | null;
  score: number;
}

export interface IndexStatus {
  ready: boolean;
  doc_count: number;
  last_updated: string;
}

export const searchPalette = (
  query: string,
  limit: number = 30,
  kinds: string[] = [],
) => invoke<SearchHit[]>("search_palette", { query, limit, kinds });

export const indexStatus = () => invoke<IndexStatus>("index_status");
