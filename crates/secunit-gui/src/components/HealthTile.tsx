import { Link } from "react-router-dom";
import { cn } from "@/lib/cn";
import type { BadgeVariant } from "@/components/ui";

interface HealthTileProps {
  label: string;
  value: number;
  caption: string;
  to?: string;
  tone?: BadgeVariant;
}

const toneRing: Record<BadgeVariant, string> = {
  ok: "border-ok/30",
  warn: "border-warn/40",
  error: "border-error/30",
  info: "border-info/30",
  neutral: "border-border",
};

const toneText: Record<BadgeVariant, string> = {
  ok: "text-ok",
  warn: "text-warn",
  error: "text-error",
  info: "text-info",
  neutral: "text-foreground",
};

export function HealthTile({
  label,
  value,
  caption,
  to,
  tone = "neutral",
}: HealthTileProps) {
  const inner = (
    <div
      className={cn(
        "flex flex-col gap-1 rounded-lg border bg-background p-4",
        "hover:bg-muted/30 transition-colors",
        toneRing[tone],
      )}
    >
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <span className={cn("text-3xl font-semibold tabular-nums", toneText[tone])}>
        {value}
      </span>
      <span className="text-xs text-muted-foreground">{caption}</span>
    </div>
  );
  if (!to) return inner;
  return (
    <Link
      to={to}
      className="rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {inner}
    </Link>
  );
}
