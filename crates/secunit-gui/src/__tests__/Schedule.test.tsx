import { render, screen, act, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Schedule } from "@/routes/Schedule";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Schedule", () => {
  it("renders both tabs and a list of upcoming firings", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "schedule_view")
        return [
          {
            control_id: "aa-weekly-audit-review",
            cadence: "weekly",
            date: "2026-05-04",
            reason: "cadence",
            note: null,
            overdue: false,
          },
          {
            control_id: "ra-2026-12-pentest",
            cadence: "scheduled",
            date: "2026-12-01",
            reason: "override-insert",
            note: "annual pentest",
            overdue: false,
          },
        ];
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter>
        <Schedule />
      </MemoryRouter>,
    );

    expect(screen.getByRole("tab", { name: "Calendar" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "List" })).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByText(/2 upcoming firings/),
      ).toBeInTheDocument(),
    );
  });

  it("flags overdue entries", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "schedule_view")
        return [
          {
            control_id: "ac-annual-access-review",
            cadence: "annual",
            date: "2025-12-31",
            reason: "cadence",
            note: null,
            overdue: true,
          },
        ];
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter>
        <Schedule />
      </MemoryRouter>,
    );
    await waitFor(() =>
      expect(screen.getByText(/1 upcoming firing · 1 overdue/)).toBeInTheDocument(),
    );
  });
});
