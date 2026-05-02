import { render, screen, act, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Findings } from "@/routes/Findings";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Findings", () => {
  it("renders one card per finding and the rendered HTML", async () => {
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_controls")
        return [
          {
            id: "aa",
            title: "Audit",
            cadence: "weekly",
            owner: "x",
            status: "sealed",
            next_due: null,
            overdue: false,
            last_run_id: null,
            last_run_at: null,
            last_status: null,
          },
        ];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "list_findings")
        return [
          {
            control_id: "aa",
            run_id: "2026-05-01-run-001",
            path: "/x/findings.md",
            year: 2026,
            quarter: 2,
            completed_at: "2026-05-01T10:00:00Z",
            run_state: "sealed",
            bytes: 100,
          },
        ];
      if (cmd === "read_findings")
        return {
          control_id: (args as { controlId: string }).controlId,
          run_id: (args as { runId: string }).runId,
          path: "/x/findings.md",
          html: "<h1>Top finding</h1><p>Body text.</p>",
        };
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter>
        <Findings />
      </MemoryRouter>,
    );

    await waitFor(() =>
      expect(screen.getByText(/1 finding/)).toBeInTheDocument(),
    );
    // The sanitised HTML lands via dangerouslySetInnerHTML.
    await waitFor(() =>
      expect(screen.getByText("Top finding")).toBeInTheDocument(),
    );
    expect(screen.getByText("Body text.")).toBeInTheDocument();
  });

  it("filters by free-text query", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "list_findings")
        return [
          {
            control_id: "aa",
            run_id: "r1",
            path: "/p1/findings.md",
            year: 2026,
            quarter: 2,
            completed_at: null,
            run_state: "sealed",
            bytes: 0,
          },
          {
            control_id: "bb",
            run_id: "r2",
            path: "/p2/findings.md",
            year: 2026,
            quarter: 2,
            completed_at: null,
            run_state: "sealed",
            bytes: 0,
          },
        ];
      if (cmd === "read_findings")
        return { control_id: "x", run_id: "y", path: "/p", html: "<p></p>" };
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });
    render(
      <MemoryRouter>
        <Findings />
      </MemoryRouter>,
    );
    await waitFor(() => expect(screen.getByText(/2 findings/)).toBeInTheDocument());

    fireEvent.change(screen.getByLabelText("search"), { target: { value: "aa" } });
    await waitFor(() =>
      expect(screen.getByText(/1 finding/)).toBeInTheDocument(),
    );
  });
});
