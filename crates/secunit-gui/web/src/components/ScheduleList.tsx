import { Badge, type BadgeVariant } from "@/components/ui";
import type { ScheduleEntryView, ScheduleReason } from "@/lib/ipc";
import { daysFromNow } from "@/lib/time";

const reasonTone: Record<ScheduleReason, BadgeVariant> = {
  cadence: "neutral",
  "override-due": "info",
  "override-insert": "info",
  "override-weekday": "info",
  "override-skip": "warn",
};

const reasonLabel: Record<ScheduleReason, string> = {
  cadence: "cadence",
  "override-due": "pinned · due",
  "override-insert": "pinned · insert",
  "override-weekday": "pinned · weekday",
  "override-skip": "pinned · skip",
};

export function ScheduleList({ entries }: { entries: ScheduleEntryView[] }) {
  if (entries.length === 0) {
    return (
      <p className="px-4 py-6 text-center text-xs text-muted-foreground">
        No firings in the window.
      </p>
    );
  }
  return (
    <ul className="divide-y">
      {entries.map((e) => (
        <li
          key={`${e.control_id}/${e.date}`}
          className="grid grid-cols-[auto_1fr_auto_auto] items-center gap-3 px-4 py-2 text-sm"
        >
          <span className="font-mono text-xs text-muted-foreground">{e.date}</span>
          <span className="font-mono text-xs">{e.control_id}</span>
          <span className="text-xs text-muted-foreground">
            {(() => {
              const d = daysFromNow(e.date);
              if (d == null) return "";
              if (d === 0) return "today";
              return d > 0 ? `in ${d}d` : `${-d}d ago`;
            })()}
          </span>
          <Badge
            variant={e.overdue ? "error" : reasonTone[e.reason]}
            title={e.note ?? reasonLabel[e.reason]}
          >
            {e.overdue ? "overdue" : reasonLabel[e.reason]}
          </Badge>
        </li>
      ))}
    </ul>
  );
}
