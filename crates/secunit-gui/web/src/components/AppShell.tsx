import { useState, type ReactNode } from "react";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/cn";
import { Kbd } from "@/components/ui";
import { ProjectSwitcher } from "@/components/ProjectSwitcher";
import type { ProjectsView } from "@/lib/ipc";

const NAV: Array<{ to: string; label: string }> = [
  { to: "/overview", label: "Overview" },
  { to: "/controls", label: "Controls" },
  { to: "/schedule", label: "Schedule" },
  { to: "/findings", label: "Findings" },
  { to: "/evidence", label: "Evidence" },
  { to: "/inventory", label: "Inventory" },
];

export interface AppShellProps {
  view: ProjectsView;
  selected: string | null;
  onSelect: (name: string) => void;
  appVersion: string;
  onCommandTrigger?: () => void;
  children: ReactNode;
}

export function AppShell({
  view,
  selected,
  onSelect,
  appVersion,
  onCommandTrigger,
  children,
}: AppShellProps) {
  const [palette] = useState(false); // wired in JOB-12
  return (
    <div className="flex h-full flex-col">
      <TopBar
        view={view}
        selected={selected}
        onSelect={onSelect}
        appVersion={appVersion}
        onCommandTrigger={onCommandTrigger ?? (() => undefined)}
        showHint={!palette}
      />
      <div className="flex flex-1 overflow-hidden">
        <SideNav />
        <main className="flex-1 overflow-auto">{children}</main>
      </div>
    </div>
  );
}

interface TopBarProps {
  view: ProjectsView;
  selected: string | null;
  onSelect: (name: string) => void;
  appVersion: string;
  onCommandTrigger: () => void;
  showHint: boolean;
}

function TopBar({
  view,
  selected,
  onSelect,
  appVersion,
  onCommandTrigger,
  showHint,
}: TopBarProps) {
  return (
    <header className="flex h-12 shrink-0 items-center justify-between gap-3 border-b px-3">
      <div className="flex items-center gap-3">
        <span className="text-sm font-semibold tracking-tight">secunit</span>
        <ProjectSwitcher view={view} selected={selected} onSelect={onSelect} />
      </div>
      <button
        type="button"
        onClick={onCommandTrigger}
        className={cn(
          "inline-flex h-8 w-72 items-center justify-between rounded-md border bg-muted/30 px-3 text-xs text-muted-foreground",
          "hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        )}
      >
        <span>Search controls, runs, findings…</span>
        {showHint && (
          <span className="flex items-center gap-1">
            <Kbd>⌘</Kbd>
            <Kbd>K</Kbd>
          </span>
        )}
      </button>
      <div className="flex items-center gap-2">
        <span className="font-mono text-xs text-muted-foreground">v{appVersion}</span>
      </div>
    </header>
  );
}

function SideNav() {
  return (
    <nav className="w-56 shrink-0 border-r p-2">
      <ul className="flex flex-col gap-0.5">
        {NAV.map((item) => (
          <li key={item.to}>
            <NavLink
              to={item.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center rounded-md px-2.5 py-1.5 text-sm",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  isActive
                    ? "bg-muted font-semibold text-foreground"
                    : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                )
              }
            >
              {({ isActive }) => (
                <span aria-current={isActive ? "page" : undefined}>
                  {item.label}
                </span>
              )}
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}
