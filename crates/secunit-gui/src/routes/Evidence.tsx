import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { getRun, type RunDetail, type RunRow } from "@/lib/ipc";
import { RunTree } from "@/components/RunTree";
import { RunSummary } from "@/components/RunSummary";
import { ArtifactPreview } from "@/components/ArtifactPreview";

export function Evidence() {
  const snapshot = useStore();
  const [selectedRun, setSelectedRun] = useState<RunRow | null>(null);
  const [detail, setDetail] = useState<RunDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedArtifact, setSelectedArtifact] = useState<string | null>(null);

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
