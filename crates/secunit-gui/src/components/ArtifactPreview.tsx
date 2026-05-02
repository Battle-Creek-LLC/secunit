import { useEffect, useState } from "react";
import { readArtifact, type ArtifactView } from "@/lib/ipc";
import { Badge } from "@/components/ui";

interface ArtifactPreviewProps {
  path: string | null;
  storeRevision: number;
}

export function ArtifactPreview({ path, storeRevision }: ArtifactPreviewProps) {
  const [view, setView] = useState<ArtifactView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (path == null) {
      setView(null);
      setError(null);
      return;
    }
    setError(null);
    setView(null);
    readArtifact(path)
      .then((v) => {
        if (!cancelled) setView(v);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [path, storeRevision]);

  if (path == null) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Select an artifact to preview.
      </div>
    );
  }
  if (error) {
    return <div className="p-6 text-sm text-error">Failed: {error}</div>;
  }
  if (!view) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        loading…
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2">
        <span className="truncate font-mono text-xs">{view.path}</span>
        <div className="flex items-center gap-2">
          <Badge variant="neutral">{view.kind}</Badge>
          <span className="font-mono text-xs text-muted-foreground">
            {humanBytes(view.bytes)}
          </span>
        </div>
      </header>
      <div className="flex-1 overflow-auto p-4">
        {view.kind === "too-large" && (
          <p className="text-xs text-muted-foreground">
            File exceeds the 2 MiB preview cap.
          </p>
        )}
        {view.kind === "binary" && (
          <p className="text-xs text-muted-foreground">
            Binary file — open in editor to inspect.
          </p>
        )}
        {view.kind === "image" && (
          <p className="text-xs text-muted-foreground">
            Image preview not wired in v1; open in editor for now.
          </p>
        )}
        {view.kind === "markdown" && view.html !== null && (
          <div
            className="prose-findings text-sm"
            dangerouslySetInnerHTML={{ __html: view.html }}
          />
        )}
        {(view.kind === "json" || view.kind === "yaml" || view.kind === "text") &&
          view.text !== null && (
            <pre className="whitespace-pre-wrap rounded-md border bg-muted/30 p-3 font-mono text-xs">
              {view.text}
            </pre>
          )}
      </div>
    </div>
  );
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
