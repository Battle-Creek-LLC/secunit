import { useEffect, useState } from "react";
import { readFindings, type FindingsRow } from "@/lib/ipc";
import { Badge } from "@/components/ui";
import { formatTimestamp, relTime } from "@/lib/time";

const stateTone = {
  sealed: "ok",
  aborted: "error",
  pending: "warn",
} as const;

interface FindingCardProps {
  row: FindingsRow;
  storeRevision: number;
}

export function FindingCard({ row, storeRevision }: FindingCardProps) {
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHtml(null);
    setError(null);
    readFindings(row.control_id, row.run_id)
      .then((r) => {
        if (!cancelled) setHtml(r.html);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [row.control_id, row.run_id, storeRevision]);

  return (
    <article className="rounded-lg border bg-background">
      <header className="flex items-start justify-between gap-3 border-b px-4 py-2">
        <div className="flex flex-col">
          <span className="font-mono text-xs text-muted-foreground">
            {row.control_id}
          </span>
          <span className="text-sm font-medium">{row.run_id}</span>
        </div>
        <div className="flex flex-col items-end gap-1 text-xs text-muted-foreground">
          <Badge variant={stateTone[row.run_state]}>{row.run_state}</Badge>
          <span title={formatTimestamp(row.completed_at)}>
            {relTime(row.completed_at)}
          </span>
        </div>
      </header>
      <div className="px-4 py-3">
        {error && (
          <p className="text-sm text-error" role="alert">
            {error}
          </p>
        )}
        {!error && html === null && (
          <p className="text-xs text-muted-foreground">loading…</p>
        )}
        {html !== null && (
          <div
            className="prose-findings text-sm"
            dangerouslySetInnerHTML={{ __html: html }}
          />
        )}
      </div>
      <footer className="flex items-center gap-3 border-t px-4 py-2 text-[10px] text-muted-foreground">
        <span className="font-mono">{row.path}</span>
      </footer>
    </article>
  );
}
