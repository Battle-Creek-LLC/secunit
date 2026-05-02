import { Link } from "react-router-dom";
import { Badge, type BadgeVariant } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { ControlSummary, DueRowView, RunRow } from "@/lib/ipc";
import { daysFromNow } from "@/lib/time";

const STALLED_DAYS = 3;
const DUE_HORIZON_DAYS = 7;

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
  runs: RunRow[];
  now?: number;
  limit?: number;
}

export function FocusList({
  controls,
  due,
  runs,
  now = Date.now(),
  limit = 8,
}: FocusListProps) {
  const items = buildFocusItems({ controls, due, runs, now });
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
  runs: RunRow[];
  now: number;
}

function buildFocusItems({ controls, due, runs, now }: BuildArgs): FocusItem[] {
  const items: FocusItem[] = [];
  const seen = new Set<string>();

  controls.forEach((c) => {
    if (!c.overdue) return;
    const days = daysFromNow(c.next_due, now);
    const overdueBy = days != null && days < 0 ? Math.abs(days) : null;
    items.push({
      key: `overdue:${c.id}`,
      controlId: c.id,
      cadence: c.cadence,
      kind: "overdue",
      badge: { label: "Overdue", tone: "error" },
      detail: overdueBy != null ? `overdue by ${overdueBy}d` : "past grace window",
      rank: overdueBy != null ? -1000 - overdueBy : -500,
      to: `/controls?id=${encodeURIComponent(c.id)}`,
    });
    seen.add(c.id);
  });

  due.forEach((d) => {
    if (d.overdue || !d.next_due) return;
    if (seen.has(d.control_id)) return;
    const days = daysFromNow(d.next_due, now);
    if (days == null || days < 0 || days > DUE_HORIZON_DAYS) return;
    items.push({
      key: `due:${d.control_id}`,
      controlId: d.control_id,
      cadence: d.cadence,
      kind: "due",
      badge: { label: days === 0 ? "Due today" : `Due in ${days}d`, tone: "warn" },
      detail: `next due ${d.next_due}`,
      rank: days,
      to: `/controls?id=${encodeURIComponent(d.control_id)}`,
    });
    seen.add(d.control_id);
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
