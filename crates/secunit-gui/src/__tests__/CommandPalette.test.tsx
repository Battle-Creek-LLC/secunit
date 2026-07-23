import { render, screen, act, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CommandPalette } from "@/components/CommandPalette";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockedInvoke.mockReset();
});

describe("CommandPalette", () => {
  it("renders nothing when closed", () => {
    render(
      <MemoryRouter>
        <CommandPalette open={false} onClose={() => {}} />
      </MemoryRouter>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("queries the palette and groups results by kind", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "search_palette")
        return [
          {
            kind: "control",
            id: "aa-weekly-audit-review",
            title: "Audit log review",
            path: "controls/aa.yaml",
            status: null,
            score: 4.2,
          },
          {
            kind: "finding",
            id: "aa/run-001",
            title: "Audit log review — week 18",
            path: "/x/findings.md",
            status: null,
            score: 2.0,
          },
        ];
      throw new Error(`unexpected: ${cmd}`);
    });

    render(
      <MemoryRouter>
        <CommandPalette open onClose={() => {}} />
      </MemoryRouter>,
    );

    await act(async () => {
      fireEvent.change(screen.getByRole("textbox"), { target: { value: "audit" } });
    });
    // Debounce + microtask flush.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 100));
    });

    await waitFor(() => expect(screen.getByText("Controls")).toBeInTheDocument());
    expect(screen.getByText("Findings")).toBeInTheDocument();
    expect(screen.getByText("Audit log review")).toBeInTheDocument();
    expect(screen.getByText("Audit log review — week 18")).toBeInTheDocument();
  });

  it("closes on Escape", async () => {
    mockedInvoke.mockImplementation(async () => []);
    const onClose = vi.fn();
    render(
      <MemoryRouter>
        <CommandPalette open onClose={onClose} />
      </MemoryRouter>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
