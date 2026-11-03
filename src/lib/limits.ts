/**
 * Plan usage limits (the subscription's 5-hour and weekly windows) as Claude Code and Codex
 * report them. The desktop app normalizes both agents to `PlanLimits` and sends updates on
 * `agentdeck://limits`; `plan_limits` returns the latest ones, saved ones included.
 */
import { create } from "zustand";
import { t, useI18n } from "./i18n";

export interface LimitWindow {
  /** "five_hour", "weekly", "weekly_opus", "weekly_sonnet", or "window_<minutes>" with `label`. */
  kind: string;
  /** A short length for windows without a kind of their own ("2h", "30d"). */
  label?: string;
  /** 0–100 (can pass 100). */
  usedPercent: number;
  /** Unix milliseconds. */
  resetsAt?: number;
  /** "allowed" | "allowed_warning" | "rejected". */
  status?: string;
}

export interface PlanLimits {
  provider: string;
  windows: LimitWindow[];
  /** When the numbers were read from the agent, unix milliseconds. */
  updatedAt: number;
}

/** Agents that report plan limits. */
export const LIMIT_PROVIDERS = ["claude-code", "codex"] as const;

interface LimitsState {
  byProvider: Record<string, PlanLimits>;
  put: (l: PlanLimits) => void;
}

export const useLimits = create<LimitsState>((set) => ({
  byProvider: {},
  put: (l) =>
    set((s) => {
      const cur = s.byProvider[l.provider];
      // A saved snapshot loaded late never replaces a newer live one.
      if (cur && cur.updatedAt > l.updatedAt) return s;
      return { byProvider: { ...s.byProvider, [l.provider]: l } };
    }),
}));

export type LimitLevel = "ok" | "warning" | "error";

export function limitLevel(w: LimitWindow): LimitLevel {
  if (w.status === "rejected" || w.usedPercent >= 100) return "error";
  if (w.status === "allowed_warning" || w.usedPercent >= 80) return "warning";
  return "ok";
}

/** Windows still running at `now`, in display order (5-hour first). */
export function liveWindows(l: PlanLimits | undefined, now: number): LimitWindow[] {
  return (l?.windows ?? []).filter((w) => w.resetsAt === undefined || w.resetsAt > now);
}

/** "5h", "Week", or the window's own length. */
export function windowLabel(w: LimitWindow): string {
  switch (w.kind) {
    case "five_hour":
      return t("5h");
    case "weekly":
      return t("Week");
    case "weekly_opus":
      return t("Opus week");
    case "weekly_sonnet":
      return t("Sonnet week");
    default:
      return w.label ?? w.kind;
  }
}

/** "5-hour limit", "Weekly limit", "2h limit". */
export function windowName(w: LimitWindow): string {
  switch (w.kind) {
    case "five_hour":
      return t("5-hour limit");
    case "weekly":
      return t("Weekly limit");
    case "weekly_opus":
      return t("Weekly Opus limit");
    case "weekly_sonnet":
      return t("Weekly Sonnet limit");
    default:
      return t("{length} limit", { length: w.label ?? w.kind });
  }
}

const locale = () => useI18n.getState().locale;

/** "34 %" the way the current language writes it; whole percents. */
export function percentText(p: number): string {
  return new Intl.NumberFormat(locale(), { style: "percent", maximumFractionDigits: 0 }).format(Math.max(0, p) / 100);
}

const DAY = 86_400_000;

/** When a window resets, briefly: a time within a day, a weekday within a week, else a date. */
export function resetShort(at: number, now: number): string {
  const loc = locale();
  const d = new Date(at);
  if (at - now < DAY) return new Intl.DateTimeFormat(loc, { hour: "numeric", minute: "2-digit" }).format(d);
  if (at - now < 6 * DAY) return new Intl.DateTimeFormat(loc, { weekday: "short" }).format(d);
  return new Intl.DateTimeFormat(loc, { month: "short", day: "numeric" }).format(d);
}

/** The exact reset (or reading) moment: weekday, date and time. */
export function dateTimeLong(at: number): string {
  return new Intl.DateTimeFormat(locale(), { weekday: "long", month: "long", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(at));
}

/** When the numbers were read: the time today, else the date and time. */
export function asOfText(at: number, now: number): string {
  const loc = locale();
  const same = new Date(at).toDateString() === new Date(now).toDateString();
  return new Intl.DateTimeFormat(loc, same ? { hour: "numeric", minute: "2-digit" } : { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(at));
}

/** The line under the composer for one window: "34% · resets 18:20". */
export function windowSummary(w: LimitWindow, now: number): string {
  const percent = percentText(w.usedPercent);
  return w.resetsAt ? t("{percent} · resets {when}", { percent, when: resetShort(w.resetsAt, now) }) : percent;
}

/** The tooltip: what the window is, how much is used, the exact reset and when it was read. */
export function windowDetail(w: LimitWindow, updatedAt: number, now: number): string {
  const parts = [
    limitLevel(w) === "error" && w.status === "rejected"
      ? t("{name}: limit reached.", { name: windowName(w) })
      : t("{name}: {percent} used.", { name: windowName(w), percent: percentText(w.usedPercent) }),
  ];
  if (w.resetsAt) parts.push(t("Resets {when}.", { when: dateTimeLong(w.resetsAt) }));
  parts.push(t("As of {time}.", { time: asOfText(updatedAt, now) }));
  return parts.join(" ");
}
