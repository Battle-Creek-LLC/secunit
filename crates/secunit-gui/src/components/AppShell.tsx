import { useEffect, useState, type ComponentType, type ReactNode, type SVGProps } from "react";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/cn";
import { Kbd } from "@/components/ui";
import { ProjectSwitcher } from "@/components/ProjectSwitcher";
import type { ProjectsView } from "@/lib/ipc";

type IconProps = SVGProps<SVGSVGElement>;

function Icon({ children, ...props }: IconProps & { children: ReactNode }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

const OverviewIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </Icon>
);
const ScheduleIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="3" y="4" width="18" height="18" rx="2" />
    <path d="M16 2v4M8 2v4M3 10h18" />
  </Icon>
);
const ControlsIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z" />
    <path d="m9 12 2 2 4-4" />
  </Icon>
);
const FindingsIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
    <path d="M12 9v4M12 17h.01" />
  </Icon>
);
const EvidenceIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
  </Icon>
);
const InventoryIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m7.5 4.27 9 5.15" />
    <path d="M21 8 12 3 3 8l9 5 9-5Z" />
    <path d="M3 8v8l9 5 9-5V8" />
    <path d="M12 13v8" />
  </Icon>
);
const ChevronLeft = (p: IconProps) => (
  <Icon {...p}>
    <path d="m15 18-6-6 6-6" />
  </Icon>
);
const ChevronRight = (p: IconProps) => (
  <Icon {...p}>
    <path d="m9 18 6-6-6-6" />
  </Icon>
);

const NAV: Array<{ to: string; label: string; icon: ComponentType<IconProps> }> = [
  { to: "/overview", label: "Overview", icon: OverviewIcon },
  { to: "/schedule", label: "Schedule", icon: ScheduleIcon },
  { to: "/controls", label: "Controls", icon: ControlsIcon },
  { to: "/findings", label: "Findings", icon: FindingsIcon },
  { to: "/evidence", label: "Evidence", icon: EvidenceIcon },
  { to: "/inventory", label: "Inventory", icon: InventoryIcon },
];

const COLLAPSED_KEY = "secunit:sidenav:collapsed";

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
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem(COLLAPSED_KEY) === "1";
  });
  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
  }, [collapsed]);
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
        <SideNav collapsed={collapsed} onToggle={() => setCollapsed((c) => !c)} />
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

interface SideNavProps {
  collapsed: boolean;
  onToggle: () => void;
}

function SideNav({ collapsed, onToggle }: SideNavProps) {
  return (
    <nav
      aria-label="Primary"
      className={cn(
        "flex shrink-0 flex-col border-r p-2 transition-[width] duration-150",
        collapsed ? "w-12" : "w-56",
      )}
    >
      <ul className="flex flex-1 flex-col gap-0.5">
        {NAV.map((item) => {
          const IconCmp = item.icon;
          return (
            <li key={item.to}>
              <NavLink
                to={item.to}
                aria-label={item.label}
                title={collapsed ? item.label : undefined}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-2.5 rounded-md py-1.5 text-sm",
                    collapsed ? "justify-center px-0" : "px-2.5",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    isActive
                      ? "bg-muted font-semibold text-foreground"
                      : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                  )
                }
              >
                {({ isActive }) => (
                  <>
                    <IconCmp className="h-4 w-4 shrink-0" />
                    <span
                      aria-current={isActive ? "page" : undefined}
                      className={collapsed ? "sr-only" : undefined}
                    >
                      {item.label}
                    </span>
                  </>
                )}
              </NavLink>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        onClick={onToggle}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-expanded={!collapsed}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        className={cn(
          "mt-1 flex h-8 items-center rounded-md text-muted-foreground hover:bg-muted/50 hover:text-foreground",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          collapsed ? "justify-center" : "justify-end px-2.5",
        )}
      >
        {collapsed ? (
          <ChevronRight className="h-4 w-4" />
        ) : (
          <ChevronLeft className="h-4 w-4" />
        )}
      </button>
    </nav>
  );
}
