/**
 * Time tracking: how long agents worked, waited for you, and how long you spent with them.
 * The desktop app measures (src-tauri/src/time.rs); this file sums it up for the Time tab and
 * keeps the pure parts tested.
 */
import { isTauri } from "./native";

export type TimeRow = { projectId: string; sessionId: string; branch: string | null; day: string; agentMs: number; waitingMs: number; youMs: number };
/** A teammate's shared time on one branch and day. */
export type BranchDay = { branch: string; day: string; agentMs: number; waitingMs: number; youMs: number };
export type Totals = { agentMs: number; waitingMs: number; youMs: number };

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Minutes east of UTC, as the desktop app wants it (+180 in Istanbul). */
export const tzOffset = () => -new Date().getTimezoneOffset();

export const timeApi = {
  available: isTauri,
  /** The last `days` local days, today included. */
  report: (days: number) => call<TimeRow[]>("time_report", { days, tzOffset: tzOffset() }),
};

export const zero = (): Totals => ({ agentMs: 0, waitingMs: 0, youMs: 0 });
const add = (t: Totals, r: Totals) => {
  t.agentMs += r.agentMs;
  t.waitingMs += r.waitingMs;
  t.youMs += r.youMs;
  return t;
};
export const total = (t: Totals) => t.agentMs + t.waitingMs + t.youMs;

export function sum(rows: Totals[]): Totals {
  return rows.reduce(add, zero());
}

/** Local "YYYY-MM-DD" of a moment. */
export function dayOf(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** The last `n` local days, oldest first, ending with today. */
export function lastDays(n: number, now = Date.now()): string[] {
  const d = new Date(now);
  return Array.from({ length: n }, (_, i) => dayOf(new Date(d.getFullYear(), d.getMonth(), d.getDate() - (n - 1 - i), 12).getTime()));
}

/** Rows summed per key, the biggest total first. */
export function groupBy<R extends Totals>(rows: R[], key: (r: R) => string): { key: string; totals: Totals; rows: R[] }[] {
  const m = new Map<string, { key: string; totals: Totals; rows: R[] }>();
  for (const r of rows) {
    const k = key(r);
    let g = m.get(k);
    if (!g) {
      g = { key: k, totals: zero(), rows: [] };
      m.set(k, g);
    }
    add(g.totals, r);
    g.rows.push(r);
  }
  return [...m.values()].sort((a, b) => total(b.totals) - total(a.totals) || a.key.localeCompare(b.key));
}

/** One bar per day (zeros included) for the chart. */
export function perDay(rows: (Totals & { day: string })[], days: string[]): { day: string; totals: Totals }[] {
  const by = new Map(days.map((d) => [d, zero()]));
  for (const r of rows) {
    const t = by.get(r.day);
    if (t) add(t, r);
  }
  return days.map((day) => ({ day, totals: by.get(day)! }));
}

/** "1h 05m", "12m", "45s", "0m". */
export function hm(ms: number): string {
  if (ms <= 0) return "0m";
  if (ms < 60_000) return `${Math.max(1, Math.round(ms / 1000))}s`;
  const m = Math.round(ms / 60_000);
  if (m < 60) return `${m}m`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** Hours with two decimals, for spreadsheets. */
const hours = (ms: number) => (ms / 3_600_000).toFixed(2);

function csvCell(v: string): string {
  return /[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
}

/** A timesheet: one line per day, project, branch and session, hours as decimals. */
export function toCsv(rows: TimeRow[], names: { project: (id: string) => string; session: (id: string) => string }): string {
  const head = ["date", "project", "branch", "session", "agent_hours", "waiting_hours", "your_hours"];
  const lines = [...rows]
    .sort((a, b) => a.day.localeCompare(b.day) || names.project(a.projectId).localeCompare(names.project(b.projectId)) || (a.branch ?? "").localeCompare(b.branch ?? ""))
    .map((r) => [r.day, names.project(r.projectId), r.branch ?? "", names.session(r.sessionId), hours(r.agentMs), hours(r.waitingMs), hours(r.youMs)].map(csvCell).join(","));
  return [head.join(","), ...lines].join("\n") + "\n";
}

/** A teammate's shared rows within the given days. */
export function sharedIn(time: BranchDay[] | undefined, days: string[]): BranchDay[] {
  const set = new Set(days);
  return (time ?? []).filter((r) => set.has(r.day));
}
