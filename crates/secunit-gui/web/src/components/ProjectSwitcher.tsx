import { useId } from "react";
import { cn } from "@/lib/cn";
import type { ProjectsView } from "@/lib/ipc";

export interface ProjectSwitcherProps {
  view: ProjectsView;
  selected: string | null;
  onSelect: (name: string) => void;
  disabled?: boolean;
}

/**
 * A native `<select>` styled to match the rest of the chrome. Native
 * because keyboard handling, escape behaviour, and screen-reader output
 * all come for free — and because a project switcher is not the place
 * to invent a custom listbox.
 */
export function ProjectSwitcher({
  view,
  selected,
  onSelect,
  disabled,
}: ProjectSwitcherProps) {
  const id = useId();

  if (view.projects.length === 0) {
    return (
      <span className="text-xs text-muted-foreground">no projects configured</span>
    );
  }

  return (
    <label htmlFor={id} className="flex items-center gap-2">
      <span className="sr-only">Project</span>
      <select
        id={id}
        value={selected ?? ""}
        onChange={(e) => onSelect(e.target.value)}
        disabled={disabled}
        className={cn(
          "h-8 rounded-md border bg-background px-2 text-sm font-medium",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          "disabled:cursor-not-allowed disabled:opacity-50",
        )}
      >
        {view.projects.map((p) => (
          <option key={p.name} value={p.name}>
            {p.exists ? p.name : `${p.name} (missing)`}
          </option>
        ))}
      </select>
    </label>
  );
}
