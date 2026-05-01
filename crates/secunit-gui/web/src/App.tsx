import { useEffect, useState } from "react";
import {
  listProjects,
  loadProject,
  listControls,
  selectProject,
  type ControlSummary,
  type LoadSummary,
  type ProjectsView,
} from "@/lib/ipc";
import { ProjectSwitcher } from "@/components/ProjectSwitcher";
import { EmptyConfig } from "@/components/EmptyConfig";
import { ErrorCard } from "@/components/ErrorCard";

declare const __APP_VERSION__: string;

type LoadState =
  | { status: "loading" }
  | {
      status: "ready";
      view: ProjectsView;
      selected: string | null;
      summary: LoadSummary | null;
      controls: ControlSummary[];
      registryError: string | null;
    }
  | { status: "error"; message: string };

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

async function openProject(name: string) {
  await selectProject(name);
  const summary = await loadProject(name);
  const controls = await listControls();
  return { summary, controls };
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
        let summary: LoadSummary | null = null;
        let controls: ControlSummary[] = [];
        let registryError: string | null = null;
        if (initial !== null) {
          try {
            const r = await openProject(initial);
            summary = r.summary;
            controls = r.controls;
          } catch (e) {
            registryError = String(e);
          }
        }
        if (cancelled) return;
        setLoad({
          status: "ready",
          view,
          selected: initial,
          summary,
          controls,
          registryError,
        });
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
    return (
      <ErrorCard title="Failed to read projects.yaml" message={load.message} />
    );
  }
  if (load.view.projects.length === 0) {
    return <EmptyConfig configPath={load.view.config_path} />;
  }

  const onSelect = async (name: string) => {
    setLoad({ ...load, selected: name, summary: null, controls: [], registryError: null });
    try {
      const r = await openProject(name);
      setLoad({
        status: "ready",
        view: load.view,
        selected: name,
        summary: r.summary,
        controls: r.controls,
        registryError: null,
      });
    } catch (e) {
      setLoad({
        status: "ready",
        view: load.view,
        selected: name,
        summary: null,
        controls: [],
        registryError: String(e),
      });
    }
  };

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
        <div className="mx-auto max-w-3xl space-y-6">
          {load.registryError && (
            <ErrorCard title="Failed to load project" message={load.registryError} />
          )}
          {load.summary && (
            <section className="rounded-lg border p-6">
              <h1 className="text-base font-semibold">{load.summary.name}</h1>
              <p className="mt-1 font-mono text-xs text-muted-foreground">
                {load.summary.root}
              </p>
              <dl className="mt-4 grid grid-cols-[8rem_1fr] gap-2 text-sm">
                <dt className="text-muted-foreground">controls</dt>
                <dd className="font-medium">{load.summary.controls_count}</dd>
                <dt className="text-muted-foreground">inventory</dt>
                <dd className="font-medium">{load.summary.inventory_count}</dd>
                <dt className="text-muted-foreground">errors</dt>
                <dd className={load.summary.errors.length > 0 ? "text-error" : ""}>
                  {load.summary.errors.length}
                </dd>
                <dt className="text-muted-foreground">warnings</dt>
                <dd>{load.summary.warnings.length}</dd>
              </dl>
            </section>
          )}
          {load.controls.length > 0 && (
            <section className="rounded-lg border">
              <header className="border-b px-4 py-2 text-sm font-semibold">
                Controls preview
              </header>
              <ul className="divide-y">
                {load.controls.map((c) => (
                  <li
                    key={c.id}
                    className="flex items-center justify-between px-4 py-2 text-sm"
                  >
                    <div className="flex flex-col">
                      <span className="font-mono text-xs text-muted-foreground">
                        {c.id}
                      </span>
                      <span className="font-medium">{c.title}</span>
                    </div>
                    <StatusPill status={c.status} />
                  </li>
                ))}
              </ul>
            </section>
          )}
          <p className="text-center text-xs text-muted-foreground">
            Six views land in JOB-05 onward. This screen is a temporary smoke
            test of the IPC bridge.
          </p>
        </div>
      </main>
    </div>
  );
}

function StatusPill({ status }: { status: ControlSummary["status"] }) {
  const tone: Record<ControlSummary["status"], string> = {
    overdue: "bg-error/10 text-error border-error/20",
    "due-soon": "bg-warn/10 text-warn border-warn/30",
    "in-progress": "bg-info/10 text-info border-info/20",
    sealed: "bg-ok/10 text-ok border-ok/20",
    aborted: "bg-error/10 text-error border-error/20",
    "never-run": "bg-muted text-muted-foreground border-border",
    idle: "bg-muted text-muted-foreground border-border",
  };
  return (
    <span
      className={`rounded-md border px-2 py-0.5 text-xs font-medium ${tone[status]}`}
    >
      {status}
    </span>
  );
}
