import { Badge, type BadgeVariant } from "@/components/ui";
import type { ControlStatus } from "@/lib/ipc";

const tones: Record<ControlStatus, BadgeVariant> = {
  overdue: "error",
  "due-soon": "warn",
  "in-progress": "info",
  sealed: "ok",
  aborted: "error",
  "never-run": "neutral",
  idle: "neutral",
};

const labels: Record<ControlStatus, string> = {
  overdue: "overdue",
  "due-soon": "due soon",
  "in-progress": "in progress",
  sealed: "sealed",
  aborted: "aborted",
  "never-run": "never run",
  idle: "idle",
};

export function StatusBadge({ status }: { status: ControlStatus }) {
  return <Badge variant={tones[status]}>{labels[status]}</Badge>;
}
