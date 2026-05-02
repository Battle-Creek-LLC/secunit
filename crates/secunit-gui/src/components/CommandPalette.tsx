import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useNavigate } from "react-router-dom";
import { searchPalette, type SearchHit } from "@/lib/ipc";
import { Input, Kbd } from "@/components/ui";
import { cn } from "@/lib/cn";

const GROUP_ORDER = ["control", "run", "finding", "inventory", "artifact"] as const;
type GroupKey = (typeof GROUP_ORDER)[number];

const GROUP_LABEL: Record<GroupKey, string> = {
  control: "Controls",
  run: "Runs",
  finding: "Findings",
  inventory: "Inventory",
  artifact: "Artifacts",
};

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const navigate = useNavigate();
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Focus input when opened; reset state when closed.
  useEffect(() => {
    if (open) {
      setActive(0);
      setQuery("");
      setHits([]);
      setError(null);
      // Defer to the next tick so the input is in the DOM.
      queueMicrotask(() => inputRef.current?.focus());
    }
  }, [open]);

  // Debounced query → searchPalette.
  useEffect(() => {
    if (!open) return;
    if (query.trim() === "") {
      setHits([]);
      return;
    }
    let cancelled = false;
    const t = window.setTimeout(() => {
      searchPalette(query, 30)
        .then((rows) => {
          if (!cancelled) {
            setHits(rows);
            setError(null);
            setActive(0);
          }
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
    }, 80);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [query, open]);

  const grouped = useMemo(() => {
    const m = new Map<GroupKey, SearchHit[]>();
    for (const h of hits) {
      const k = (GROUP_ORDER.includes(h.kind as GroupKey)
        ? h.kind
        : "artifact") as GroupKey;
      const list = m.get(k) ?? [];
      list.push(h);
      m.set(k, list);
    }
    return GROUP_ORDER.map((k) => ({ key: k, items: m.get(k) ?? [] })).filter(
      (g) => g.items.length > 0,
    );
  }, [hits]);

  const flat = useMemo(() => grouped.flatMap((g) => g.items), [grouped]);

  const openHit = useCallback(
    (hit: SearchHit) => {
      switch (hit.kind) {
        case "control":
          navigate(`/controls?id=${encodeURIComponent(hit.id)}`);
          break;
        case "run":
          navigate("/evidence");
          break;
        case "finding":
          navigate("/findings");
          break;
        case "inventory":
          navigate("/inventory");
          break;
        default:
          navigate("/evidence");
      }
      onClose();
    },
    [navigate, onClose],
  );

  // Keyboard navigation across the flat order.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActive((a) => Math.min(a + 1, Math.max(0, flat.length - 1)));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActive((a) => Math.max(a - 1, 0));
      } else if (e.key === "Enter") {
        const hit = flat[active];
        if (hit) {
          e.preventDefault();
          openHit(hit);
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, flat, active, onClose, openHit]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-foreground/10 p-8 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl overflow-hidden rounded-lg border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b p-2">
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search controls, runs, findings…"
            className="h-9 border-0 bg-transparent text-base focus-visible:ring-0"
          />
        </div>
        <div className="max-h-[60vh] overflow-auto">
          {error && (
            <p className="px-4 py-3 text-sm text-error" role="alert">
              {error}
            </p>
          )}
          {!error && query.trim() === "" && (
            <p className="px-4 py-6 text-center text-xs text-muted-foreground">
              Start typing to search the index.
            </p>
          )}
          {!error && query.trim() !== "" && hits.length === 0 && (
            <p className="px-4 py-6 text-center text-xs text-muted-foreground">
              No matches.
            </p>
          )}
          {grouped.map((g) => {
            const startIdx = flat.indexOf(g.items[0]!);
            return (
              <section key={g.key}>
                <div className="bg-muted/40 px-3 py-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  {GROUP_LABEL[g.key]}
                </div>
                <ul>
                  {g.items.map((h, i) => {
                    const idx = startIdx + i;
                    const isActive = idx === active;
                    return (
                      <li key={`${h.kind}/${h.id}/${h.path}`}>
                        <button
                          type="button"
                          onMouseEnter={() => setActive(idx)}
                          onClick={() => openHit(h)}
                          className={cn(
                            "flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm",
                            isActive ? "bg-muted" : "hover:bg-muted/40",
                          )}
                        >
                          <div className="flex min-w-0 flex-col">
                            <span className="truncate font-medium">{h.title}</span>
                            <span className="truncate font-mono text-xs text-muted-foreground">
                              {h.id}
                            </span>
                          </div>
                          <span className="font-mono text-[10px] text-muted-foreground">
                            {h.score.toFixed(2)}
                          </span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            );
          })}
        </div>
        <footer className="flex items-center justify-between gap-3 border-t px-3 py-1.5 text-[10px] text-muted-foreground">
          <div className="flex items-center gap-2">
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd>
            <span>navigate</span>
            <Kbd>↵</Kbd>
            <span>open</span>
            <Kbd>Esc</Kbd>
            <span>close</span>
          </div>
        </footer>
      </div>
    </div>
  );
}
