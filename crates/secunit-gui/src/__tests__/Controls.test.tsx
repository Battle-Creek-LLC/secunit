import { render, screen, fireEvent, act } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Controls } from "@/routes/Controls";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

const mkControl = (id: string, status: string = "sealed", title?: string) => ({
  id,
  title: title ?? id,
  cadence: "weekly",
  owner: "owner@example",
  status,
  next_due: null,
  overdue: status === "overdue",
  last_run_id: null,
  last_run_at: null,
  last_status: null,
});

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Controls", () => {
  async function setup(controls: ReturnType<typeof mkControl>[]) {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return controls;
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });
  }

  it("filters by status from URL search params", async () => {
    await setup([
      mkControl("one", "overdue", "First control"),
      mkControl("two", "sealed", "Second control"),
      mkControl("three", "due-soon", "Third control"),
    ]);

    render(
      <MemoryRouter initialEntries={["/controls?status=overdue"]}>
        <Controls />
      </MemoryRouter>,
    );

    expect(screen.getByText("First control")).toBeInTheDocument();
    expect(screen.queryByText("Second control")).not.toBeInTheDocument();
    expect(screen.queryByText("Third control")).not.toBeInTheDocument();
    expect(screen.getByText(/1 of 3/)).toBeInTheDocument();
  });

  it("free-text search filters by title and id", async () => {
    await setup([
      mkControl("aa-weekly-audit-review", "sealed", "Audit log review"),
      mkControl("sca-weekly-dependency-scan", "sealed", "Dependency scan"),
    ]);

    render(
      <MemoryRouter initialEntries={["/controls"]}>
        <Controls />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText("search"), {
      target: { value: "scan" },
    });

    expect(screen.getByText("Dependency scan")).toBeInTheDocument();
    expect(screen.queryByText("Audit log review")).not.toBeInTheDocument();
  });

  it("selects a row on click and fetches the detail", async () => {
    let detailCalls = 0;
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_controls")
        return [mkControl("aa-weekly-audit-review", "sealed", "Audit log review")];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "get_control") {
        detailCalls += 1;
        return {
          summary: mkControl((args as { id: string }).id, "sealed", "Audit log review"),
          policy: "security/policy.md",
          nist: ["AU-1"],
          skill: "skill",
          references: [],
          recent_runs: [],
          resolved_scope_today: [],
        };
      }
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter initialEntries={["/controls"]}>
        <Controls />
      </MemoryRouter>,
    );

    expect(
      screen.getByText("Select a control to inspect."),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByText("Audit log review"));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(detailCalls).toBeGreaterThan(0);
  });
});
