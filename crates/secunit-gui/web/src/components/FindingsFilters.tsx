import { Input, Label } from "@/components/ui";

export interface FindingsFiltersProps {
  controlOptions: string[];
  quarterOptions: string[];
  controlId: string;
  quarter: string;
  query: string;
  onChange: (next: { control_id?: string; quarter?: string; query?: string }) => void;
}

export function FindingsFilters({
  controlOptions,
  quarterOptions,
  controlId,
  quarter,
  query,
  onChange,
}: FindingsFiltersProps) {
  return (
    <aside className="flex w-60 shrink-0 flex-col gap-4 border-r p-4">
      <div>
        <Label htmlFor="ff-q">search</Label>
        <Input
          id="ff-q"
          value={query}
          placeholder="text in heading…"
          onChange={(e) => onChange({ query: e.target.value })}
        />
      </div>
      <div>
        <Label htmlFor="ff-c">control</Label>
        <select
          id="ff-c"
          className="mt-0.5 h-8 w-full rounded-md border bg-background px-2 text-sm"
          value={controlId}
          onChange={(e) => onChange({ control_id: e.target.value })}
        >
          <option value="">all</option>
          {controlOptions.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>
      <div>
        <Label htmlFor="ff-quarter">quarter</Label>
        <select
          id="ff-quarter"
          className="mt-0.5 h-8 w-full rounded-md border bg-background px-2 text-sm"
          value={quarter}
          onChange={(e) => onChange({ quarter: e.target.value })}
        >
          <option value="">all</option>
          {quarterOptions.map((q) => (
            <option key={q} value={q}>
              {q}
            </option>
          ))}
        </select>
      </div>
    </aside>
  );
}
