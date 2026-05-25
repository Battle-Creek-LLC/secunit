import { useEffect, useMemo, useRef } from "react";
import { Badge, Table, THead, TBody, TH, TR, TD } from "@/components/ui";
import type { RiskRow } from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { severityTone, statusTone, statusLabel, slaCountdown } from "@/lib/risks";

export type RiskSortKey =
  | "id"
  | "title"
  | "severity"
  | "status"
  | "owner"
  | "due_at"
  | "source_control";

export interface RisksTableProps {
  rows: RiskRow[];
  selected: string | null;
  onSelect: (id: string) => void;
  sort: RiskSortKey;
  sortAsc: boolean;
  onSort: (k: RiskSortKey) => void;
}

export function RisksTable({
  rows,
  selected,
  onSelect,
  sort,
  sortAsc,
  onSort,
}: RisksTableProps) {
  const tableRef = useRef<HTMLTableElement | null>(null);

  // Keyboard nav: ↑/↓ move selection across rows; Esc clears.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (rows.length === 0) return;
      if (e.target instanceof HTMLInputElement) return;
      const idx = selected ? rows.findIndex((r) => r.id === selected) : -1;
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

  const headerProps = (key: RiskSortKey) => ({
    role: "columnheader" as const,
    "aria-sort":
      sort === key
        ? sortAsc
          ? ("ascending" as const)
          : ("descending" as const)
        : ("none" as const),
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
          <TH {...headerProps("severity")}>severity</TH>
          <TH {...headerProps("status")}>status</TH>
          <TH {...headerProps("owner")}>owner</TH>
          <TH {...headerProps("due_at")}>SLA</TH>
          <TH {...headerProps("source_control")}>source</TH>
          <TH>tracker</TH>
        </tr>
      </THead>
      <TBody>
        {rows.map((r) => (
          <Row
            key={r.id}
            row={r}
            isSelected={r.id === selected}
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
  row: RiskRow;
  isSelected: boolean;
  onSelect: (id: string) => void;
}) {
  const sla = useMemo(
    () => slaCountdown(row.due_at, row.status),
    [row.due_at, row.status],
  );
  const tracker = row.external[0] ?? null;
  return (
    <TR
      data-state={isSelected ? "selected" : undefined}
      onClick={() => onSelect(row.id)}
      className={cn("cursor-pointer", isSelected && "bg-muted")}
    >
      <TD className="font-mono text-xs text-muted-foreground">{row.id}</TD>
      <TD className="font-medium">{row.title}</TD>
      <TD>
        <Badge variant={severityTone[row.severity]}>{row.severity}</Badge>
      </TD>
      <TD>
        <Badge variant={statusTone[row.status]}>{statusLabel[row.status]}</Badge>
      </TD>
      <TD className="text-xs">{row.owner ?? "—"}</TD>
      <TD title={row.due_at ?? "no SLA"}>
        {row.due_at ? (
          <span
            className={cn(
              "text-xs",
              sla.overdue && "font-medium text-error",
            )}
          >
            {row.due_at}
            <span className="ml-2 opacity-80">({sla.label})</span>
          </span>
        ) : (
          <span className="text-xs text-muted-foreground">—</span>
        )}
      </TD>
      <TD className="font-mono text-xs text-muted-foreground">
        {row.source_control}
      </TD>
      <TD className="text-xs">
        {tracker ? (
          <span className="text-info" title={tracker.url}>
            {tracker.system} {tracker.id}
            {row.external.length > 1 && (
              <span className="ml-1 text-muted-foreground">
                +{row.external.length - 1}
              </span>
            )}
          </span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TD>
    </TR>
  );
}
