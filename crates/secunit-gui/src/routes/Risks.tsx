import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { useStore } from "@/store";
import { RisksTable, type RiskSortKey } from "@/components/RisksTable";
import { RiskDetailPane } from "@/components/RiskDetail";
import { Input, Label, Select } from "@/components/ui";
import type { RiskRow, RiskStatus } from "@/lib/ipc";
import { SEVERITY_ORDER, slaCountdown } from "@/lib/risks";

// Status filter mirrors the lifecycle machine plus a "past SLA" lens (the
// CLI's `--past-sla`). "open" groups everything still actionable.
const STATUSES: Array<{
  key: string;
  label: string;
  match: (r: RiskRow) => boolean;
}> = [
  { key: "all", label: "all", match: () => true },
  {
    key: "past-sla",
    label: "past SLA",
    match: (r) => slaCountdown(r.due_at, r.status).overdue,
  },
  {
    key: "open",
    label: "open / active",
    match: (r) =>
      r.status === "open" ||
      r.status === "in-progress" ||
      r.status === "reopened",
  },
  { key: "in-progress", label: "in progress", match: (r) => r.status === "in-progress" },
  { key: "remediated", label: "remediated", match: (r) => r.status === "remediated" },
  {
    key: "accepted-exception",
    label: "accepted (exception)",
    match: (r) => r.status === "accepted-exception",
  },
  {
    key: "false-positive",
    label: "false positive",
    match: (r) => r.status === "false-positive",
  },
];

const SEVERITIES = ["all", "critical", "high", "medium", "low", "info"];

export function Risks() {
  const [params, setParams] = useSearchParams();
  const snapshot = useStore();
  const statusKey = params.get("status") ?? "all";
  const severity = params.get("severity") ?? "all";
  const query = params.get("q") ?? "";
  const selected = params.get("id");
  const sort = (params.get("sort") as RiskSortKey) ?? "severity";
  const sortAsc = params.get("dir") !== "desc";

  const filtered = useMemo(() => {
    const all = Array.from(snapshot.risks.values());
    const statusFilter = STATUSES.find((s) => s.key === statusKey) ?? STATUSES[0]!;
    const q = query.trim().toLowerCase();
    let rows = all.filter(
      (r) =>
        statusFilter.match(r) &&
        (severity === "all" || r.severity === severity) &&
        (q === "" ||
          r.id.toLowerCase().includes(q) ||
          r.title.toLowerCase().includes(q) ||
          r.fingerprint.toLowerCase().includes(q) ||
          (r.owner ?? "").toLowerCase().includes(q) ||
          r.source_control.toLowerCase().includes(q)),
    );
    rows = rows.sort((a, b) => {
      const cmp = compare(a, b, sort);
      return sortAsc ? cmp : -cmp;
    });
    return rows;
  }, [snapshot.risks, statusKey, severity, query, sort, sortAsc]);

  const updateParam = (key: string, value: string | null) => {
    setParams((prev) => {
      const next = new URLSearchParams(prev);
      if (value == null || value === "") next.delete(key);
      else next.set(key, value);
      return next;
    });
  };

  const onSelect = (id: string) => updateParam("id", id || null);

  const onSort = (k: RiskSortKey) => {
    if (sort === k) {
      updateParam("dir", sortAsc ? "desc" : null);
    } else {
      updateParam("sort", k);
      updateParam("dir", null);
    }
  };

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <section className="flex flex-1 flex-col overflow-hidden">
        <header className="flex flex-wrap items-end gap-3 border-b p-3">
          <div className="flex-1">
            <Label htmlFor="risk-search">search</Label>
            <Input
              id="risk-search"
              placeholder="filter by id, title, owner, control…"
              value={query}
              onChange={(e) => updateParam("q", e.target.value)}
            />
          </div>
          <div>
            <Label htmlFor="risk-status">status</Label>
            <Select
              id="risk-status"
              className="w-44"
              value={statusKey}
              onChange={(v) => updateParam("status", v)}
              options={STATUSES.map((s) => ({ value: s.key, label: s.label }))}
            />
          </div>
          <div>
            <Label htmlFor="risk-severity">severity</Label>
            <Select
              id="risk-severity"
              className="w-32"
              value={severity}
              onChange={(v) => updateParam("severity", v)}
              options={SEVERITIES.map((s) => ({ value: s, label: s }))}
            />
          </div>
          <div className="text-xs text-muted-foreground">
            {filtered.length} of {snapshot.risks.size}
          </div>
        </header>
        <div className="flex-1 overflow-auto">
          {snapshot.risks.size === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">
              No risks in the register yet. Open one with{" "}
              <code className="font-mono">secunit risks open</code>.
            </p>
          ) : (
            <RisksTable
              rows={filtered}
              selected={selected}
              onSelect={onSelect}
              sort={sort}
              sortAsc={sortAsc}
              onSort={onSort}
            />
          )}
        </div>
      </section>
      <aside className="w-[30rem] shrink-0 border-l">
        <RiskDetailPane id={selected} storeRevision={snapshot.revision} />
      </aside>
    </div>
  );
}

function compare(a: RiskRow, b: RiskRow, key: RiskSortKey): number {
  switch (key) {
    case "id":
      return a.id.localeCompare(b.id);
    case "title":
      return a.title.localeCompare(b.title);
    case "severity": {
      const cmp = SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity];
      return cmp !== 0 ? cmp : a.id.localeCompare(b.id);
    }
    case "status":
      return statusRank(a.status) - statusRank(b.status);
    case "owner":
      return (a.owner ?? "").localeCompare(b.owner ?? "");
    case "source_control":
      return a.source_control.localeCompare(b.source_control);
    case "due_at": {
      // No due date sorts last on ascending.
      if (!a.due_at && !b.due_at) return a.id.localeCompare(b.id);
      if (!a.due_at) return 1;
      if (!b.due_at) return -1;
      return a.due_at.localeCompare(b.due_at);
    }
  }
}

// Actionable states first when sorting by status.
function statusRank(s: RiskStatus): number {
  const order: Record<RiskStatus, number> = {
    open: 0,
    "in-progress": 1,
    reopened: 2,
    "accepted-exception": 3,
    remediated: 4,
    "false-positive": 5,
  };
  return order[s];
}
