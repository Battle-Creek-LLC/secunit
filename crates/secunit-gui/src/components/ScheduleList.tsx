import { useMemo } from "react";
import { Link } from "react-router-dom";
import { Badge, type BadgeVariant } from "@/components/ui";
import type { ScheduleEntryView, ScheduleReason } from "@/lib/ipc";
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

export function ScheduleList({ entries }: { entries: ScheduleEntryView[] }) {
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
        const tone = urgencyTone(days, e.overdue);
        const override = e.reason !== "cadence" ? e.reason : null;
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
                  {relative && (
                    <span className={cn("ml-1.5 tabular-nums", tone)}>
                      · {relative}
                    </span>
                  )}
                </span>
              </div>
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

function urgencyTone(days: number | null, overdue: boolean): string {
  if (overdue) return "font-medium text-error";
  if (days === 0) return "font-medium text-warn";
  return "text-muted-foreground";
}
