import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { useStore } from "@/store";
import { ControlsTable } from "@/components/ControlsTable";
import { ControlDetailPane } from "@/components/ControlDetail";
import { Input, Label } from "@/components/ui";
import type { ControlStatus } from "@/lib/ipc";

const STATUSES: Array<{ key: string; label: string; match: (s: ControlStatus) => boolean }> = [
  { key: "all", label: "all", match: () => true },
  { key: "overdue", label: "overdue", match: (s) => s === "overdue" },
  { key: "due-soon", label: "due soon", match: (s) => s === "due-soon" },
  { key: "in-progress", label: "in progress", match: (s) => s === "in-progress" },
  { key: "sealed", label: "sealed", match: (s) => s === "sealed" },
  { key: "never-run", label: "never run", match: (s) => s === "never-run" },
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
  const status = params.get("status") ?? "all";
  const cadence = params.get("cadence") ?? "all";
  const query = params.get("q") ?? "";
  const selected = params.get("id");
  const sort = (params.get("sort") as SortKey) ?? "next_due";
  const sortAsc = params.get("dir") !== "desc";

  const filtered = useMemo(() => {
    const all = Array.from(snapshot.controls.values());
    const statusFilter = STATUSES.find((s) => s.key === status) ?? STATUSES[0]!;
    const q = query.trim().toLowerCase();
    let rows = all.filter(
      (c) =>
        statusFilter.match(c.status) &&
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
  }, [snapshot.controls, status, cadence, query, sort, sortAsc]);

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
            <select
              id="status"
              value={status}
              onChange={(e) => updateParam("status", e.target.value)}
              className="h-8 rounded-md border bg-background px-2 text-sm"
            >
              {STATUSES.map((s) => (
                <option key={s.key} value={s.key}>
                  {s.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <Label htmlFor="cadence">cadence</Label>
            <select
              id="cadence"
              value={cadence}
              onChange={(e) => updateParam("cadence", e.target.value)}
              className="h-8 rounded-md border bg-background px-2 text-sm"
            >
              {CADENCES.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
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

function compare(
  a: { id: string; title: string; cadence: string; owner: string; next_due: string | null; status: string },
  b: { id: string; title: string; cadence: string; owner: string; next_due: string | null; status: string },
  key: SortKey,
): number {
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
      return a.status.localeCompare(b.status);
    case "next_due": {
      // nulls at the end on asc, at the start on desc — mirror cli's `secunit due`.
      if (a.next_due == null && b.next_due == null) return a.id.localeCompare(b.id);
      if (a.next_due == null) return 1;
      if (b.next_due == null) return -1;
      return a.next_due.localeCompare(b.next_due);
    }
  }
}
