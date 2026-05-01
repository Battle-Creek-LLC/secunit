import { useEffect, useMemo, useRef } from "react";
import { Table, THead, TBody, TH, TR, TD } from "@/components/ui";
import { StatusBadge } from "@/components/StatusBadge";
import type { ControlSummary } from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { relTime, daysFromNow } from "@/lib/time";

type SortKey = "id" | "title" | "cadence" | "owner" | "next_due" | "status";

export interface ControlsTableProps {
  rows: ControlSummary[];
  selected: string | null;
  onSelect: (id: string) => void;
  sort: SortKey;
  sortAsc: boolean;
  onSort: (k: SortKey) => void;
}

export function ControlsTable({
  rows,
  selected,
  onSelect,
  sort,
  sortAsc,
  onSort,
}: ControlsTableProps) {
  const tableRef = useRef<HTMLTableElement | null>(null);

  // Keyboard nav: ↑/↓ move selection across rows; ↵ confirms.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (rows.length === 0) return;
      if (e.target instanceof HTMLInputElement) return;
      const idx = selected
        ? rows.findIndex((r) => r.id === selected)
        : -1;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        const next = rows[Math.min(idx + 1, rows.length - 1)];
        if (next) onSelect(next.id);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        const prev = rows[Math.max(idx - 1, 0)];
        if (prev) onSelect(prev.id);
      } else if (e.key === "Escape" && selected) {
        e.preventDefault();
        onSelect("");
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [rows, selected, onSelect]);

  const headerProps = (key: SortKey) => ({
    role: "columnheader" as const,
    "aria-sort":
      sort === key ? (sortAsc ? ("ascending" as const) : ("descending" as const)) : ("none" as const),
    onClick: () => onSort(key),
    className: "cursor-pointer select-none",
    title: "Click to sort",
  });

  return (
    <Table ref={tableRef}>
      <THead>
        <tr>
          <TH {...headerProps("id")}>id</TH>
          <TH {...headerProps("title")}>title</TH>
          <TH {...headerProps("cadence")}>cadence</TH>
          <TH {...headerProps("owner")}>owner</TH>
          <TH {...headerProps("next_due")}>next due</TH>
          <TH {...headerProps("status")}>status</TH>
          <TH>last run</TH>
        </tr>
      </THead>
      <TBody>
        {rows.map((c) => (
          <Row
            key={c.id}
            row={c}
            isSelected={c.id === selected}
            onSelect={onSelect}
          />
        ))}
      </TBody>
    </Table>
  );
}

function Row({
  row,
  isSelected,
  onSelect,
}: {
  row: ControlSummary;
  isSelected: boolean;
  onSelect: (id: string) => void;
}) {
  const dueDelta = useMemo(() => daysFromNow(row.next_due), [row.next_due]);
  return (
    <TR
      data-state={isSelected ? "selected" : undefined}
      onClick={() => onSelect(row.id)}
      className={cn(
        "cursor-pointer",
        isSelected && "bg-muted",
      )}
    >
      <TD className="font-mono text-xs text-muted-foreground">{row.id}</TD>
      <TD className="font-medium">{row.title}</TD>
      <TD className="text-xs">{row.cadence}</TD>
      <TD className="text-xs">{row.owner}</TD>
      <TD title={row.next_due ?? "—"}>
        {row.next_due ? (
          <span className="text-xs">
            {row.next_due}
            {dueDelta !== null && (
              <span className="ml-2 text-muted-foreground">
                ({dueDelta >= 0 ? `in ${dueDelta}d` : `${-dueDelta}d ago`})
              </span>
            )}
          </span>
        ) : (
          <span className="text-xs text-muted-foreground">—</span>
        )}
      </TD>
      <TD>
        <StatusBadge status={row.status} />
      </TD>
      <TD className="text-xs text-muted-foreground" title={row.last_run_at ?? "never"}>
        {row.last_run_at ? relTime(row.last_run_at) : "never"}
      </TD>
    </TR>
  );
}
