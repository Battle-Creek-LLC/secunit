import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { cn } from "@/lib/cn";
import type { ProjectsView } from "@/lib/ipc";

export interface ProjectSwitcherProps {
  view: ProjectsView;
  selected: string | null;
  onSelect: (name: string) => void;
  disabled?: boolean;
}

export function ProjectSwitcher({
  view,
  selected,
  onSelect,
  disabled,
}: ProjectSwitcherProps) {
  const listboxId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);

  const projects = view.projects;
  const selectedIndex = useMemo(
    () => Math.max(0, projects.findIndex((p) => p.name === selected)),
    [projects, selected],
  );
  const current = projects[selectedIndex];

  useEffect(() => {
    if (open) setActive(selectedIndex);
  }, [open, selectedIndex]);

  const close = useCallback(() => {
    setOpen(false);
    triggerRef.current?.focus();
  }, []);

  const commit = useCallback(
    (idx: number) => {
      const p = projects[idx];
      if (!p) return;
      if (p.name !== selected) onSelect(p.name);
      close();
    },
    [projects, selected, onSelect, close],
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setActive((a) => Math.min(a + 1, projects.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActive((a) => Math.max(a - 1, 0));
      } else if (e.key === "Home") {
        e.preventDefault();
        setActive(0);
      } else if (e.key === "End") {
        e.preventDefault();
        setActive(projects.length - 1);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        commit(active);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, projects.length, active, commit, close]);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: MouseEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (popoverRef.current?.contains(t)) return;
      if (triggerRef.current?.contains(t)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onPointer);
    return () => document.removeEventListener("mousedown", onPointer);
  }, [open]);

  if (projects.length === 0) {
    return (
      <span className="text-xs text-muted-foreground">no projects configured</span>
    );
  }

  const triggerLabel = current
    ? current.exists
      ? current.name
      : `${current.name} (missing)`
    : "Select project";

  const activeId = `${listboxId}-opt-${active}`;

  return (
    <div className="relative">
      <button
        ref={triggerRef}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-activedescendant={open ? activeId : undefined}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => {
          if (!open && (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            setOpen(true);
          }
        }}
        className={cn(
          "inline-flex h-8 min-w-[12rem] items-center justify-between gap-2 rounded-md border bg-background px-2.5 text-sm font-medium",
          "hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          "disabled:cursor-not-allowed disabled:opacity-50",
          current && !current.exists && "text-error",
        )}
      >
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate">{triggerLabel}</span>
        </span>
        <Chevron open={open} />
      </button>

      {open && (
        <div
          ref={popoverRef}
          className="absolute left-0 top-full z-30 mt-1 w-[min(22rem,90vw)] overflow-hidden rounded-md border bg-background shadow-lg"
        >
          <ul
            id={listboxId}
            role="listbox"
            aria-label="Project"
            className="max-h-72 overflow-auto py-1"
          >
            {projects.map((p, i) => {
              const isActive = i === active;
              const isSelected = p.name === selected;
              return (
                <li
                  key={p.name}
                  id={`${listboxId}-opt-${i}`}
                  role="option"
                  aria-selected={isSelected}
                  onMouseEnter={() => setActive(i)}
                  onClick={() => commit(i)}
                  className={cn(
                    "flex cursor-pointer items-start gap-2 px-2.5 py-1.5 text-sm",
                    isActive && "bg-muted",
                  )}
                >
                  <Check visible={isSelected} />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <div className="flex items-center gap-2">
                      <span className="truncate font-medium">{p.name}</span>
                      {!p.exists && (
                        <span className="rounded bg-error/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-error">
                          missing
                        </span>
                      )}
                    </div>
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {p.path}
                    </span>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      aria-hidden
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="none"
      className={cn("shrink-0 text-muted-foreground transition-transform", open && "rotate-180")}
    >
      <path
        d="M3 4.5 L6 7.5 L9 4.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function Check({ visible }: { visible: boolean }) {
  return (
    <svg
      aria-hidden
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      className={cn("mt-0.5 shrink-0 text-foreground", !visible && "invisible")}
    >
      <path
        d="M3 7.5 L5.5 10 L11 4"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
