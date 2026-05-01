import { useEffect, useState } from "react";
import { listProjects, selectProject, type ProjectsView } from "@/lib/ipc";
import { ProjectSwitcher } from "@/components/ProjectSwitcher";
import { EmptyConfig } from "@/components/EmptyConfig";
import { ErrorCard } from "@/components/ErrorCard";

declare const __APP_VERSION__: string;

type LoadState =
  | { status: "loading" }
  | { status: "ready"; view: ProjectsView; selected: string | null }
  | { status: "error"; message: string };

/**
 * Pick the project to preselect on mount. Order: persisted last_selected
 * → declared default → first in the list → none.
 */
function pickInitial(view: ProjectsView): string | null {
  const known = new Set(view.projects.map((p) => p.name));
  if (view.last_selected && known.has(view.last_selected)) {
    return view.last_selected;
  }
  if (view.default && known.has(view.default)) {
    return view.default;
  }
  return view.projects[0]?.name ?? null;
}

export function App() {
  const [load, setLoad] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const view = await listProjects();
        if (cancelled) return;
        const initial = pickInitial(view);
        if (initial !== null) {
          await selectProject(initial);
        }
        setLoad({ status: "ready", view, selected: initial });
      } catch (e) {
        if (cancelled) return;
        setLoad({ status: "error", message: String(e) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (load.status === "loading") {
    return (
      <main className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading…
      </main>
    );
  }
  if (load.status === "error") {
    return <ErrorCard title="Failed to read projects.yaml" message={load.message} />;
  }
  if (load.view.projects.length === 0) {
    return <EmptyConfig configPath={load.view.config_path} />;
  }

  const onSelect = async (name: string) => {
    await selectProject(name);
    setLoad({ ...load, selected: name });
  };

  const active = load.view.projects.find((p) => p.name === load.selected) ?? null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 items-center justify-between border-b px-4">
        <div className="flex items-center gap-3">
          <span className="text-sm font-semibold tracking-tight">secunit</span>
          <ProjectSwitcher
            view={load.view}
            selected={load.selected}
            onSelect={onSelect}
          />
        </div>
        <span className="font-mono text-xs text-muted-foreground">
          v{__APP_VERSION__}
        </span>
      </header>
      <main className="flex-1 overflow-auto p-8">
        <div className="mx-auto max-w-2xl rounded-lg border bg-background p-6">
          <h1 className="text-base font-semibold">Project</h1>
          <dl className="mt-4 grid grid-cols-[8rem_1fr] gap-2 text-sm">
            <dt className="text-muted-foreground">name</dt>
            <dd className="font-medium">{active?.name ?? "—"}</dd>
            <dt className="text-muted-foreground">path</dt>
            <dd className="font-mono text-xs">
              {active?.resolved_path ?? "—"}
            </dd>
            <dt className="text-muted-foreground">on disk</dt>
            <dd>{active?.exists ? "yes" : "missing"}</dd>
          </dl>
          <p className="mt-6 text-xs text-muted-foreground">
            Views land in JOB-05 onward. The switcher is wired to the Rust
            shell — try selecting a different project to see the Rust side
            log the change.
          </p>
        </div>
      </main>
    </div>
  );
}
