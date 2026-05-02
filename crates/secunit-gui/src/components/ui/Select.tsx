import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { cn } from "@/lib/cn";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  id?: string;
  ariaLabel?: string;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}

export function Select({
  value,
  onChange,
  options,
  id,
  ariaLabel,
  placeholder = "Select…",
  disabled,
  className,
}: SelectProps) {
  const reactId = useId();
  const listboxId = `${reactId}-list`;
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);

  const selectedIndex = useMemo(
    () => Math.max(0, options.findIndex((o) => o.value === value)),
    [options, value],
  );
  const current = options.find((o) => o.value === value);

  useEffect(() => {
    if (open) setActive(selectedIndex);
  }, [open, selectedIndex]);

  const close = useCallback(() => {
    setOpen(false);
    triggerRef.current?.focus();
  }, []);

  const commit = useCallback(
    (idx: number) => {
      const o = options[idx];
      if (!o) return;
      if (o.value !== value) onChange(o.value);
      close();
    },
    [options, value, onChange, close],
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setActive((a) => Math.min(a + 1, options.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActive((a) => Math.max(a - 1, 0));
      } else if (e.key === "Home") {
        e.preventDefault();
        setActive(0);
      } else if (e.key === "End") {
        e.preventDefault();
        setActive(options.length - 1);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        commit(active);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, options.length, active, commit, close]);

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

  const activeId = `${listboxId}-opt-${active}`;
  const label = current?.label ?? placeholder;

  return (
    <div className={cn("relative inline-block", className)}>
      <button
        ref={triggerRef}
        id={id}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-activedescendant={open ? activeId : undefined}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => {
          if (!open && (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            setOpen(true);
          }
        }}
        className={cn(
          "inline-flex h-8 w-full items-center justify-between gap-2 rounded-md border bg-background px-2.5 text-sm",
          "hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          "disabled:cursor-not-allowed disabled:opacity-50",
        )}
      >
        <span className={cn("truncate", !current && "text-muted-foreground")}>{label}</span>
        <Chevron open={open} />
      </button>

      {open && (
        <div
          ref={popoverRef}
          className="absolute left-0 top-full z-30 mt-1 min-w-full overflow-hidden rounded-md border bg-background shadow-lg"
        >
          <ul
            id={listboxId}
            role="listbox"
            aria-label={ariaLabel}
            className="max-h-72 overflow-auto py-1"
          >
            {options.map((o, i) => {
              const isActive = i === active;
              const isSelected = o.value === value;
              return (
                <li
                  key={o.value}
                  id={`${listboxId}-opt-${i}`}
                  role="option"
                  aria-selected={isSelected}
                  onMouseEnter={() => setActive(i)}
                  onClick={() => commit(i)}
                  className={cn(
                    "flex cursor-pointer items-center gap-2 px-2.5 py-1.5 text-sm",
                    isActive && "bg-muted",
                  )}
                >
                  <Check visible={isSelected} />
                  <span className="truncate">{o.label}</span>
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
      className={cn("shrink-0 text-foreground", !visible && "invisible")}
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
