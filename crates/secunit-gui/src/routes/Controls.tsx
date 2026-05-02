import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { useStore } from "@/store";
import { ControlsTable } from "@/components/ControlsTable";
import { ControlDetailPane } from "@/components/ControlDetail";
import { Input, Label, Select } from "@/components/ui";
import type { ControlSummary } from "@/lib/ipc";
import { status, urgency } from "@/lib/controlStatus";

// Filters intentionally mix the two axes — pure status (sealed, failed,
// in-progress, never-run) and urgency (overdue, due-soon) — so the user
// can ask either kind of question from one dropdown.
const STATUSES: Array<{
  key: string;
  label: string;
  match: (c: ControlSummary) => boolean;
}> = [
  { key: "all", label: "all", match: () => true },
  { key: "overdue", label: "overdue", match: (c) => urgency(c) === "overdue" },
  { key: "due-soon", label: "due soon", match: (c) => urgency(c) === "due-soon" },
  { key: "in-progress", label: "in progress", match: (c) => status(c) === "in-progress" },
  { key: "failed", label: "failed", match: (c) => status(c) === "failed" },
  { key: "sealed", label: "sealed", match: (c) => status(c) === "sealed" },
  { key: "never-run", label: "never run", match: (c) => status(c) === "never-run" },
];

const CADENCES = [
  "all",
  "weekly",
  "monthly",
  "quarterly",
  "semi-annual",
  "annual",
  "scheduled",
  "continuous",
];

type SortKey = "id" | "title" | "cadence" | "owner" | "next_due" | "status";

export function Controls() {
  const [params, setParams] = useSearchParams();
  const snapshot = useStore();
  const statusKey = params.get("status") ?? "all";
  const cadence = params.get("cadence") ?? "all";
  const query = params.get("q") ?? "";
  const selected = params.get("id");
  const sort = (params.get("sort") as SortKey) ?? "next_due";
  const sortAsc = params.get("dir") !== "desc";

  const filtered = useMemo(() => {
    const all = Array.from(snapshot.controls.values());
    const statusFilter = STATUSES.find((s) => s.key === statusKey) ?? STATUSES[0]!;
    const q = query.trim().toLowerCase();
    let rows = all.filter(
      (c) =>
        statusFilter.match(c) &&
        (cadence === "all" || c.cadence === cadence) &&
        (q === "" ||
          c.id.toLowerCase().includes(q) ||
          c.title.toLowerCase().includes(q) ||
          c.owner.toLowerCase().includes(q)),
    );
    rows = rows.sort((a, b) => {
      const cmp = compare(a, b, sort);
      return sortAsc ? cmp : -cmp;
    });
    return rows;
  }, [snapshot.controls, statusKey, cadence, query, sort, sortAsc]);

  const updateParam = (key: string, value: string | null) => {
    setParams((prev) => {
      const next = new URLSearchParams(prev);
      if (value == null || value === "") next.delete(key);
      else next.set(key, value);
      return next;
    });
  };

  const onSelect = (id: string) => updateParam("id", id || null);

  const onSort = (k: SortKey) => {
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
            <Label htmlFor="search">search</Label>
            <Input
              id="search"
              placeholder="filter by id, title, owner…"
              value={query}
              onChange={(e) => updateParam("q", e.target.value)}
            />
          </div>
          <div>
            <Label htmlFor="status">status</Label>
            <Select
              id="status"
              className="w-36"
              value={statusKey}
              onChange={(v) => updateParam("status", v)}
              options={STATUSES.map((s) => ({ value: s.key, label: s.label }))}
            />
          </div>
          <div>
            <Label htmlFor="cadence">cadence</Label>
            <Select
              id="cadence"
              className="w-36"
              value={cadence}
              onChange={(v) => updateParam("cadence", v)}
              options={CADENCES.map((c) => ({ value: c, label: c }))}
            />
          </div>
          <div className="text-xs text-muted-foreground">
            {filtered.length} of {snapshot.controls.size}
          </div>
        </header>
        <div className="flex-1 overflow-auto">
          <ControlsTable
            rows={filtered}
            selected={selected}
            onSelect={onSelect}
            sort={sort}
            sortAsc={sortAsc}
            onSort={onSort}
          />
        </div>
      </section>
      <aside className="w-[28rem] shrink-0 border-l">
        <ControlDetailPane id={selected} storeRevision={snapshot.revision} />
      </aside>
    </div>
  );
}

function compare(a: ControlSummary, b: ControlSummary, key: SortKey): number {
  switch (key) {
    case "id":
      return a.id.localeCompare(b.id);
    case "title":
      return a.title.localeCompare(b.title);
    case "cadence":
      return a.cadence.localeCompare(b.cadence);
    case "owner":
      return a.owner.localeCompare(b.owner);
    case "status":
      return status(a).localeCompare(status(b));
    case "next_due": {
      // nulls at the end on asc, at the start on desc — mirror cli's `secunit due`.
      if (a.next_due == null && b.next_due == null) return a.id.localeCompare(b.id);
      if (a.next_due == null) return 1;
      if (b.next_due == null) return -1;
      return a.next_due.localeCompare(b.next_due);
    }
  }
}
