import { Input, Label, Select } from "@/components/ui";

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
        <Select
          id="ff-c"
          className="mt-0.5 w-full"
          value={controlId}
          onChange={(v) => onChange({ control_id: v })}
          options={[
            { value: "", label: "all" },
            ...controlOptions.map((c) => ({ value: c, label: c })),
          ]}
        />
      </div>
      <div>
        <Label htmlFor="ff-quarter">quarter</Label>
        <Select
          id="ff-quarter"
          className="mt-0.5 w-full"
          value={quarter}
          onChange={(v) => onChange({ quarter: v })}
          options={[
            { value: "", label: "all" },
            ...quarterOptions.map((q) => ({ value: q, label: q })),
          ]}
        />
      </div>
    </aside>
  );
}
