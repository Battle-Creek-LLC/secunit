// Tiny helpers for the relative + absolute timestamps the views render.
// Avoid pulling in date-fns / dayjs for what is two functions.

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const WEEK = 7 * DAY;
const MONTH = 30 * DAY;
const YEAR = 365 * DAY;

export function relTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = now - t;
  const past = diff >= 0;
  const ms = Math.abs(diff);
  const word = (n: number, unit: string) =>
    `${n}${unit} ${past ? "ago" : "from now"}`;
  if (ms < MINUTE) return past ? "just now" : "in a moment";
  if (ms < HOUR) return word(Math.round(ms / MINUTE), "m");
  if (ms < DAY) return word(Math.round(ms / HOUR), "h");
  if (ms < WEEK) return word(Math.round(ms / DAY), "d");
  if (ms < MONTH) return word(Math.round(ms / WEEK), "w");
  if (ms < YEAR) return word(Math.round(ms / MONTH), "mo");
  return word(Math.round(ms / YEAR), "y");
}

export function formatTimestamp(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

export function daysFromNow(iso: string | null | undefined, now = Date.now()): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return Math.round((t - now) / DAY);
}
