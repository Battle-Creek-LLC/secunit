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
  | "aborted"
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

export type RunState = "sealed" | "aborted" | "pending";

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
