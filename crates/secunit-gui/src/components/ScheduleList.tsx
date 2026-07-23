import { useMemo } from "react";
import { Link } from "react-router";
import { Badge, type BadgeVariant } from "@/components/ui";
import type {
  CurrentPeriodStatus,
  PeriodStatus,
  ScheduleEntryView,
  ScheduleReason,
} from "@/lib/ipc";
import { daysFromNow } from "@/lib/time";
import { cn } from "@/lib/cn";

const overrideTone: Record<Exclude<ScheduleReason, "cadence">, BadgeVariant> = {
  "override-due": "info",
  "override-insert": "info",
  "override-weekday": "info",
  "override-skip": "warn",
};

const overrideLabel: Record<Exclude<ScheduleReason, "cadence">, string> = {
  "override-due": "pinned · due",
  "override-insert": "pinned · insert",
  "override-weekday": "pinned · weekday",
  "override-skip": "pinned · skip",
};

const statusBadge: Partial<Record<PeriodStatus, { label: string; tone: BadgeVariant }>> = {
  gap: { label: "Overdue", tone: "error" },
  open: { label: "Open", tone: "warn" },
  satisfied: { label: "Passed", tone: "ok" },
  failed: { label: "Failed", tone: "error" },
  skipped: { label: "Skipped", tone: "info" },
};

export function ScheduleList({
  entries,
  periods,
}: {
  entries: ScheduleEntryView[];
  periods?: Map<string, CurrentPeriodStatus>;
}) {
  const sorted = useMemo(() => {
    return [...entries].sort((a, b) => {
      if (a.overdue !== b.overdue) return a.overdue ? -1 : 1;
      return a.date.localeCompare(b.date);
    });
  }, [entries]);

  if (sorted.length === 0) {
    return (
      <div className="rounded-md border bg-background px-4 py-8 text-center text-sm text-muted-foreground">
        No firings in the window.
      </div>
    );
  }

  return (
    <ul className="divide-y rounded-md border bg-background">
      {sorted.map((e) => {
        const days = daysFromNow(e.date);
        const relative = relativeLabel(days, e.overdue);
        const override = e.reason !== "cadence" ? e.reason : null;
        const period = periods?.get(e.control_id);
        const status = period ? statusBadge[period.status] : undefined;
        const cid = encodeURIComponent(e.control_id);
        return (
          <li key={`${e.control_id}/${e.date}`}>
            <Link
              to={`/controls?q=${cid}&id=${cid}`}
              className={cn(
                "flex items-center gap-3 px-4 py-2.5 text-sm",
                "hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                e.overdue && "border-l-2 border-error/70 bg-error/5",
              )}
            >
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate font-mono text-xs text-muted-foreground">
                  {e.control_id}
                </span>
                <span className="truncate font-medium">
                  {e.cadence} · {e.date}
                </span>
              </div>
              {relative && (
                <span className="shrink-0 tabular-nums text-xs text-muted-foreground">
                  {relative}
                </span>
              )}
              {status && (
                <Badge
                  variant={status.tone}
                  title={periodTitle(period!)}
                >
                  {status.label}
                </Badge>
              )}
              {override && (
                <Badge
                  variant={overrideTone[override]}
                  title={e.note ?? overrideLabel[override]}
                >
                  {overrideLabel[override]}
                </Badge>
              )}
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

function relativeLabel(days: number | null, overdue: boolean): string {
  if (days == null) return "";
  if (days === 0) return "today";
  if (days > 0) return `in ${days}d`;
  const ago = -days;
  return overdue ? `${ago}d overdue` : `${ago}d ago`;
}

function periodTitle(p: CurrentPeriodStatus): string {
  const parts: string[] = [];
  if (p.period_start && p.period_end) {
    parts.push(`period ${p.period_start} → ${p.period_end}`);
  } else if (p.period_end) {
    parts.push(`period ends ${p.period_end}`);
  }
  if (p.satisfied_by_run_id) {
    parts.push(`satisfied by ${p.satisfied_by_run_id}`);
  }
  if (p.late) parts.push("late");
  return parts.join(" · ");
}
