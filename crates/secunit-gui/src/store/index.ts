// Reactive store for the GUI. Keyed by `control_id` and `run_id` so
// views can subscribe to a single slice without re-rendering on
// unrelated changes. Refetches via the IPC layer on every event the
// watcher fires — `secunit-core` remains the single source of truth.

import { useEffect, useState, useSyncExternalStore } from "react";
import {
  dueRows,
  getInventory,
  listControls,
  recentRuns,
  type ControlSummary,
  type DueRowView,
  type InventoryView,
  type RunRow,
} from "@/lib/ipc";
import type { WatcherEvent } from "./events";

export interface StoreState {
  controls: Map<string, ControlSummary>;
  due: Map<string, DueRowView>;
  inventory: InventoryView | null;
  runs: RunRow[];
  /** Bumped on every successful refresh — handy for downstream memoisation. */
  revision: number;
}

const initialState = (): StoreState => ({
  controls: new Map(),
  due: new Map(),
  inventory: null,
  runs: [],
  revision: 0,
});

type Listener = () => void;

class Store {
  private state: StoreState = initialState();
  private listeners = new Set<Listener>();

  getSnapshot = (): StoreState => this.state;

  subscribe = (l: Listener) => {
    this.listeners.add(l);
    return () => {
      this.listeners.delete(l);
    };
  };

  reset() {
    this.state = initialState();
    this.notify();
  }

  /** Prime after `load_project` succeeds. */
  async prime() {
    const [controls, due, inventory, runs] = await Promise.all([
      listControls(),
      dueRows(),
      getInventory(),
      recentRuns(50),
    ]);
    this.state = {
      controls: new Map(controls.map((c) => [c.id, c])),
      due: new Map(due.map((d) => [d.control_id, d])),
      inventory,
      runs,
      revision: this.state.revision + 1,
    };
    this.notify();
  }

  /** Apply a watcher event by re-fetching the affected slice. */
  async apply(event: WatcherEvent) {
    switch (event.kind) {
      case "control_changed": {
        // Re-fetch the whole control list for now — title/cadence/owner
        // can all change in one edit. Cheap; refine if profiles say so.
        const next = await listControls();
        const due = await dueRows();
        this.state = {
          ...this.state,
          controls: new Map(next.map((c) => [c.id, c])),
          due: new Map(due.map((d) => [d.control_id, d])),
          revision: this.state.revision + 1,
        };
        break;
      }
      case "state_json_changed": {
        const next = await listControls();
        this.state = {
          ...this.state,
          controls: new Map(next.map((c) => [c.id, c])),
          revision: this.state.revision + 1,
        };
        break;
      }
      case "inventory_changed": {
        const inventory = await getInventory();
        this.state = {
          ...this.state,
          inventory,
          revision: this.state.revision + 1,
        };
        break;
      }
      case "run_state_changed":
      case "findings_changed": {
        const runs = await recentRuns(50);
        const controls = await listControls();
        this.state = {
          ...this.state,
          runs,
          controls: new Map(controls.map((c) => [c.id, c])),
          revision: this.state.revision + 1,
        };
        break;
      }
    }
    this.notify();
  }

  private notify() {
    for (const l of this.listeners) {
      l();
    }
  }
}

export const store = new Store();

/**
 * Returns the whole `StoreState` snapshot. Stable across renders unless
 * the store mutated — `useSyncExternalStore` requires a referentially
 * stable getter, so we only swap the snapshot inside Store.notify().
 *
 * Consumers shape with `useMemo` (or a derived hook). Avoid passing a
 * selector here that returns a freshly computed array/object on every
 * call; that will spin React into an infinite render loop.
 */
export function useStore(): StoreState {
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}

/**
 * Subscribe to a single control by id. Re-renders only when the keyed
 * slice changes (compares by reference; `apply` always swaps the Map).
 */
export function useControl(id: string | null): ControlSummary | null {
  const [snapshot, setSnapshot] = useState<ControlSummary | null>(() =>
    id == null ? null : store.getSnapshot().controls.get(id) ?? null,
  );
  useEffect(() => {
    if (id == null) {
      setSnapshot(null);
      return;
    }
    const update = () => {
      const next = store.getSnapshot().controls.get(id) ?? null;
      setSnapshot((prev) => (prev === next ? prev : next));
    };
    update();
    return store.subscribe(update);
  }, [id]);
  return snapshot;
}
