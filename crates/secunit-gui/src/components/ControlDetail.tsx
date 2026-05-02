import { useEffect, useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Separator,
} from "@/components/ui";
import { getControl, type ControlDetail } from "@/lib/ipc";
import { StatusBadge } from "@/components/StatusBadge";
import { relTime, formatTimestamp } from "@/lib/time";

interface ControlDetailPaneProps {
  id: string | null;
  storeRevision: number;
}

export function ControlDetailPane({ id, storeRevision }: ControlDetailPaneProps) {
  const [detail, setDetail] = useState<ControlDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (id == null) {
      setDetail(null);
      setError(null);
      return;
    }
    setError(null);
    getControl(id)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [id, storeRevision]);

  if (id == null) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Select a control to inspect.
      </div>
    );
  }
  if (error) {
    return (
      <div className="p-6 text-sm text-error">
        Failed to load: {error}
      </div>
    );
  }
  if (!detail) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        loading…
      </div>
    );
  }

  const s = detail.summary;
  return (
    <div className="flex h-full flex-col gap-4 overflow-auto p-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <CardTitle>{s.title}</CardTitle>
            <StatusBadge status={s.status} />
          </div>
          <CardDescription className="font-mono text-xs">{s.id}</CardDescription>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5 text-sm">
            <Row label="cadence" value={s.cadence} />
            <Row label="owner" value={s.owner} />
            <Row label="next due" value={s.next_due ?? "—"} />
            <Row label="last run" value={s.last_run_at ? relTime(s.last_run_at) : "never"} hint={s.last_run_at ?? undefined} />
            <Row label="policy" value={detail.policy} mono />
            <Row label="skill" value={detail.skill} mono />
            {detail.nist.length > 0 && (
              <Row label="nist" value={detail.nist.join(", ")} mono />
            )}
          </dl>
          {detail.references.length > 0 && (
            <>
              <Separator className="my-3" />
              <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                References
              </h4>
              <ul className="mt-1 list-disc space-y-1 pl-5 text-xs">
                {detail.references.map((r, i) => (
                  <li key={i}>
                    {r.title}
                    {r.path && (
                      <span className="ml-2 font-mono text-muted-foreground">{r.path}</span>
                    )}
                    {r.url && (
                      <span className="ml-2 text-info underline-offset-2">{r.url}</span>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Scope (today)</CardTitle>
          <CardDescription>
            {detail.resolved_scope_today.length} system{detail.resolved_scope_today.length === 1 ? "" : "s"}
          </CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {detail.resolved_scope_today.length === 0 ? (
            <p className="px-4 pb-4 text-xs text-muted-foreground">
              No scope today (continuous controls or empty inventory).
            </p>
          ) : (
            <ul className="divide-y">
              {detail.resolved_scope_today.map((r) => (
                <li
                  key={`${r.kind}/${r.name}`}
                  className="flex items-center justify-between px-4 py-2 text-sm"
                >
                  <span className="font-mono text-xs">{r.name}</span>
                  <span className="text-xs text-muted-foreground">{r.kind}</span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Recent runs</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {detail.recent_runs.length === 0 ? (
            <p className="px-4 pb-4 text-xs text-muted-foreground">
              No runs yet.
            </p>
          ) : (
            <ul className="divide-y">
              {detail.recent_runs.map((r) => (
                <li
                  key={r.run_id}
                  className="flex items-center justify-between px-4 py-2 text-sm"
                >
                  <span className="font-mono text-xs">{r.run_id}</span>
                  <span className="text-xs text-muted-foreground" title={formatTimestamp(r.completed_at ?? r.started_at)}>
                    {relTime(r.completed_at ?? r.started_at)} · {r.state}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function Row({
  label,
  value,
  mono,
  hint,
}: {
  label: string;
  value: string;
  mono?: boolean;
  hint?: string;
}) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd
        className={mono ? "font-mono text-xs" : ""}
        title={hint}
      >
        {value}
      </dd>
    </>
  );
}
