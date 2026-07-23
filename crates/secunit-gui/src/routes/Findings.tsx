import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { useStore } from "@/store";
import { listFindings, type FindingsRow } from "@/lib/ipc";
import { FindingsFilters } from "@/components/FindingsFilters";
import { FindingCard } from "@/components/FindingCard";

interface Filters {
  control_id: string;
  quarter: string;
  query: string;
}

const PAGE = 20;

export function Findings() {
  const snapshot = useStore();
  // Seed filters from the URL so deep-links (e.g. from a risk's bound
  // evidence) land pre-filtered on the right control/run.
  const [params] = useSearchParams();
  const [rows, setRows] = useState<FindingsRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [filters, setFilters] = useState<Filters>(() => ({
    control_id: params.get("control") ?? "",
    quarter: params.get("quarter") ?? "",
    query: params.get("q") ?? "",
  }));
  const [visible, setVisible] = useState(PAGE);

  useEffect(() => {
    if (snapshot.revision === 0) return;
    let cancelled = false;
    listFindings(filters.control_id || null, filters.quarter || null)
      .then((rs) => {
        if (!cancelled) {
          setRows(rs);
          setError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [snapshot.revision, filters.control_id, filters.quarter]);

  const filtered = useMemo(() => {
    const q = filters.query.trim().toLowerCase();
    if (q === "") return rows;
    return rows.filter(
      (r) =>
        r.control_id.toLowerCase().includes(q) ||
        r.run_id.toLowerCase().includes(q) ||
        r.path.toLowerCase().includes(q),
    );
  }, [rows, filters.query]);

  const visibleRows = useMemo(() => filtered.slice(0, visible), [filtered, visible]);

  const controlOptions = useMemo(
    () => Array.from(snapshot.controls.keys()).sort(),
    [snapshot.controls],
  );
  const quarterOptions = useMemo(() => {
    const set = new Set<string>();
    rows.forEach((r) => set.add(`${r.year}-q${r.quarter}`));
    return Array.from(set).sort().reverse();
  }, [rows]);

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <FindingsFilters
        controlOptions={controlOptions}
        quarterOptions={quarterOptions}
        controlId={filters.control_id}
        quarter={filters.quarter}
        query={filters.query}
        onChange={(next) => setFilters((f) => ({ ...f, ...next }))}
      />
      <section className="flex-1 overflow-auto p-4">
        {error && (
          <p className="mb-3 text-sm text-error" role="alert">
            {error}
          </p>
        )}
        <p className="mb-3 text-xs text-muted-foreground">
          {filtered.length} finding{filtered.length === 1 ? "" : "s"}
          {filtered.length > visibleRows.length && ` · showing ${visibleRows.length}`}
        </p>
        <div className="flex flex-col gap-4">
          {visibleRows.map((r) => (
            <FindingCard
              key={`${r.control_id}/${r.run_id}/${r.path}`}
              row={r}
              storeRevision={snapshot.revision}
            />
          ))}
          {filtered.length > visibleRows.length && (
            <button
              type="button"
              onClick={() => setVisible((v) => v + PAGE)}
              className="mx-auto rounded-md border px-3 py-1 text-xs hover:bg-muted"
            >
              show {Math.min(PAGE, filtered.length - visibleRows.length)} more
            </button>
          )}
          {filtered.length === 0 && rows.length > 0 && (
            <p className="text-center text-xs text-muted-foreground">
              No findings match the current filters.
            </p>
          )}
          {rows.length === 0 && (
            <p className="text-center text-xs text-muted-foreground">
              No findings.md files in evidence/ yet.
            </p>
          )}
        </div>
      </section>
    </div>
  );
}
