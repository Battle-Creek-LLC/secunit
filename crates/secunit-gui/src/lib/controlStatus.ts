// Splits the IPC's combined `ControlStatus` (which mixes run-state and
// scheduling urgency) into two independent axes for the Controls page:
//
//   * `Status` — the run-history facet only: did the last run succeed,
//     is one in flight, has it ever run? Maps `overdue`/`due-soon` back to
//     the underlying outcome.
//   * `Urgency` — the scheduling facet: based on `next_due` and the
//     `overdue` flag, with a fixed 7-day "due soon" horizon.
//
// Other pages still consume the combined IPC enum verbatim; this module
// is scoped to the Controls table.

import type { ControlSummary } from "@/lib/ipc";

export type Status =
  | "sealed"
  | "failed"
  | "in-progress"
  | "never-run"
  | "idle";

export type Urgency = "overdue" | "due-soon" | "on-track" | "none";

export const DUE_SOON_HORIZON_DAYS = 7;

export function status(c: ControlSummary): Status {
  if (c.status === "overdue" || c.status === "due-soon") {
    switch (c.last_status) {
      case "complete":
        return "sealed";
      case "in-progress":
        return "in-progress";
      case "failed":
        return "failed";
      case "never-run":
      case null:
      default:
        return "never-run";
    }
  }
  return c.status as Status;
}

export function urgency(c: ControlSummary, today: Date = new Date()): Urgency {
  if (c.overdue) return "overdue";
  if (!c.next_due) return "none";
  const due = Date.parse(c.next_due);
  if (Number.isNaN(due)) return "none";
  const days = Math.round((due - today.getTime()) / 86_400_000);
  if (days <= DUE_SOON_HORIZON_DAYS) return "due-soon";
  return "on-track";
}
