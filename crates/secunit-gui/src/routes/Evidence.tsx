import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useStore } from "@/store";
import { getRun, type RunDetail, type RunRow } from "@/lib/ipc";
import { RunTree } from "@/components/RunTree";
import { RunSummary } from "@/components/RunSummary";
import { ArtifactPreview } from "@/components/ArtifactPreview";

export function Evidence() {
  const snapshot = useStore();
  const [params, setParams] = useSearchParams();
  const [selectedRun, setSelectedRun] = useState<RunRow | null>(null);
  const [detail, setDetail] = useState<RunDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedArtifact, setSelectedArtifact] = useState<string | null>(null);

  // Hydrate the selection from `?control=…&run=…` so other views can deep-
  // link into a specific run. We resolve the row out of the snapshot once
  // it's loaded; if the params don't match anything we just leave the pane
  // empty rather than synthesizing a stub.
  const paramControl = params.get("control");
  const paramRun = params.get("run");
  useEffect(() => {
    if (!paramControl || !paramRun) return;
    if (
      selectedRun &&
      selectedRun.control_id === paramControl &&
      selectedRun.run_id === paramRun
    )
      return;
    const match = snapshot.runs.find(
      (r) => r.control_id === paramControl && r.run_id === paramRun,
    );
    if (match) {
      setSelectedRun(match);
      setSelectedArtifact(null);
    }
  }, [paramControl, paramRun, snapshot.runs, selectedRun]);

  useEffect(() => {
    let cancelled = false;
    if (selectedRun === null) {
      setDetail(null);
      setError(null);
      return;
    }
    setError(null);
    getRun(selectedRun.control_id, selectedRun.run_id)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedRun, snapshot.revision]);

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <aside className="w-64 shrink-0 overflow-auto border-r">
        <RunTree
          runs={snapshot.runs}
          selected={
            selectedRun
              ? { control_id: selectedRun.control_id, run_id: selectedRun.run_id }
              : null
          }
          onSelect={(r) => {
            setSelectedRun(r);
            setSelectedArtifact(null);
            setParams(
              (prev) => {
                const next = new URLSearchParams(prev);
                next.set("control", r.control_id);
                next.set("run", r.run_id);
                return next;
              },
              { replace: true },
            );
          }}
        />
      </aside>
      <section className="w-[28rem] shrink-0 overflow-hidden border-r">
        {selectedRun === null && (
          <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
            Select a run from the tree.
          </div>
        )}
        {selectedRun !== null && error && (
          <div className="p-6 text-sm text-error">Failed: {error}</div>
        )}
        {selectedRun !== null && !error && !detail && (
          <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
            loading…
          </div>
        )}
        {selectedRun !== null && detail && (
          <RunSummary
            detail={detail}
            selectedPath={selectedArtifact}
            onSelectArtifact={(p) => setSelectedArtifact(p)}
          />
        )}
      </section>
      <section className="flex-1 overflow-hidden">
        <ArtifactPreview path={selectedArtifact} storeRevision={snapshot.revision} />
      </section>
    </div>
  );
}
