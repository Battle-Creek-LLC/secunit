import { Link } from "react-router";
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
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  );
}

function EvidenceIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
    </svg>
  );
}
