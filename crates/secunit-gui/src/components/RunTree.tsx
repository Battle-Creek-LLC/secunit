import { useMemo, useState } from "react";
import { Badge, type BadgeVariant } from "@/components/ui";
import type { RunRow, RunState } from "@/lib/ipc";
import { cn } from "@/lib/cn";

const stateTone: Record<RunState, BadgeVariant> = {
  sealed: "ok",
  aborted: "error",
  pending: "warn",
};

interface RunTreeProps {
  runs: RunRow[];
  selected: { control_id: string; run_id: string } | null;
  onSelect: (row: RunRow) => void;
}

interface YearNode {
  year: number;
  quarters: QuarterNode[];
}
interface QuarterNode {
  quarter: number;
  controls: ControlNode[];
}
interface ControlNode {
  control_id: string;
  runs: RunRow[];
}

function group(runs: RunRow[]): YearNode[] {
  const byYear = new Map<number, Map<number, Map<string, RunRow[]>>>();
  for (const r of runs) {
    if (!byYear.has(r.year)) byYear.set(r.year, new Map());
    const byQ = byYear.get(r.year)!;
    if (!byQ.has(r.quarter)) byQ.set(r.quarter, new Map());
    const byC = byQ.get(r.quarter)!;
    if (!byC.has(r.control_id)) byC.set(r.control_id, []);
    byC.get(r.control_id)!.push(r);
  }
  return Array.from(byYear.entries())
    .sort(([a], [b]) => b - a)
    .map(([year, qs]) => ({
      year,
      quarters: Array.from(qs.entries())
        .sort(([a], [b]) => b - a)
        .map(([q, cs]) => ({
          quarter: q,
          controls: Array.from(cs.entries())
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([cid, rs]) => ({
              control_id: cid,
              runs: rs.sort((a, b) => b.run_id.localeCompare(a.run_id)),
            })),
        })),
    }));
}

export function RunTree({ runs, selected, onSelect }: RunTreeProps) {
  const tree = useMemo(() => group(runs), [runs]);
  const [openYears, setOpenYears] = useState<Set<number>>(
    () => new Set(tree.slice(0, 1).map((y) => y.year)),
  );
  const [openQuarters, setOpenQuarters] = useState<Set<string>>(() => {
    const s = new Set<string>();
    if (tree[0]) {
      const first = tree[0]!;
      first.quarters.slice(0, 1).forEach((q) => s.add(`${first.year}-q${q.quarter}`));
    }
    return s;
  });
  const [openControls, setOpenControls] = useState<Set<string>>(new Set());

  if (runs.length === 0) {
    return (
      <p className="px-4 py-6 text-center text-xs text-muted-foreground">
        No runs in evidence/ yet.
      </p>
    );
  }

  const toggleSet = <T,>(s: Set<T>, v: T): Set<T> => {
    const next = new Set(s);
    if (next.has(v)) next.delete(v);
    else next.add(v);
    return next;
  };

  return (
    <div className="overflow-auto p-2 text-sm">
      {tree.map((y) => {
        const yOpen = openYears.has(y.year);
        return (
          <div key={y.year}>
            <button
              type="button"
              onClick={() => setOpenYears((s) => toggleSet(s, y.year))}
              className="flex w-full items-center gap-1 px-1.5 py-1 text-left font-mono text-xs hover:bg-muted/50"
            >
              <Caret open={yOpen} />
              {y.year}
            </button>
            {yOpen &&
              y.quarters.map((q) => {
                const key = `${y.year}-q${q.quarter}`;
                const qOpen = openQuarters.has(key);
                return (
                  <div key={key} className="ml-3">
                    <button
                      type="button"
                      onClick={() => setOpenQuarters((s) => toggleSet(s, key))}
                      className="flex w-full items-center gap-1 px-1.5 py-1 text-left font-mono text-xs hover:bg-muted/50"
                    >
                      <Caret open={qOpen} />q{q.quarter}
                    </button>
                    {qOpen &&
                      q.controls.map((c) => {
                        const ckey = `${key}/${c.control_id}`;
                        const cOpen = openControls.has(ckey);
                        return (
                          <div key={ckey} className="ml-3">
                            <button
                              type="button"
                              onClick={() => setOpenControls((s) => toggleSet(s, ckey))}
                              className="flex w-full items-center gap-1 px-1.5 py-1 text-left font-mono text-xs hover:bg-muted/50"
                            >
                              <Caret open={cOpen} />
                              {c.control_id}
                            </button>
                            {cOpen &&
                              c.runs.map((r) => {
                                const isSel =
                                  selected !== null &&
                                  selected.control_id === r.control_id &&
                                  selected.run_id === r.run_id;
                                return (
                                  <button
                                    key={r.run_id}
                                    type="button"
                                    onClick={() => onSelect(r)}
                                    className={cn(
                                      "ml-5 flex w-full items-center justify-between rounded-sm px-1.5 py-1 text-left",
                                      isSel ? "bg-muted" : "hover:bg-muted/50",
                                    )}
                                  >
                                    <span className="font-mono text-xs">{r.run_id}</span>
                                    <Badge variant={stateTone[r.state]}>{r.state}</Badge>
                                  </button>
                                );
                              })}
                          </div>
                        );
                      })}
                  </div>
                );
              })}
          </div>
        );
      })}
    </div>
  );
}

function Caret({ open }: { open: boolean }) {
  return (
    <span className="inline-block w-3 select-none text-muted-foreground">
      {open ? "▾" : "▸"}
    </span>
  );
}
