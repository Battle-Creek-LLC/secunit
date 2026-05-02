// Mirrors `crates/secunit-gui/src/watcher.rs#WatcherEvent`. Events are
// emitted by the Rust watcher via `tauri::AppHandle::emit` against
// the topic name (`control_changed`, …); the payload follows the
// `#[serde(tag="kind")]` enum shape but topics are dispatched per-name
// rather than reading the discriminator twice.

export type RunChange = "sealed" | "prepared-or-modified";

export interface ControlChanged {
  kind: "control_changed";
  id: string;
  path: string;
}

export interface InventoryChanged {
  kind: "inventory_changed";
  path: string;
}

export interface StateJsonChanged {
  kind: "state_json_changed";
  path: string;
}

export interface RunStateChanged {
  kind: "run_state_changed";
  control_id: string;
  run_id: string;
  change: RunChange;
}

export interface FindingsChanged {
  kind: "findings_changed";
  control_id: string;
  run_id: string;
  path: string;
}

export type WatcherEvent =
  | ControlChanged
  | InventoryChanged
  | StateJsonChanged
  | RunStateChanged
  | FindingsChanged;

export const TOPICS = [
  "control_changed",
  "inventory_changed",
  "state_json_changed",
  "run_state_changed",
  "findings_changed",
] as const;

export type WatcherTopic = (typeof TOPICS)[number];
