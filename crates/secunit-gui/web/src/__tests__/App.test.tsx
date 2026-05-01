import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { App } from "../App";

const mockedInvoke = vi.mocked(invoke);

describe("App — bootstrap", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("shows the empty-config explainer when no projects are configured", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_projects") {
        return {
          projects: [],
          default: null,
          last_selected: null,
          config_path: "/home/op/.config/secunit-gui/projects.yaml",
        };
      }
      throw new Error(`unexpected: ${cmd}`);
    });

    render(<App />);

    expect(
      await screen.findByRole("heading", { name: /no projects configured/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("/home/op/.config/secunit-gui/projects.yaml"),
    ).toBeInTheDocument();
  });

  it("renders the switcher and preselects last_selected over default", async () => {
    const view = {
      projects: [
        { name: "acme", path: "~/a", resolved_path: "/home/op/a", exists: true },
        { name: "widgets", path: "~/w", resolved_path: "/home/op/w", exists: true },
      ],
      default: "acme",
      last_selected: "widgets",
      config_path: "/home/op/.config/secunit-gui/projects.yaml",
    };
    const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];
    mockedInvoke.mockImplementation(async (cmd, args) => {
      calls.push({ cmd, args: args as Record<string, unknown> });
      if (cmd === "list_projects") return view;
      if (cmd === "select_project") return (args as { name: string }).name;
      throw new Error(`unexpected: ${cmd}`);
    });

    render(<App />);

    const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
    expect(select.value).toBe("widgets");
    expect(calls.some((c) => c.cmd === "select_project" && c.args?.name === "widgets")).toBe(
      true,
    );
  });

  it("falls back to the declared default when no last_selected exists", async () => {
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_projects")
        return {
          projects: [
            { name: "a", path: "/a", resolved_path: "/a", exists: true },
            { name: "b", path: "/b", resolved_path: "/b", exists: true },
          ],
          default: "b",
          last_selected: null,
          config_path: "/cfg.yaml",
        };
      if (cmd === "select_project") return (args as { name: string }).name;
      throw new Error(`unexpected: ${cmd}`);
    });

    render(<App />);
    const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
    expect(select.value).toBe("b");
  });

  it("invokes select_project when the user picks another project", async () => {
    const view = {
      projects: [
        { name: "a", path: "/a", resolved_path: "/a", exists: true },
        { name: "b", path: "/b", resolved_path: "/b", exists: true },
      ],
      default: "a",
      last_selected: null,
      config_path: "/cfg.yaml",
    };
    const select_calls: string[] = [];
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_projects") return view;
      if (cmd === "select_project") {
        const n = (args as { name: string }).name;
        select_calls.push(n);
        return n;
      }
      throw new Error(`unexpected: ${cmd}`);
    });

    render(<App />);
    const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("a"));
    fireEvent.change(select, { target: { value: "b" } });
    await waitFor(() => expect(select.value).toBe("b"));
    expect(select_calls).toEqual(["a", "b"]);
  });

  it("renders the error card when list_projects fails", async () => {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_projects") throw "parse /cfg.yaml: yaml: bad token at 3:7";
      throw new Error(`unexpected: ${cmd}`);
    });
    render(<App />);
    expect(
      await screen.findByRole("heading", { name: /failed to read projects.yaml/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/yaml: bad token at 3:7/)).toBeInTheDocument();
  });

  it("flags missing project paths in the switcher", async () => {
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_projects")
        return {
          projects: [
            { name: "real", path: "/r", resolved_path: "/r", exists: true },
            { name: "ghost", path: "/g", resolved_path: "/g", exists: false },
          ],
          default: "real",
          last_selected: null,
          config_path: "/cfg.yaml",
        };
      if (cmd === "select_project") return (args as { name: string }).name;
      throw new Error(`unexpected: ${cmd}`);
    });
    render(<App />);
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "ghost (missing)" })).toBeInTheDocument();
  });
});
