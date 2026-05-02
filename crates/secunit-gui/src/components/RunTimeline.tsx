import { Badge } from "@/components/ui";
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
      <p className="px-4 py-6 text-center text-xs text-muted-foreground">
        {emptyHint ?? "no runs yet"}
      </p>
    );
  }
  return (
    <ul className="divide-y">
      {runs.map((r) => (
        <li
          key={`${r.control_id}/${r.run_id}`}
          className="flex items-center justify-between px-4 py-2 text-sm"
        >
          <div className="flex flex-col">
            <span className="font-mono text-xs text-muted-foreground">
              {r.control_id}
            </span>
            <span className="font-medium">{r.run_id}</span>
          </div>
          <div className="flex items-center gap-3">
            <span
              className="font-mono text-xs text-muted-foreground"
              title={formatTimestamp(r.completed_at ?? r.started_at)}
            >
              {relTime(r.completed_at ?? r.started_at)}
            </span>
            <Badge variant={stateTone[r.state]}>{r.state}</Badge>
          </div>
        </li>
      ))}
    </ul>
  );
}
