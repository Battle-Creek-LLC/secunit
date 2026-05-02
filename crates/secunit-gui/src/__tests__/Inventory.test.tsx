import { render, screen, act, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Inventory } from "@/routes/Inventory";
import { store } from "@/store";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Inventory", () => {
  it("renders sections per kind with active counts", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "recent_runs") return [];
      if (cmd === "get_inventory")
        return {
          kinds: [
            {
              kind: "source_repos",
              entries: [
                {
                  name: "alpha",
                  tags: ["has-sca"],
                  in_scope_since: null,
                  retired_on: null,
                  aliases: [],
                  active_today: true,
                  extras: { url: "https://github.com/x/alpha" },
                },
                {
                  name: "beta",
                  tags: ["has-sca"],
                  in_scope_since: null,
                  retired_on: "2025-12-01",
                  aliases: [],
                  active_today: false,
                  extras: {},
                },
              ],
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
        <Inventory />
      </MemoryRouter>,
    );

    expect(screen.getByText("source_repos")).toBeInTheDocument();
    expect(screen.getByText(/1 active · 2 total/)).toBeInTheDocument();
    expect(screen.getByText("alpha")).toBeInTheDocument();
    expect(screen.getByText("beta")).toBeInTheDocument();
    expect(screen.getByText("retired")).toBeInTheDocument();
  });

  it("filters by search query across tags and extras", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "recent_runs") return [];
      if (cmd === "get_inventory")
        return {
          kinds: [
            {
              kind: "source_repos",
              entries: [
                {
                  name: "alpha",
                  tags: ["has-sca"],
                  in_scope_since: null,
                  retired_on: null,
                  aliases: [],
                  active_today: true,
                  extras: { stack: "rust" },
                },
                {
                  name: "beta",
                  tags: ["has-sast"],
                  in_scope_since: null,
                  retired_on: null,
                  aliases: [],
                  active_today: true,
                  extras: { stack: "ts" },
                },
              ],
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
        <Inventory />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText("search"), {
      target: { value: "rust" },
    });
    await waitFor(() => expect(screen.queryByText("beta")).not.toBeInTheDocument());
    expect(screen.getByText("alpha")).toBeInTheDocument();
  });
});
