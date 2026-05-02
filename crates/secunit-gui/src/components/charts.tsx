import { cn } from "@/lib/cn";

interface SparklineProps {
  values: number[];
  width?: number;
  height?: number;
  className?: string;
  ariaLabel?: string;
}

export function Sparkline({
  values,
  width = 96,
  height = 24,
  className,
  ariaLabel,
}: SparklineProps) {
  if (values.length === 0) {
    return (
      <svg
        width={width}
        height={height}
        className={cn("text-muted-foreground/40", className)}
        aria-label={ariaLabel}
        role={ariaLabel ? "img" : undefined}
      >
        <line
          x1={0}
          y1={height / 2}
          x2={width}
          y2={height / 2}
          stroke="currentColor"
          strokeWidth={1}
          strokeDasharray="2 3"
        />
      </svg>
    );
  }
  const max = Math.max(...values);
  const min = Math.min(...values);
  const range = max - min || 1;
  const stepX = values.length > 1 ? width / (values.length - 1) : 0;
  const points = values
    .map((v, i) => {
      const x = i * stepX;
      const y = height - ((v - min) / range) * height;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={cn("text-info", className)}
      aria-label={ariaLabel}
      role={ariaLabel ? "img" : undefined}
    >
      <polyline
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={points}
      />
    </svg>
  );
}

export interface Segment {
  value: number;
  label: string;
  className: string;
}

interface SegmentBarProps {
  segments: Segment[];
  className?: string;
}

export function SegmentBar({ segments, className }: SegmentBarProps) {
  const total = segments.reduce((acc, s) => acc + s.value, 0);
  if (total === 0) {
    return (
      <div
        className={cn("h-1.5 w-full rounded bg-muted", className)}
        aria-hidden="true"
      />
    );
  }
  return (
    <div
      className={cn("flex h-1.5 w-full overflow-hidden rounded bg-muted", className)}
      role="img"
      aria-label={segments
        .filter((s) => s.value > 0)
        .map((s) => `${s.value} ${s.label}`)
        .join(", ")}
    >
      {segments.map((s) =>
        s.value > 0 ? (
          <div
            key={s.label}
            className={s.className}
            style={{ width: `${(s.value / total) * 100}%` }}
            title={`${s.label}: ${s.value}`}
          />
        ) : null,
      )}
    </div>
  );
}
