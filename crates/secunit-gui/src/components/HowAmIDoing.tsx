import { useMemo } from "react";
import { Card, CardContent } from "@/components/ui";
import { Sparkline, SegmentBar } from "@/components/charts";
import { cn } from "@/lib/cn";
import type { ControlSummary, RunRow } from "@/lib/ipc";

const DAY_MS = 24 * 60 * 60 * 1000;
const WEEK_MS = 7 * DAY_MS;
const STALLED_DAYS = 3;
const SPARK_WEEKS = 12;

interface HowAmIDoingProps {
  controls: Map<string, ControlSummary>;
  runs: RunRow[];
  now?: number;
}

export function HowAmIDoing({ controls, runs, now = Date.now() }: HowAmIDoingProps) {
  const coverage = useMemo(() => computeCoverage(controls), [controls]);
  const cadence = useMemo(() => computeCadence(runs, now), [runs, now]);
  const inflight = useMemo(() => computeInflight(runs, now), [runs, now]);

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <Card>
        <CardContent className="flex flex-col gap-3 p-4">
          <div className="flex items-baseline justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              On-track coverage
            </span>
            <span className="text-xs text-muted-foreground tabular-nums">
              {coverage.onTrack}/{coverage.total}
            </span>
          </div>
          <span
            className={cn(
              "text-3xl font-semibold tabular-nums",
              coverage.pct >= 90 ? "text-ok" : coverage.pct >= 70 ? "text-warn" : "text-error",
            )}
          >
            {coverage.total === 0 ? "—" : `${coverage.pct}%`}
          </span>
          <SegmentBar
            segments={[
              { value: coverage.onTrack, label: "on track", className: "bg-ok" },
              { value: coverage.dueSoon, label: "due soon", className: "bg-warn" },
              { value: coverage.overdue, label: "overdue", className: "bg-error" },
            ]}
          />
          <span className="text-xs text-muted-foreground">
            {coverage.overdue > 0
              ? `${coverage.overdue} overdue · ${coverage.dueSoon} due soon`
              : `${coverage.dueSoon} due soon`}
          </span>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="flex flex-col gap-3 p-4">
          <div className="flex items-baseline justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              Sealed this week
            </span>
            <span
              className={cn(
                "text-xs tabular-nums",
                cadence.delta > 0
                  ? "text-ok"
                  : cadence.delta < 0
                    ? "text-warn"
                    : "text-muted-foreground",
              )}
              title={`vs ${cadence.avg.toFixed(1)} avg over ${SPARK_WEEKS}w`}
            >
              {cadence.delta > 0 ? "+" : ""}
              {cadence.delta} vs avg
            </span>
          </div>
          <span className="text-3xl font-semibold tabular-nums">{cadence.thisWeek}</span>
          <Sparkline
            values={cadence.weekly}
            width={140}
            height={28}
            ariaLabel={`Weekly sealed runs over the last ${SPARK_WEEKS} weeks`}
          />
          <span className="text-xs text-muted-foreground">
            {SPARK_WEEKS}w avg {cadence.avg.toFixed(1)}/wk
          </span>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="flex flex-col gap-3 p-4">
          <div className="flex items-baseline justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              In flight
            </span>
            <span className="text-xs text-muted-foreground tabular-nums">
              {inflight.pending} pending
            </span>
          </div>
          <span
            className={cn(
              "text-3xl font-semibold tabular-nums",
              inflight.stalled > 0 ? "text-warn" : "text-foreground",
            )}
          >
            {inflight.stalled}
          </span>
          <SegmentBar
            segments={[
              { value: inflight.fresh, label: "fresh", className: "bg-info" },
              { value: inflight.stalled, label: "stalled", className: "bg-warn" },
            ]}
          />
          <span className="text-xs text-muted-foreground">
            stalled = prepared &gt; {STALLED_DAYS}d, not sealed
          </span>
        </CardContent>
      </Card>
    </div>
  );
}

function computeCoverage(controls: Map<string, ControlSummary>) {
  let overdue = 0;
  let dueSoon = 0;
  let onTrack = 0;
  controls.forEach((c) => {
    if (c.status === "overdue" || c.overdue) overdue += 1;
    else if (c.status === "due-soon") dueSoon += 1;
    else onTrack += 1;
  });
  const total = controls.size;
  const pct = total === 0 ? 0 : Math.round(((onTrack + dueSoon) / total) * 100);
  return { overdue, dueSoon, onTrack, total, pct };
}

function computeCadence(runs: RunRow[], now: number) {
  const weekly = new Array<number>(SPARK_WEEKS).fill(0);
  // Bucket index 0 = oldest (SPARK_WEEKS-1 weeks ago), last = current week.
  runs.forEach((r) => {
    if (r.state !== "sealed" || !r.completed_at) return;
    const t = Date.parse(r.completed_at);
    if (Number.isNaN(t)) return;
    const weeksAgo = Math.floor((now - t) / WEEK_MS);
    if (weeksAgo < 0 || weeksAgo >= SPARK_WEEKS) return;
    weekly[SPARK_WEEKS - 1 - weeksAgo] += 1;
  });
  const thisWeek = weekly[SPARK_WEEKS - 1] ?? 0;
  const sum = weekly.reduce((a, b) => a + b, 0);
  const avg = sum / SPARK_WEEKS;
  const delta = thisWeek - Math.round(avg);
  return { weekly, thisWeek, avg, delta };
}

function computeInflight(runs: RunRow[], now: number) {
  let pending = 0;
  let stalled = 0;
  runs.forEach((r) => {
    if (r.state !== "pending") return;
    pending += 1;
    const t = r.started_at ? Date.parse(r.started_at) : NaN;
    if (!Number.isNaN(t) && now - t > STALLED_DAYS * DAY_MS) stalled += 1;
  });
  return { pending, stalled, fresh: pending - stalled };
}
