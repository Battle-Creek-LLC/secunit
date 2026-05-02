import { Link } from "react-router-dom";
import { Badge } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { RunRow, RunState } from "@/lib/ipc";
import { relTime, formatTimestamp } from "@/lib/time";

const stateTone: Record<RunState, "ok" | "error" | "warn" | "info"> = {
  sealed: "ok",
  pending: "warn",
};

interface RunTimelineProps {
  runs: RunRow[];
  emptyHint?: string;
}

export function RunTimeline({ runs, emptyHint }: RunTimelineProps) {
  if (runs.length === 0) {
    return (
      <div className="rounded-md border bg-background px-4 py-8 text-center text-sm text-muted-foreground">
        {emptyHint ?? "no runs yet"}
      </div>
    );
  }
  return (
    <ul className="divide-y rounded-md border bg-background">
      {runs.map((r) => {
        const evidenceTo = `/evidence?control=${encodeURIComponent(r.control_id)}&run=${encodeURIComponent(r.run_id)}`;
        const cid = encodeURIComponent(r.control_id);
        const controlTo = `/controls?q=${cid}&id=${cid}`;
        return (
          <li
            key={`${r.control_id}/${r.run_id}`}
            className="flex items-center gap-3 px-4 py-2.5 text-sm"
          >
            <Link
              to={evidenceTo}
              className={cn(
                "flex min-w-0 flex-1 flex-col -my-2.5 -ml-4 py-2.5 pl-4 pr-2",
                "hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              )}
            >
              <span className="truncate font-mono text-xs text-muted-foreground">
                {r.control_id}
              </span>
              <span className="truncate font-medium">{r.run_id}</span>
            </Link>
            <span
              className="font-mono text-xs text-muted-foreground"
              title={formatTimestamp(r.completed_at ?? r.started_at)}
            >
              {relTime(r.completed_at ?? r.started_at)}
            </span>
            <IconLink to={controlTo} label={`open control ${r.control_id}`}>
              <ControlIcon />
            </IconLink>
            <IconLink to={evidenceTo} label={`open evidence for ${r.run_id}`}>
              <EvidenceIcon />
            </IconLink>
            <Badge variant={stateTone[r.state]}>{r.state}</Badge>
          </li>
        );
      })}
    </ul>
  );
}

function IconLink({
  to,
  label,
  children,
}: {
  to: string;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <Link
      to={to}
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground",
        "hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
      )}
    >
      {children}
    </Link>
  );
}

function ControlIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="2.5" y="3" width="11" height="10" rx="1.5" />
      <path d="M5 6h6M5 8.5h6M5 11h4" />
    </svg>
  );
}

function EvidenceIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3.5 2.5h6L12.5 5.5V13a1 1 0 0 1-1 1h-8a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z" />
      <path d="M9.5 2.5V5.5h3" />
    </svg>
  );
}
