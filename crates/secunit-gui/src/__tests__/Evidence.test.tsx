import { render, screen, act, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Evidence } from "@/routes/Evidence";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Evidence", () => {
  it("groups runs in a tree and selects on click", async () => {
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs")
        return [
          {
            control_id: "aa",
            run_id: "2026-05-04-run-001",
            run_dir: "/r1",
            state: "sealed",
            started_at: "2026-05-04T08:00:00Z",
            completed_at: "2026-05-04T09:00:00Z",
            manifest_sha256: "abc123",
            year: 2026,
            quarter: 2,
          },
        ];
      if (cmd === "get_run")
        return {
          row: {
            control_id: (args as { controlId: string }).controlId,
            run_id: (args as { runId: string }).runId,
            run_dir: "/r1",
            state: "sealed",
            started_at: "2026-05-04T08:00:00Z",
            completed_at: "2026-05-04T09:00:00Z",
            manifest_sha256: "abc123",
            year: 2026,
            quarter: 2,
          },
          manifest: {},
          prepare: null,
          abort: null,
          tree: [
            {
              name: "manifest.json",
              path: "/r1/manifest.json",
              kind: "file",
              size: 256,
              children: [],
            },
          ],
        };
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter>
        <Evidence />
      </MemoryRouter>,
    );

    // Year + quarter open by default. Click control to expand, then run.
    fireEvent.click(screen.getByText("aa"));
    fireEvent.click(screen.getByText("2026-05-04-run-001"));

    await waitFor(() =>
      expect(screen.getByText("manifest.json")).toBeInTheDocument(),
    );
    // Selecting the run swaps the placeholder for run summary.
    expect(
      screen.queryByText("Select a run from the tree."),
    ).not.toBeInTheDocument();
  });

  it("hydrates the selection from ?control=&run= URL params", async () => {
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs")
        return [
          {
            control_id: "aa",
            run_id: "2026-05-04-run-001",
            run_dir: "/r1",
            state: "sealed",
            started_at: "2026-05-04T08:00:00Z",
            completed_at: "2026-05-04T09:00:00Z",
            manifest_sha256: "abc123",
            year: 2026,
            quarter: 2,
          },
        ];
      if (cmd === "get_run")
        return {
          row: {
            control_id: (args as { controlId: string }).controlId,
            run_id: (args as { runId: string }).runId,
            run_dir: "/r1",
            state: "sealed",
            started_at: "2026-05-04T08:00:00Z",
            completed_at: "2026-05-04T09:00:00Z",
            manifest_sha256: "abc123",
            year: 2026,
            quarter: 2,
          },
          manifest: {},
          prepare: null,
          abort: null,
          tree: [
            {
              name: "manifest.json",
              path: "/r1/manifest.json",
              kind: "file",
              size: 256,
              children: [],
            },
          ],
        };
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter
        initialEntries={["/evidence?control=aa&run=2026-05-04-run-001"]}
      >
        <Evidence />
      </MemoryRouter>,
    );

    // Without any tree clicks, the run named in the URL should already
    // be loaded — the right pane shows manifest.json from get_run, and
    // the placeholder is gone.
    await waitFor(() =>
      expect(screen.getByText("manifest.json")).toBeInTheDocument(),
    );
    expect(
      screen.queryByText("Select a run from the tree."),
    ).not.toBeInTheDocument();
  });
});
