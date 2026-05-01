import { useEffect, useState } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import {
  listProjects,
  loadProject,
  selectProject,
  type ProjectsView,
} from "@/lib/ipc";
import { AppShell } from "@/components/AppShell";
import { EmptyConfig } from "@/components/EmptyConfig";
import { ErrorCard } from "@/components/ErrorCard";
import { store } from "@/store";
import { wireWatcherEvents } from "@/store/wire";
import { Overview } from "@/routes/Overview";
import { Controls } from "@/routes/Controls";
import { Schedule } from "@/routes/Schedule";
import { Findings } from "@/routes/Findings";
import { Evidence } from "@/routes/Evidence";
import { Inventory } from "@/routes/Inventory";

declare const __APP_VERSION__: string;

type LoadState =
  | { status: "loading" }
  | {
      status: "ready";
      view: ProjectsView;
      selected: string | null;
      registryError: string | null;
    }
  | { status: "error"; message: string };

function pickInitial(view: ProjectsView): string | null {
  const known = new Set(view.projects.map((p) => p.name));
  if (view.last_selected && known.has(view.last_selected)) return view.last_selected;
  if (view.default && known.has(view.default)) return view.default;
  return view.projects[0]?.name ?? null;
}

async function openProject(name: string): Promise<string | null> {
  await selectProject(name);
  store.reset();
  try {
    await loadProject(name);
    await store.prime();
    return null;
  } catch (e) {
    return String(e);
  }
}

export function App() {
  const [load, setLoad] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    let detach: (() => void) | undefined;

    (async () => {
      try {
        detach = await wireWatcherEvents();
        const view = await listProjects();
        if (cancelled) return;
        const initial = pickInitial(view);
        let registryError: string | null = null;
        if (initial !== null) {
          registryError = await openProject(initial);
        }
        if (cancelled) return;
        setLoad({ status: "ready", view, selected: initial, registryError });
      } catch (e) {
        if (cancelled) return;
        setLoad({ status: "error", message: String(e) });
      }
    })();

    return () => {
      cancelled = true;
      detach?.();
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
    setLoad({ ...load, selected: name, registryError: null });
    const err = await openProject(name);
    setLoad({
      status: "ready",
      view: load.view,
      selected: name,
      registryError: err,
    });
  };

  return (
    <HashRouter>
      <AppShell
        view={load.view}
        selected={load.selected}
        onSelect={onSelect}
        appVersion={__APP_VERSION__}
      >
        {load.registryError && (
          <div className="p-4">
            <ErrorCard
              title="Failed to load project"
              message={load.registryError}
            />
          </div>
        )}
        <Routes>
          <Route path="/" element={<Navigate to="/overview" replace />} />
          <Route path="/overview" element={<Overview />} />
          <Route path="/controls" element={<Controls />} />
          <Route path="/schedule" element={<Schedule />} />
          <Route path="/findings" element={<Findings />} />
          <Route path="/evidence" element={<Evidence />} />
          <Route path="/inventory" element={<Inventory />} />
          <Route path="*" element={<Navigate to="/overview" replace />} />
        </Routes>
      </AppShell>
    </HashRouter>
  );
}
