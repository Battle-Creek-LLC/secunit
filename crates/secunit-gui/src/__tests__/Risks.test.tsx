import { render, screen, fireEvent, act } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Risks } from "@/routes/Risks";
import { store } from "@/store";
import type { RiskRow } from "@/lib/ipc";

const mockedInvoke = vi.mocked(invoke);

const mkRisk = (over: Partial<RiskRow> & { id: string }): RiskRow => ({
  id: over.id,
  title: over.title ?? over.id,
  fingerprint: over.fingerprint ?? `ctrl:${over.id}`,
  severity: over.severity ?? "high",
  status: over.status ?? "open",
  owner: over.owner ?? "cto",
  due_at: over.due_at ?? null,
  source_control: over.source_control ?? "ra-vuln-audit",
  first_run_id: over.first_run_id ?? "2026-05-25-run-001",
  external: over.external ?? [],
  log_head_sha256: over.log_head_sha256 ?? "deadbeef",
});

beforeEach(() => {
  mockedInvoke.mockReset();
  store.reset();
});

describe("Risks", () => {
  async function setup(risks: RiskRow[]) {
    mockedInvoke.mockImplementation(async (cmd) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "list_risks") return risks;
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });
  }

  it("renders the register table with rows", async () => {
    await setup([
      mkRisk({ id: "R-0001", title: "Pickle RCE", severity: "critical" }),
      mkRisk({ id: "R-0002", title: "Weak TLS", severity: "medium" }),
    ]);

    render(
      <MemoryRouter initialEntries={["/risks"]}>
        <Risks />
      </MemoryRouter>,
    );

    expect(screen.getByText("Pickle RCE")).toBeInTheDocument();
    expect(screen.getByText("Weak TLS")).toBeInTheDocument();
    expect(screen.getByText(/2 of 2/)).toBeInTheDocument();
  });

  it("flags an overdue open risk as past SLA", async () => {
    await setup([
      mkRisk({
        id: "R-0001",
        title: "Pickle RCE",
        status: "open",
        due_at: "2000-01-01",
      }),
    ]);

    render(
      <MemoryRouter initialEntries={["/risks"]}>
        <Risks />
      </MemoryRouter>,
    );

    expect(screen.getByText(/overdue/)).toBeInTheDocument();
  });

  it("does not flag a remediated risk as overdue", async () => {
    await setup([
      mkRisk({
        id: "R-0001",
        title: "Pickle RCE",
        status: "remediated",
        due_at: "2000-01-01",
      }),
    ]);

    render(
      <MemoryRouter initialEntries={["/risks"]}>
        <Risks />
      </MemoryRouter>,
    );

    expect(screen.queryByText(/overdue/)).not.toBeInTheDocument();
    expect(screen.getByText(/closed/)).toBeInTheDocument();
  });

  it("filters to past-SLA from URL search params", async () => {
    await setup([
      mkRisk({ id: "R-0001", title: "Lapsed", status: "open", due_at: "2000-01-01" }),
      mkRisk({ id: "R-0002", title: "Fresh", status: "open", due_at: "2999-01-01" }),
    ]);

    render(
      <MemoryRouter initialEntries={["/risks?status=past-sla"]}>
        <Risks />
      </MemoryRouter>,
    );

    expect(screen.getByText("Lapsed")).toBeInTheDocument();
    expect(screen.queryByText("Fresh")).not.toBeInTheDocument();
  });

  it("selects a row on click and fetches the detail", async () => {
    let detailCalls = 0;
    mockedInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === "list_controls") return [];
      if (cmd === "due_rows") return [];
      if (cmd === "current_period_status") return [];
      if (cmd === "get_inventory") return { kinds: [] };
      if (cmd === "recent_runs") return [];
      if (cmd === "list_risks")
        return [mkRisk({ id: "R-0001", title: "Pickle RCE" })];
      if (cmd === "get_risk") {
        detailCalls += 1;
        return {
          id: (args as { id: string }).id,
          title: "Pickle RCE",
          severity: "critical",
          status: "open",
          impact: 5,
          likelihood: 4,
          owner: "cto",
          due_at: "2026-06-24",
          sla_days: 30,
          affected_systems: [],
          source_control: "ra-vuln-audit",
          first_run_id: "2026-05-25-run-001",
          fingerprint: "ra-vuln-audit:S032",
          resolved_at: null,
          exception_expires_at: null,
          external: [],
          external_status: {},
          finding_refs: [],
          events: [],
        };
      }
      throw new Error(`unexpected: ${cmd}`);
    });
    await act(async () => {
      await store.prime();
    });

    render(
      <MemoryRouter initialEntries={["/risks"]}>
        <Risks />
      </MemoryRouter>,
    );

    expect(screen.getByText("Select a risk to inspect.")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Pickle RCE"));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(detailCalls).toBeGreaterThan(0);
  });
});
