import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect } from "vitest";
import { AppShell } from "@/components/AppShell";
import type { ProjectsView } from "@/lib/ipc";

const view: ProjectsView = {
  projects: [
    { name: "acme", path: "~/a", resolved_path: "/home/op/a", exists: true },
  ],
  default: "acme",
  last_selected: null,
  config_path: "/cfg.yaml",
};

describe("AppShell", () => {
  it("renders the six nav links and search trigger", () => {
    render(
      <MemoryRouter initialEntries={["/overview"]}>
        <AppShell view={view} selected="acme" onSelect={() => {}} appVersion="0.1.0">
          <div>content</div>
        </AppShell>
      </MemoryRouter>,
    );

    for (const label of [
      "Overview",
      "Schedule",
      "Controls",
      "Findings",
      "Evidence",
      "Inventory",
    ]) {
      expect(screen.getByRole("link", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("button", { name: /search controls/i }),
    ).toBeInTheDocument();
  });

  it("highlights the active route", () => {
    render(
      <MemoryRouter initialEntries={["/controls"]}>
        <AppShell view={view} selected="acme" onSelect={() => {}} appVersion="0.1.0">
          <div>content</div>
        </AppShell>
      </MemoryRouter>,
    );
    const active = screen.getByText("Controls");
    expect(active).toHaveAttribute("aria-current", "page");
    const inactive = screen.getByText("Overview");
    expect(inactive).not.toHaveAttribute("aria-current");
  });
});
