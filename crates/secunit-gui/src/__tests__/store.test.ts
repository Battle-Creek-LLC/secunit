import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

const mkControl = (id: string, title = id) => ({
  id,
  title,
  cadence: "weekly",
  owner: "owner@example",
  status: "sealed" as const,
  next_due: null,
  overdue: false,
  last_run_id: null,
  last_run_at: null,
  last_status: null,
});

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("store", () => {
  it("primes from the IPC bridge", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [mkControl("a"), mkControl("b")];
      if (cmd === "due_rows")
        return [
          { control_id: "a", cadence: "weekly", next_due: null, overdue: false },
        ];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected ${cmd}`);
    });
    await store.prime();
    const s = store.getSnapshot();
    expect(s.controls.size).toBe(2);
    expect(s.due.size).toBe(1);
    expect(s.inventory).toEqual({ kinds: [] });
    expect(s.revision).toBe(1);
  });

  it("apply control_changed re-fetches controls and bumps revision", async () => {
    let titleA = "first";
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [mkControl("a", titleA)];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected ${cmd}`);
    });
    await store.prime();
    expect(store.getSnapshot().controls.get("a")?.title).toBe("first");

    titleA = "second";
    await store.apply({ kind: "control_changed", id: "a", path: "/r/controls/a.yaml" });
    expect(store.getSnapshot().controls.get("a")?.title).toBe("second");
    expect(store.getSnapshot().revision).toBe(2);
  });

  it("notifies subscribers on apply", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [mkControl("a")];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected ${cmd}`);
    });

    const calls: number[] = [];
    const unsubscribe = store.subscribe(() => {
      calls.push(store.getSnapshot().revision);
    });
    await store.prime();
    await store.apply({
      kind: "run_state_changed",
      control_id: "a",
      run_id: "run-001",
      change: "sealed",
    });
    unsubscribe();
    expect(calls).toEqual([1, 2]);
  });

  it("inventory_changed only refetches inventory", async () => {
    let inv = { kinds: [{ kind: "source_repos", entries: [] }] };
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [mkControl("a")];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return inv;
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected ${cmd}`);
    });
    await store.prime();

    inv = { kinds: [{ kind: "cloud_accounts", entries: [] }] };
    await store.apply({ kind: "inventory_changed", path: "/r/inventory.yaml" });
    expect(store.getSnapshot().inventory?.kinds[0]?.kind).toBe("cloud_accounts");
  });
});
