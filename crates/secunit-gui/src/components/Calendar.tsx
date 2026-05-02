import { useMemo } from "react";
import { cn } from "@/lib/cn";
import type { ScheduleEntryView } from "@/lib/ipc";

interface CalendarProps {
  entries: ScheduleEntryView[];
  today?: Date;
  monthsAhead?: number;
}

export function Calendar({
  entries,
  today: todayProp,
  monthsAhead = 5,
}: CalendarProps) {
  const today = todayProp ?? new Date();
  const months = useMemo(() => buildMonths(today, monthsAhead), [today, monthsAhead]);
  const byDate = useMemo(() => indexEntries(entries), [entries]);

  return (
    <div className="grid gap-6 p-4 md:grid-cols-2 xl:grid-cols-3">
      {months.map((m) => (
        <MonthGrid key={`${m.year}-${m.month}`} month={m} byDate={byDate} today={today} />
      ))}
    </div>
  );
}

interface Month {
  year: number;
  month: number; // 0..11
  label: string;
}

function buildMonths(today: Date, monthsAhead: number): Month[] {
  const out: Month[] = [];
  for (let i = 0; i <= monthsAhead; i += 1) {
    const d = new Date(today.getFullYear(), today.getMonth() + i, 1);
    out.push({
      year: d.getFullYear(),
      month: d.getMonth(),
      label: d.toLocaleString(undefined, { month: "long", year: "numeric" }),
    });
  }
  return out;
}

function indexEntries(entries: ScheduleEntryView[]): Map<string, ScheduleEntryView[]> {
  const m = new Map<string, ScheduleEntryView[]>();
  for (const e of entries) {
    const list = m.get(e.date) ?? [];
    list.push(e);
    m.set(e.date, list);
  }
  return m;
}

function MonthGrid({
  month,
  byDate,
  today,
}: {
  month: Month;
  byDate: Map<string, ScheduleEntryView[]>;
  today: Date;
}) {
  const first = new Date(month.year, month.month, 1);
  const lead = first.getDay(); // 0..6 Sun..Sat
  const daysInMonth = new Date(month.year, month.month + 1, 0).getDate();
  const cells: Array<Date | null> = [];
  for (let i = 0; i < lead; i += 1) cells.push(null);
  for (let d = 1; d <= daysInMonth; d += 1) cells.push(new Date(month.year, month.month, d));
  while (cells.length % 7 !== 0) cells.push(null);

  const todayIso = isoOf(today);
  return (
    <div className="rounded-lg border">
      <div className="border-b px-3 py-2 text-sm font-semibold">{month.label}</div>
      <div className="grid grid-cols-7 gap-px border-b bg-border text-center text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
        {["S", "M", "T", "W", "T", "F", "S"].map((d, i) => (
          <div key={i} className="bg-background py-1">
            {d}
          </div>
        ))}
      </div>
      <div className="grid grid-cols-7 gap-px bg-border">
        {cells.map((d, i) => {
          if (d === null) return <div key={i} className="bg-background" />;
          const iso = isoOf(d);
          const list = byDate.get(iso) ?? [];
          return (
            <div
              key={i}
              className={cn(
                "min-h-[3.5rem] bg-background p-1 text-[10px]",
                iso === todayIso && "ring-1 ring-inset ring-info",
              )}
            >
              <div className="text-muted-foreground">{d.getDate()}</div>
              <ul className="mt-1 space-y-0.5">
                {list.slice(0, 3).map((e) => (
                  <li
                    key={e.control_id}
                    className={cn(
                      "truncate rounded-sm border px-1 font-mono text-[9px]",
                      e.overdue
                        ? "bg-error/10 text-error border-error/30"
                        : e.reason === "cadence"
                          ? "bg-muted text-foreground"
                          : "bg-info/10 text-info border-info/30",
                    )}
                    title={`${e.control_id} (${e.reason})${e.note ? ` — ${e.note}` : ""}`}
                  >
                    {e.control_id}
                  </li>
                ))}
                {list.length > 3 && (
                  <li className="text-[9px] text-muted-foreground">
                    +{list.length - 3} more
                  </li>
                )}
              </ul>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function isoOf(d: Date): string {
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}
