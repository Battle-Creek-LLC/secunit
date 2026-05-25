// Shared helpers for the read-only Risks views: badge tones for severity
// and lifecycle status, and the SLA countdown (red when overdue) computed
// from `due_at` against today. Mirrors the CLI's `--past-sla` notion: a
// risk is past SLA when it is still actionable (not a terminal good state)
// and its due date is in the past.

import type { BadgeVariant } from "@/components/ui";
import type { RiskSeverity, RiskStatus } from "@/lib/ipc";

const DAY_MS = 24 * 60 * 60 * 1000;

export const SEVERITY_ORDER: Record<RiskSeverity, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
  info: 4,
};

export const severityTone: Record<RiskSeverity, BadgeVariant> = {
  critical: "error",
  high: "error",
  medium: "warn",
  low: "info",
  info: "neutral",
};

export const statusTone: Record<RiskStatus, BadgeVariant> = {
  open: "warn",
  "in-progress": "info",
  remediated: "ok",
  reopened: "warn",
  "accepted-exception": "neutral",
  "false-positive": "neutral",
};

export const statusLabel: Record<RiskStatus, string> = {
  open: "open",
  "in-progress": "in progress",
  remediated: "remediated",
  reopened: "reopened",
  "accepted-exception": "accepted (exception)",
  "false-positive": "false positive",
};

/** Statuses that still carry an actionable SLA. Terminal-good states stop
 *  the clock so a remediated/false-positive risk never reads "overdue". */
const ACTIVE_SLA_STATUSES: ReadonlySet<RiskStatus> = new Set<RiskStatus>([
  "open",
  "in-progress",
  "reopened",
  "accepted-exception",
]);

export interface SlaCountdown {
  /** Whole days until `due_at` (negative = past due). null if no due date. */
  days: number | null;
  /** Past the due date while still actionable. */
  overdue: boolean;
  /** Human label: "12d left", "due today", "3d overdue", or "—". */
  label: string;
}

export function slaCountdown(
  dueAt: string | null,
  status: RiskStatus,
  now: number = Date.now(),
): SlaCountdown {
  if (!dueAt) return { days: null, overdue: false, label: "—" };
  const t = Date.parse(dueAt);
  if (Number.isNaN(t)) return { days: null, overdue: false, label: dueAt };
  // Compare on calendar days (due_at is a date, not a datetime).
  const today = Math.floor(now / DAY_MS);
  const due = Math.floor(t / DAY_MS);
  const days = due - today;
  const active = ACTIVE_SLA_STATUSES.has(status);
  const overdue = active && days < 0;

  let label: string;
  if (!active && (status === "remediated" || status === "false-positive")) {
    label = "closed";
  } else if (days < 0) {
    label = `${-days}d overdue`;
  } else if (days === 0) {
    label = "due today";
  } else {
    label = `${days}d left`;
  }
  return { days, overdue, label };
}
