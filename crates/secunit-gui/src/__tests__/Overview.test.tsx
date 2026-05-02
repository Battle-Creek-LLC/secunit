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
  status?: "sealed" | "overdue" | "due-soon" | "in-progress" | "failed" | "never-run" | "idle";
  next_due?: string | null;
}

const mkControl = (id: string, opts: ControlSummaryFixture = {}) => ({
  id,
  title: id,
  cadence: "weekly",
  owner: "owner@example",
  status: opts.status ?? "sealed",
  next_due: opts.next_due ?? null,
  overdue: opts.status === "overdue",
  last_run_id: null,
  last_run_at: null,
  last_status: null,
});

describe("Overview", () => {
  it("renders the focus and how-am-i-doing sections", async () => {
    const today = "2026-05-01T12:00:00Z";
    vi.useFakeTimers();
    vi.setSystemTime(new Date(today));

    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls")
        return [
          mkControl("a", { status: "overdue", next_due: "2026-04-28" }),
          mkControl("b", { status: "due-soon", next_due: "2026-05-04" }),
          mkControl("c"),
        ];
      if (cmd === "due_rows")
        return [
          { control_id: "b", cadence: "weekly", next_due: "2026-05-04", overdue: false },
        ];
      if (cmd === "current_period_status")
        return [
          {
            control_id: "a",
            cadence: "weekly",
            period_id: "2026-W17",
            period_start: "2026-04-20",
            period_end: "2026-04-26",
            status: "gap",
            satisfied_by_run_id: null,
            late: false,
          },
          {
            control_id: "b",
            cadence: "weekly",
            period_id: "2026-W18",
            period_start: "2026-04-27",
            period_end: "2026-05-03",
            status: "open",
            satisfied_by_run_id: null,
            late: false,
          },
          {
            control_id: "c",
            cadence: "weekly",
            period_id: "2026-W18",
            period_start: "2026-04-27",
            period_end: "2026-05-03",
            status: "satisfied",
            satisfied_by_run_id: "r1",
            late: false,
          },
        ];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs")
        return [
          {
            control_id: "c",
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
            started_at: "2026-04-25T08:00:00Z",
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

    expect(screen.getByRole("heading", { name: /Overview/ })).toBeInTheDocument();
    expect(screen.getByText(/Focus now/i)).toBeInTheDocument();
    expect(screen.getByText(/How am I doing/i)).toBeInTheDocument();

    // Alert strip — overdue / due / stalled link counts
    expect(screen.getByRole("link", { name: /^1 overdue$/i })).toHaveAttribute(
      "href",
      "/controls?status=overdue",
    );
    expect(screen.getByRole("link", { name: /due this week/i })).toHaveAttribute(
      "href",
      "/schedule",
    );

    // Focus list shows the overdue control with its badge text
    expect(screen.getAllByText(/Overdue/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText("a").length).toBeGreaterThan(0);

    // Focus row links filter+select on the controls page so the right
    // detail pane opens on the row the operator clicked.
    const focusLinks = screen
      .getAllByRole("link")
      .filter((el) => el.getAttribute("href") === "/controls?q=a&id=a");
    expect(focusLinks.length).toBeGreaterThan(0);

    // Recent runs: the row text deep-links to evidence with both ids,
    // and each row carries icon links to control + evidence.
    const evidenceLinks = screen
      .getAllByRole("link")
      .filter((el) => el.getAttribute("href") === "/evidence?control=c&run=r1");
    expect(evidenceLinks.length).toBe(2); // row text + evidence icon
    expect(
      screen.getByRole("link", { name: /open control c/i }),
    ).toHaveAttribute("href", "/controls?q=c&id=c");
    expect(
      screen.getByRole("link", { name: /open evidence for r1/i }),
    ).toHaveAttribute("href", "/evidence?control=c&run=r1");

    // Coverage card — 2 of 3 on track-ish (sealed + due-soon) ⇒ 67%
    expect(screen.getByText("67%")).toBeInTheDocument();

    vi.useRealTimers();
  });

  it("shows an all-clear state when nothing needs attention", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [mkControl("ok")];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
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
    expect(screen.getByText(/All clear/i)).toBeInTheDocument();
    expect(screen.getByText(/Nothing needs attention/i)).toBeInTheDocument();
  });
});
