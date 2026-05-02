import { useMemo, useState } from "react";
import {
  Badge,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
  TBody,
  TD,
  TH,
  THead,
  Table,
  TR,
} from "@/components/ui";
import { useStore } from "@/store";
import type { InventoryEntryView } from "@/lib/ipc";

export function Inventory() {
  const snapshot = useStore();
  const [query, setQuery] = useState("");

  const filteredKinds = useMemo(() => {
    if (snapshot.inventory == null) return [];
    const q = query.trim().toLowerCase();
    return snapshot.inventory.kinds
      .map((k) => ({
        kind: k.kind,
        entries: k.entries.filter((e) => matches(e, q)),
      }))
      .filter((k) => k.entries.length > 0 || q === "");
  }, [snapshot.inventory, query]);

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }
  if (snapshot.inventory == null || snapshot.inventory.kinds.length === 0) {
    return (
      <div className="p-8">
        <p className="mx-auto max-w-xl text-center text-sm text-muted-foreground">
          inventory.yaml is empty.
        </p>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="mx-auto flex max-w-5xl flex-col gap-4">
        <header className="flex items-end justify-between gap-4">
          <div>
            <h1 className="text-base font-semibold">Inventory</h1>
            <p className="text-xs text-muted-foreground">
              Read-only view of inventory.yaml. Edit via git or the CLI.
            </p>
          </div>
          <div className="w-72">
            <Label htmlFor="inv-q">search</Label>
            <Input
              id="inv-q"
              value={query}
              placeholder="name, tag, or extras value…"
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </header>
        {filteredKinds.map((k) => (
          <Card key={k.kind}>
            <CardHeader>
              <CardTitle>
                <span className="font-mono">{k.kind}</span>
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                  {countActive(k.entries)} active · {k.entries.length} total
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <Table>
                <THead>
                  <tr>
                    <TH>name</TH>
                    <TH>tags</TH>
                    <TH>in scope since</TH>
                    <TH>retired on</TH>
                    <TH>state</TH>
                    <TH>extras</TH>
                  </tr>
                </THead>
                <TBody>
                  {k.entries.map((e) => (
                    <TR key={e.name}>
                      <TD className="font-mono text-xs">{e.name}</TD>
                      <TD>
                        <div className="flex flex-wrap gap-1">
                          {e.tags.map((t) => (
                            <Badge key={t} variant="neutral">
                              {t}
                            </Badge>
                          ))}
                        </div>
                      </TD>
                      <TD className="text-xs text-muted-foreground">
                        {e.in_scope_since ?? "—"}
                      </TD>
                      <TD className="text-xs text-muted-foreground">
                        {e.retired_on ?? "—"}
                      </TD>
                      <TD>
                        <StateBadge entry={e} />
                      </TD>
                      <TD className="font-mono text-[10px] text-muted-foreground">
                        {summariseExtras(e.extras)}
                      </TD>
                    </TR>
                  ))}
                </TBody>
              </Table>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}

function matches(e: InventoryEntryView, q: string): boolean {
  if (q === "") return true;
  if (e.name.toLowerCase().includes(q)) return true;
  if (e.tags.some((t) => t.toLowerCase().includes(q))) return true;
  if (e.aliases.some((a) => a.toLowerCase().includes(q))) return true;
  for (const value of Object.values(e.extras)) {
    if (typeof value === "string" && value.toLowerCase().includes(q)) return true;
  }
  return false;
}

function countActive(entries: InventoryEntryView[]): number {
  return entries.filter((e) => e.active_today).length;
}

function StateBadge({ entry }: { entry: InventoryEntryView }) {
  if (entry.retired_on != null) {
    const t = Date.parse(entry.retired_on);
    if (!Number.isNaN(t) && t <= Date.now()) {
      return <Badge variant="error">retired</Badge>;
    }
  }
  if (entry.in_scope_since != null) {
    const t = Date.parse(entry.in_scope_since);
    if (!Number.isNaN(t) && t > Date.now()) {
      return <Badge variant="warn">not yet</Badge>;
    }
  }
  return entry.active_today ? (
    <Badge variant="ok">active</Badge>
  ) : (
    <Badge variant="neutral">inactive</Badge>
  );
}

function summariseExtras(extras: Record<string, unknown>): string {
  const keys = Object.keys(extras).filter((k) => k !== "name" && k !== "tags");
  if (keys.length === 0) return "";
  return keys
    .slice(0, 4)
    .map((k) => `${k}=${stringifyShort(extras[k])}`)
    .join(", ");
}

function stringifyShort(v: unknown): string {
  if (typeof v === "string") return v.length > 30 ? `${v.slice(0, 27)}…` : v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  return "…";
}
