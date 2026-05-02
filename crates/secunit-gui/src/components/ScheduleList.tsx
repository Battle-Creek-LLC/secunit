import { useMemo } from "react";
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

  const hasOverrides = useMemo(
    () => sorted.some((e) => e.reason !== "cadence"),
    [sorted],
  );

  if (sorted.length === 0) {
    return (
      <p className="px-4 py-6 text-center text-xs text-muted-foreground">
        No firings in the window.
      </p>
    );
  }

  return (
    <ul className="divide-y">
      {sorted.map((e) => {
        const days = daysFromNow(e.date);
        const relative = relativeLabel(days, e.overdue);
        const tone = urgencyTone(days, e.overdue);
        const override = e.reason !== "cadence" ? e.reason : null;

        return (
          <li
            key={`${e.control_id}/${e.date}`}
            className={cn(
              "grid items-center gap-3 px-4 py-2 text-sm",
              hasOverrides
                ? "grid-cols-[auto_1fr_auto_auto_auto]"
                : "grid-cols-[auto_1fr_auto_auto]",
              e.overdue && "border-l-2 border-error/70 bg-error/5",
            )}
          >
            <span className="font-mono text-xs text-muted-foreground">{e.date}</span>
            <span className="font-mono text-xs">{e.control_id}</span>
            <span className={cn("text-xs tabular-nums", tone)}>{relative}</span>
            <span className="text-[11px] text-muted-foreground">{e.cadence}</span>
            {hasOverrides && (
              <span className="min-w-[5.5rem] text-right">
                {override && (
                  <Badge
                    variant={overrideTone[override]}
                    title={e.note ?? overrideLabel[override]}
                  >
                    {overrideLabel[override]}
                  </Badge>
                )}
              </span>
            )}
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
