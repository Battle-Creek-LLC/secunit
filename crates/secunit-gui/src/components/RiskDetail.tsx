import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Separator,
} from "@/components/ui";
import {
  getRisk,
  type RiskDetail,
  type RiskEventView,
  type RiskFindingRefView,
} from "@/lib/ipc";
import {
  severityTone,
  statusTone,
  statusLabel,
  slaCountdown,
} from "@/lib/risks";
import { formatTimestamp, relTime } from "@/lib/time";
import { cn } from "@/lib/cn";

interface RiskDetailPaneProps {
  id: string | null;
  storeRevision: number;
}

export function RiskDetailPane({ id, storeRevision }: RiskDetailPaneProps) {
  const [detail, setDetail] = useState<RiskDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (id == null) {
      setDetail(null);
      setError(null);
      return;
    }
    setError(null);
    getRisk(id)
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
        Select a risk to inspect.
      </div>
    );
  }
  if (error) {
    return <div className="p-6 text-sm text-error">Failed to load: {error}</div>;
  }
  if (!detail) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        loading…
      </div>
    );
  }

  const sla = slaCountdown(detail.due_at, detail.status);

  return (
    <div className="flex h-full flex-col gap-4 overflow-auto p-4">
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-2">
            <CardTitle>{detail.title}</CardTitle>
            <div className="flex shrink-0 flex-col items-end gap-1">
              <Badge variant={statusTone[detail.status]}>
                {statusLabel[detail.status]}
              </Badge>
              <Badge variant={severityTone[detail.severity]}>
                {detail.severity}
              </Badge>
            </div>
          </div>
          <CardDescription className="font-mono text-xs">
            {detail.id}
            {detail.fingerprint && ` · ${detail.fingerprint}`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1.5 text-sm">
            <Row label="owner" value={detail.owner ?? "unassigned"} />
            <Row
              label="SLA due"
              value={detail.due_at ?? "—"}
              hint={detail.due_at ?? undefined}
              tone={sla.overdue ? "error" : undefined}
              suffix={detail.due_at ? `(${sla.label})` : undefined}
            />
            <Row label="impact" value={String(detail.impact)} />
            <Row label="likelihood" value={String(detail.likelihood)} />
            {detail.source_control && (
              <Row label="source control" value={detail.source_control} mono />
            )}
            {detail.first_run_id && (
              <Row label="first run" value={detail.first_run_id} mono />
            )}
            {detail.affected_systems.length > 0 && (
              <Row
                label="affected"
                value={detail.affected_systems.join(", ")}
              />
            )}
            {detail.resolved_at && (
              <Row
                label="resolved"
                value={relTime(detail.resolved_at)}
                hint={detail.resolved_at}
              />
            )}
            {detail.exception_expires_at && (
              <Row
                label="exception until"
                value={detail.exception_expires_at}
              />
            )}
          </dl>

          {detail.external.length > 0 && (
            <>
              <Separator className="my-3" />
              <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Tracker mirrors
              </h4>
              <ul className="mt-1 space-y-1 text-xs">
                {detail.external.map((e, i) => (
                  <li key={i} className="flex items-center gap-2">
                    <span className="font-medium">{e.system}</span>
                    <span className="font-mono">{e.id}</span>
                    {detail.external_status[e.system] && (
                      <Badge variant="neutral" title="Advisory only — never authoritative">
                        observed: {detail.external_status[e.system]}
                      </Badge>
                    )}
                    <span className="text-muted-foreground">{e.url}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Bound evidence</CardTitle>
          <CardDescription>
            {detail.finding_refs.length} finding ref
            {detail.finding_refs.length === 1 ? "" : "s"} · bound by content hash
          </CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {detail.finding_refs.length === 0 ? (
            <p className="px-4 pb-4 text-xs text-muted-foreground">
              No bound findings.
            </p>
          ) : (
            <ul className="divide-y">
              {detail.finding_refs.map((fr, i) => (
                <FindingRefRow key={i} fr={fr} index={i} />
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Timeline</CardTitle>
          <CardDescription>
            {detail.events.length} event
            {detail.events.length === 1 ? "" : "s"} · the audit narrative
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Timeline events={detail.events} />
        </CardContent>
      </Card>
    </div>
  );
}

function FindingRefRow({
  fr,
  index,
}: {
  fr: RiskFindingRefView;
  index: number;
}) {
  // Deep-link into the existing Findings view for the bound run, the way
  // other views reach evidence — a read-only in-app navigation.
  const to = `/findings?control=${encodeURIComponent(
    fr.control_id,
  )}&q=${encodeURIComponent(fr.run_id)}`;
  return (
    <li className="flex items-start justify-between gap-3 px-4 py-2 text-xs">
      <div className="flex flex-col gap-0.5">
        <span className="font-medium">
          {index === 0 ? "originating" : "re-observed"} · {fr.finding_id}
        </span>
        <Link to={to} className="font-mono text-info underline-offset-2 hover:underline">
          {fr.control_id} / {fr.run_id}
        </Link>
        {fr.body_path && (
          <span className="font-mono text-muted-foreground">{fr.body_path}</span>
        )}
        <span
          className="font-mono text-[10px] text-muted-foreground"
          title={fr.manifest_sha256}
        >
          manifest {fr.manifest_sha256.slice(0, 12)}…
        </span>
      </div>
      <VerifiedBadge verified={fr.verified} />
    </li>
  );
}

function VerifiedBadge({ verified }: { verified: boolean | null }) {
  if (verified === true) {
    return (
      <Badge variant="ok" title="Recomputed manifest sha matches the pinned hash">
        ✓ verified
      </Badge>
    );
  }
  if (verified === false) {
    return (
      <Badge variant="error" title="Recomputed manifest sha does NOT match">
        ✗ mismatch
      </Badge>
    );
  }
  return (
    <Badge variant="neutral" title="Manifest not found on disk to verify against">
      ? unverifiable
    </Badge>
  );
}

function Timeline({ events }: { events: RiskEventView[] }) {
  if (events.length === 0) {
    return <p className="text-xs text-muted-foreground">No events.</p>;
  }
  return (
    <ol className="relative space-y-4 border-l pl-4">
      {events.map((ev) => (
        <li key={ev.seq} className="relative">
          <span
            className="absolute -left-[1.3125rem] top-1 h-2 w-2 rounded-full bg-muted-foreground/50"
            aria-hidden="true"
          />
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
            <span className="text-sm font-medium">{eventLabel(ev.type)}</span>
            <span
              className="text-[11px] text-muted-foreground"
              title={formatTimestamp(ev.ts)}
            >
              {relTime(ev.ts)}
            </span>
            <span className="text-[11px] text-muted-foreground">
              · {ev.actor}
              {ev.agent && (
                <span title={`${ev.agent.model} / ${ev.agent.skill}`}>
                  {" "}(agent)
                </span>
              )}
            </span>
          </div>
          <EventData type={ev.type} data={ev.data} />
        </li>
      ))}
    </ol>
  );
}

const EVENT_LABELS: Record<string, string> = {
  opened: "Opened",
  "owner-assigned": "Owner assigned",
  "score-changed": "Score changed",
  "sla-set": "SLA set",
  "status-changed": "Status changed",
  "evidence-linked": "Evidence linked",
  "external-linked": "External linked",
  "external-status-observed": "External status observed",
  note: "Note",
  remediated: "Remediated",
  reopened: "Reopened",
  "exception-documented": "Exception documented",
};

function eventLabel(type: string): string {
  return EVENT_LABELS[type] ?? type;
}

/** Render the salient fields of an event's payload — type-specific, kept
 *  small and human; the full raw payload is in the JSON contract. */
function EventData({
  type,
  data,
}: {
  type: string;
  data: Record<string, unknown>;
}) {
  const pairs = salientPairs(type, data);
  if (pairs.length === 0) return null;
  return (
    <dl className="mt-1 grid grid-cols-[7rem_1fr] gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
      {pairs.map(([k, v]) => (
        <div key={k} className="contents">
          <dt>{k}</dt>
          <dd className="break-words text-foreground/80">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function salientPairs(
  type: string,
  data: Record<string, unknown>,
): Array<[string, string]> {
  const get = (k: string) => stringify(data[k]);
  switch (type) {
    case "opened":
      return [
        ["severity", get("severity")],
        ["impact", get("impact")],
        ["likelihood", get("likelihood")],
        ["due", get("due_at")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    case "owner-assigned":
      return [["owner", get("owner")]];
    case "score-changed":
      return [
        ["severity", get("severity")],
        ["impact", get("impact")],
        ["likelihood", get("likelihood")],
        ["reason", get("reason")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    case "sla-set":
      return [
        ["due", get("due_at")],
        ["basis", get("basis")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    case "status-changed":
      return [
        ["from → to", `${get("from")} → ${get("to")}`],
        ["reason", get("reason")],
      ].filter((p) => p[1] !== "" && p[1] !== " → ") as Array<[string, string]>;
    case "evidence-linked": {
      const ref = data["finding_ref"] as Record<string, unknown> | undefined;
      if (!ref) return [];
      return [["finding", `${ref["control_id"]} / ${ref["run_id"]} · ${ref["finding_id"]}`]];
    }
    case "external-linked":
      return [
        ["system", get("system")],
        ["id", get("external_id")],
        ["url", get("url")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    case "external-status-observed":
      return [
        ["system", get("system")],
        ["status", get("status")],
        ["observed", get("observed_at")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    case "note":
      return [["text", get("text")]];
    case "remediated":
      return [["note", get("note")]].filter((p) => p[1] !== "") as Array<
        [string, string]
      >;
    case "reopened":
      return [["reason", get("reason")]];
    case "exception-documented":
      return [
        ["rationale", get("rationale")],
        ["approved by", get("approved_by")],
        ["expires", get("expires_at")],
      ].filter((p) => p[1] !== "") as Array<[string, string]>;
    default:
      return [];
  }
}

function stringify(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "string" || typeof v === "number" || typeof v === "boolean") {
    return String(v);
  }
  return JSON.stringify(v);
}

function Row({
  label,
  value,
  mono,
  hint,
  tone,
  suffix,
}: {
  label: string;
  value: string;
  mono?: boolean;
  hint?: string;
  tone?: "error";
  suffix?: string;
}) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd
        className={cn(mono && "font-mono text-xs", tone === "error" && "text-error")}
        title={hint}
      >
        {value}
        {suffix && <span className="ml-2 opacity-80">{suffix}</span>}
      </dd>
    </>
  );
}
