import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Overview } from "@/routes/Overview";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

interface ControlSummaryFixture {
  status?: "sealed" | "overdue" | "due-soon" | "in-progress" | "aborted" | "never-run" | "idle";
}

const mkControl = (id: string, opts: ControlSummaryFixture = {}) => ({
  id,
  title: id,
  cadence: "weekly",
  owner: "owner@example",
  status: opts.status ?? "sealed",
  next_due: null,
  overdue: opts.status === "overdue",
  last_run_id: null,
  last_run_at: null,
  last_status: null,
});

describe("Overview", () => {
  it("renders four health tiles with computed counts", async () => {
    const today = "2026-05-01T12:00:00Z";
    vi.useFakeTimers();
    vi.setSystemTime(new Date(today));

    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls")
        return [mkControl("a", { status: "overdue" }), mkControl("b")];
      if (cmd === "due_rows")
        return [
          { control_id: "b", cadence: "weekly", next_due: "2026-05-04", overdue: false },
        ];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs")
        return [
          {
            control_id: "b",
            run_id: "r1",
            run_dir: "/x",
            state: "sealed",
            started_at: "2026-04-29T00:00:00Z",
            completed_at: "2026-04-29T00:30:00Z",
            manifest_sha256: "abc",
            year: 2026,
            quarter: 2,
          },
          {
            control_id: "a",
            run_id: "r2",
            run_dir: "/x",
            state: "pending",
            started_at: "2026-05-01T08:00:00Z",
            completed_at: null,
            manifest_sha256: null,
            year: 2026,
            quarter: 2,
          },
        ];
      throw new Error(`unexpected: ${cmd}`);
    });

    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter>
        <Overview />
      </MemoryRouter>,
    );

    expect(screen.getByText("Overdue").nextSibling).toHaveTextContent("1");
    expect(screen.getByText("Due this week").nextSibling).toHaveTextContent("1");
    expect(screen.getByText("In progress").nextSibling).toHaveTextContent("1");
    expect(screen.getByText("Sealed last 30d").nextSibling).toHaveTextContent("1");

    vi.useRealTimers();
  });

  it("links tiles to their detail views", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });
    render(
      <MemoryRouter>
        <Overview />
      </MemoryRouter>,
    );
    expect(
      screen.getByRole("link", { name: /Overdue/i }),
    ).toHaveAttribute("href", "/controls?status=overdue");
    expect(screen.getByRole("link", { name: /Due this week/i })).toHaveAttribute(
      "href",
      "/schedule",
    );
  });
});
