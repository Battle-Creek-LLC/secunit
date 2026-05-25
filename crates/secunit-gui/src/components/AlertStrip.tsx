import { Link } from "react-router-dom";
import { cn } from "@/lib/cn";

interface AlertStripProps {
  overdue: number;
  dueSoon: number;
  stalled: number;
}

export function AlertStrip({ overdue, dueSoon, stalled }: AlertStripProps) {
  const items: Array<{
    key: string;
    count: number;
    label: string;
    to: string;
    tone: "error" | "warn" | "info";
  }> = [
    {
      key: "overdue",
      count: overdue,
      label: overdue === 1 ? "overdue" : "overdue",
      to: "/controls?status=overdue",
      tone: "error",
    },
    {
      key: "due",
      count: dueSoon,
      label: "due this week",
      to: "/schedule",
      tone: "warn",
    },
    {
      key: "stalled",
      count: stalled,
      label: stalled === 1 ? "stalled run" : "stalled runs",
      to: "/evidence",
      tone: "info",
    },
  ];
  const dot: Record<"error" | "warn" | "info", string> = {
    error: "bg-error",
    warn: "bg-warn",
    info: "bg-info",
  };
  const total = overdue + dueSoon + stalled;
  if (total === 0) {
    return (
      <div
        className="flex items-center gap-2 rounded-md border border-ok/30 bg-ok/5 px-3 py-2 text-sm"
        role="status"
      >
        <span className="h-2 w-2 rounded-full bg-ok" aria-hidden="true" />
        <span className="text-foreground">All clear — nothing overdue, due this week, or stalled.</span>
      </div>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md border bg-muted/20 px-3 py-2 text-sm">
      {items.map((it) => (
        <Link
          key={it.key}
          to={it.to}
          aria-label={`${it.count} ${it.label}`}
          className={cn(
            "group inline-flex items-center gap-2 rounded px-1 -mx-1",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            it.count === 0 && "text-muted-foreground",
          )}
        >
          <span
            className={cn("h-2 w-2 rounded-full", it.count > 0 ? dot[it.tone] : "bg-muted-foreground/40")}
            aria-hidden="true"
          />
          <span className="font-semibold tabular-nums">{it.count}</span>
          <span className="text-muted-foreground group-hover:text-foreground">{it.label}</span>
        </Link>
      ))}
    </div>
  );
}
