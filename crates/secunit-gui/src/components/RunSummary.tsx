import { Badge, Card, CardContent, CardHeader, CardTitle } from "@/components/ui";
import type { RunDetail, RunTreeNode } from "@/lib/ipc";
import { formatTimestamp } from "@/lib/time";
import { cn } from "@/lib/cn";

interface RunSummaryProps {
  detail: RunDetail;
  selectedPath: string | null;
  onSelectArtifact: (path: string) => void;
}

export function RunSummary({ detail, selectedPath, onSelectArtifact }: RunSummaryProps) {
  const r = detail.row;
  const stateTone = r.state === "sealed" ? "ok" : "warn";

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <CardTitle>{r.run_id}</CardTitle>
            <Badge variant={stateTone}>{r.state}</Badge>
          </div>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5 text-sm">
            <Row label="control" value={r.control_id} mono />
            <Row label="started" value={formatTimestamp(r.started_at)} />
            <Row label="completed" value={formatTimestamp(r.completed_at)} />
            <Row label="manifest" value={r.manifest_sha256 ?? "—"} mono trunc />
            <Row label="quarter" value={`${r.year}-q${r.quarter}`} />
            <Row label="dir" value={r.run_dir} mono trunc />
          </dl>
        </CardContent>
      </Card>
      <Card className="flex-1">
        <CardHeader>
          <CardTitle>Files</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Tree
            nodes={detail.tree}
            depth={0}
            selectedPath={selectedPath}
            onSelectArtifact={onSelectArtifact}
          />
        </CardContent>
      </Card>
    </div>
  );
}

function Tree({
  nodes,
  depth,
  selectedPath,
  onSelectArtifact,
}: {
  nodes: RunTreeNode[];
  depth: number;
  selectedPath: string | null;
  onSelectArtifact: (path: string) => void;
}) {
  return (
    <ul className={depth === 0 ? "" : "ml-3"}>
      {nodes.map((n) => {
        if (n.kind === "dir") {
          return (
            <li key={n.path}>
              <div className="flex items-center gap-1 px-3 py-1 text-xs text-muted-foreground">
                <span>📁</span>
                <span className="font-mono">{n.name}</span>
              </div>
              <Tree
                nodes={n.children}
                depth={depth + 1}
                selectedPath={selectedPath}
                onSelectArtifact={onSelectArtifact}
              />
            </li>
          );
        }
        const isSel = selectedPath === n.path;
        return (
          <li key={n.path}>
            <button
              type="button"
              onClick={() => onSelectArtifact(n.path)}
              className={cn(
                "flex w-full items-center justify-between gap-2 px-3 py-1 text-left text-xs",
                isSel ? "bg-muted" : "hover:bg-muted/50",
              )}
            >
              <span className="flex items-center gap-1 truncate">
                <span>📄</span>
                <span className="truncate font-mono">{n.name}</span>
              </span>
              <span className="font-mono text-[10px] text-muted-foreground">
                {n.size != null ? humanBytes(n.size) : ""}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function Row({
  label,
  value,
  mono,
  trunc,
}: {
  label: string;
  value: string;
  mono?: boolean;
  trunc?: boolean;
}) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd
        className={cn(mono && "font-mono text-xs", trunc && "truncate")}
        title={trunc ? value : undefined}
      >
        {value}
      </dd>
    </>
  );
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
