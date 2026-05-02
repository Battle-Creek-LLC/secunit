import { Link } from "react-router-dom";
import { Badge, type BadgeVariant } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { ControlSummary, CurrentPeriodStatus, RunRow } from "@/lib/ipc";

const STALLED_DAYS = 3;
const DUE_HORIZON_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

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
  periods: Map<string, CurrentPeriodStatus>;
  runs: RunRow[];
  now?: number;
  limit?: number;
}

export function FocusList({
  controls,
  periods,
  runs,
  now = Date.now(),
  limit = 8,
}: FocusListProps) {
  const items = buildFocusItems({ controls, periods, runs, now });
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
              "flex items-center gap-3 px-4 py-2.5 text-sm",
              "hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            )}
          >
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate font-mono text-xs text-muted-foreground">
                {item.controlId}
              </span>
              <span className="truncate font-medium">
                {item.cadence} · {item.detail}
              </span>
            </div>
            <Badge variant={item.badge.tone}>{item.badge.label}</Badge>
          </Link>
        </li>
      ))}
    </ul>
  );
}

interface BuildArgs {
  controls: Map<string, ControlSummary>;
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
  const runsByControl = groupRunsByControl(runs);

  // "Focus now" is action-shaped, not coverage-shaped. We surface:
  //   * Overdue gaps (period ended uncovered) — always.
  //   * Open periods whose period_end is within DUE_HORIZON_DAYS — i.e.,
  //     work the operator should do this week. period_end is the actual
  //     lapse deadline (once today > period_end with no satisfier, the
  //     period flips to gap); next_due is the next scheduled fire date,
  //     which can be later. An annual control's period is "open" for
  //     ~360 days/year, so this filter is what keeps the list useful.
  //   * Stalled pending runs (>STALLED_DAYS old) — finish or abort.
  // An open period with an in-flight pending run gets a "· run prepared"
  // tag rather than a separate item, so the user sees one row per
  // control with the most actionable framing.
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
        to: focusTo(p.control_id),
      });
    } else if (p.status === "open") {
      const days = daysUntilIso(p.period_end, now);
      const withinHorizon = days != null && days <= DUE_HORIZON_DAYS;
      // Surface when period_end is in the horizon, OR when there's a
      // pending run in flight (operator already started — show it so
      // they finish even if the deadline is months out).
      if (!withinHorizon && !inPeriod) return;
      items.push({
        key: `due:${p.control_id}`,
        controlId: p.control_id,
        cadence: c?.cadence ?? p.cadence,
        kind: "due",
        badge: { label: "Open", tone: "warn" },
        detail: composeDetail(p.period_end ?? "open", inPeriod),
        rank: withinHorizon ? (days as number) : 100,
        to: focusTo(p.control_id),
      });
    }
  });

  runs.forEach((r) => {
    if (r.state !== "pending") return;
    const startedAt = r.started_at ? Date.parse(r.started_at) : NaN;
    if (Number.isNaN(startedAt)) return;
    const ageDays = Math.floor((now - startedAt) / DAY_MS);
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
      to: focusTo(r.control_id),
    });
  });

  items.sort((a, b) => a.rank - b.rank);
  return items;
}

// Filter the table down to the one control AND select it so the detail
// pane opens — the operator landed here from "Focus now" wanting to act
// on this row, not browse around it.
function focusTo(controlId: string): string {
  const id = encodeURIComponent(controlId);
  return `/controls?q=${id}&id=${id}`;
}

function daysUntilIso(date: string | null, now: number): number | null {
  if (!date) return null;
  const t = Date.parse(date);
  if (Number.isNaN(t)) return null;
  return Math.ceil((t - now) / DAY_MS);
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
