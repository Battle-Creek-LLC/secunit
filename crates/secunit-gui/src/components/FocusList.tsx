import { Link } from "react-router-dom";
import { Badge, type BadgeVariant } from "@/components/ui";
import { cn } from "@/lib/cn";
import type {
  ControlSummary,
  CurrentPeriodStatus,
  DueRowView,
  RunRow,
} from "@/lib/ipc";

const STALLED_DAYS = 3;

type FocusKind = "overdue" | "due" | "stalled";

interface FocusItem {
  key: string;
  controlId: string;
  cadence: string;
  kind: FocusKind;
  badge: { label: string; tone: BadgeVariant };
  detail: string;
  /** lower = more urgent */
  rank: number;
  to: string;
}

interface FocusListProps {
  controls: Map<string, ControlSummary>;
  due: Map<string, DueRowView>;
  periods: Map<string, CurrentPeriodStatus>;
  runs: RunRow[];
  now?: number;
  limit?: number;
}

export function FocusList({
  controls,
  due,
  periods,
  runs,
  now = Date.now(),
  limit = 8,
}: FocusListProps) {
  const items = buildFocusItems({ controls, due, periods, runs, now });
  const top = items.slice(0, limit);
  if (top.length === 0) {
    return (
      <div className="rounded-md border bg-background px-4 py-8 text-center text-sm text-muted-foreground">
        Nothing needs attention right now.
      </div>
    );
  }
  return (
    <ul className="divide-y rounded-md border bg-background">
      {top.map((item) => (
        <li key={item.key}>
          <Link
            to={item.to}
            className={cn(
              "flex items-center justify-between gap-3 px-4 py-2.5 text-sm",
              "hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            )}
          >
            <div className="flex min-w-0 items-center gap-3">
              <Badge variant={item.badge.tone}>{item.badge.label}</Badge>
              <div className="flex min-w-0 flex-col">
                <span className="truncate font-mono text-xs text-foreground">
                  {item.controlId}
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  {item.cadence} · {item.detail}
                </span>
              </div>
            </div>
            <span className="text-xs text-muted-foreground" aria-hidden="true">
              →
            </span>
          </Link>
        </li>
      ))}
    </ul>
  );
}

interface BuildArgs {
  controls: Map<string, ControlSummary>;
  due: Map<string, DueRowView>;
  periods: Map<string, CurrentPeriodStatus>;
  runs: RunRow[];
  now: number;
}

function buildFocusItems({
  controls,
  periods,
  runs,
  now,
}: BuildArgs): FocusItem[] {
  const items: FocusItem[] = [];
  const seen = new Set<string>();
  const runsByControl = groupRunsByControl(runs);

  // Period-driven Open / Overdue. A "gap" is a current period that ended
  // without a satisfying run; "open" is the current period still inside
  // its window. The legacy 7-day horizon is gone — period coverage IS the
  // signal, so a control completed this period stays quiet until the
  // next period rolls.
  periods.forEach((p) => {
    const c = controls.get(p.control_id);
    const inPeriod = findPendingRunInPeriod(runsByControl.get(p.control_id), p);
    if (p.status === "gap") {
      items.push({
        key: `overdue:${p.control_id}`,
        controlId: p.control_id,
        cadence: c?.cadence ?? p.cadence,
        kind: "overdue",
        badge: { label: "Overdue", tone: "error" },
        detail: composeDetail(p.period_end ?? "uncovered period", inPeriod),
        rank: -1000,
        to: `/controls?id=${encodeURIComponent(p.control_id)}`,
      });
      seen.add(p.control_id);
    } else if (p.status === "open") {
      items.push({
        key: `due:${p.control_id}`,
        controlId: p.control_id,
        cadence: c?.cadence ?? p.cadence,
        kind: "due",
        badge: { label: "Open", tone: "warn" },
        detail: composeDetail(p.period_end ?? "open", inPeriod),
        rank: 0,
        to: `/controls?id=${encodeURIComponent(p.control_id)}`,
      });
      seen.add(p.control_id);
    }
  });

  const dayMs = 24 * 60 * 60 * 1000;
  runs.forEach((r) => {
    if (r.state !== "pending") return;
    const startedAt = r.started_at ? Date.parse(r.started_at) : NaN;
    if (Number.isNaN(startedAt)) return;
    const ageDays = Math.floor((now - startedAt) / dayMs);
    if (ageDays < STALLED_DAYS) return;
    const c = controls.get(r.control_id);
    items.push({
      key: `stalled:${r.control_id}/${r.run_id}`,
      controlId: r.control_id,
      cadence: c?.cadence ?? "—",
      kind: "stalled",
      badge: { label: "Stalled", tone: "info" },
      detail: `prepared ${ageDays}d ago, not sealed`,
      rank: 1000 - ageDays,
      to: `/controls?id=${encodeURIComponent(r.control_id)}`,
    });
  });

  items.sort((a, b) => a.rank - b.rank);
  return items;
}

function groupRunsByControl(runs: RunRow[]): Map<string, RunRow[]> {
  const m = new Map<string, RunRow[]>();
  runs.forEach((r) => {
    const arr = m.get(r.control_id);
    if (arr) arr.push(r);
    else m.set(r.control_id, [r]);
  });
  return m;
}

// A pending run inside the period window is in-flight work the user
// already started — worth surfacing so they finish it. A sealed run in
// the same window would normally flip the period to "satisfied" and
// drop it from focus entirely, so we don't tag that case.
function findPendingRunInPeriod(
  rs: RunRow[] | undefined,
  p: CurrentPeriodStatus,
): RunRow | null {
  if (!rs || rs.length === 0) return null;
  const start = p.period_start ? Date.parse(p.period_start) : NaN;
  const end = p.period_end ? Date.parse(p.period_end) : NaN;
  for (const r of rs) {
    if (r.state !== "pending") continue;
    const t = r.started_at ? Date.parse(r.started_at) : NaN;
    if (Number.isNaN(t)) continue;
    if (!Number.isNaN(start) && t < start) continue;
    if (!Number.isNaN(end) && t > end + 24 * 60 * 60 * 1000) continue;
    return r;
  }
  return null;
}

function composeDetail(base: string, run: RunRow | null): string {
  return run ? `${base} · run prepared` : base;
}
