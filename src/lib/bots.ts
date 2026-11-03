/**
 * Bots on the frontend: what a bot is doing in a few words, the order its tasks are shown in,
 * and the colors of its mark. The desktop app owns bots and runs their queues.
 */
import type { Bot, BotColor, BotSchedule, BotTask } from "./live";
import { fmt, t, tn, useI18n } from "./i18n";
import { clock } from "./format";

export const BOT_COLORS: BotColor[] = ["violet", "blue", "green", "amber", "rose", "teal"];

/** Tailwind classes for a bot's mark, per accent. */
export const BOT_MARK: Record<BotColor, string> = {
  violet: "bg-violet-soft text-violet-text",
  blue: "bg-[color-mix(in_srgb,var(--blue)_16%,transparent)] text-blue",
  green: "bg-[color-mix(in_srgb,var(--success)_16%,transparent)] text-success",
  amber: "bg-[color-mix(in_srgb,var(--warning)_16%,transparent)] text-warning",
  rose: "bg-[color-mix(in_srgb,var(--error)_14%,transparent)] text-error",
  teal: "bg-[color-mix(in_srgb,#14b8a6_16%,transparent)] text-[#14b8a6]",
};

/** English names; translated where shown. */
export const COLOR_NAMES: Record<BotColor, string> = /* i18n */ {
  violet: "Violet",
  blue: "Blue",
  green: "Green",
  amber: "Amber",
  rose: "Rose",
  teal: "Teal",
};

export type BotState = "needs" | "working" | "queued" | "paused" | "idle";

/** Where a bot is: waiting for you first, then working, queued, paused or idle. */
export function botState(bot: Pick<Bot, "needsYou" | "runningTaskId" | "queued" | "paused">): BotState {
  if (bot.needsYou) return "needs";
  if (bot.runningTaskId) return "working";
  if (bot.paused) return "paused";
  if (bot.queued > 0) return "queued";
  return "idle";
}

/** The sidebar's status line: "Working on: …", "3 queued", "Paused", "Needs you", "Idle". */
export function botStatusLine(bot: Bot): string {
  switch (botState(bot)) {
    case "needs":
      return t("Needs you");
    case "working":
      return t("Working on: {task}", { task: firstLine(bot.runningText ?? "") });
    case "paused":
      return bot.queued > 0 ? tn(bot.queued, "Paused, {n} queued", "Paused, {n} queued") : t("Paused");
    case "queued":
      return tn(bot.queued, "{n} queued", "{n} queued");
    default:
      return bot.nextRunAt ? t("Next: {time}", { time: whenText(bot.nextRunAt) }) : t("Idle");
  }
}

/** A time to come, short: "09:00" today, "Tue 09:00" this week, "12 Nov 09:00" later. */
export function whenText(ms: number, now = Date.now()): string {
  const loc = useI18n.getState().locale;
  const d = new Date(ms);
  const today = new Date(now);
  if (d.toDateString() === today.toDateString()) return clock(ms);
  const days = (new Date(d).setHours(0, 0, 0, 0) - new Date(today).setHours(0, 0, 0, 0)) / 86_400_000;
  const day = days > 0 && days < 7 ? d.toLocaleDateString(loc, { weekday: "short" }) : d.toLocaleDateString(loc, { day: "numeric", month: "short" });
  return `${day} ${clock(ms)}`;
}

/** Short weekday names in the interface language, Monday (1) first. */
export function weekdayName(day: number, width: "short" | "narrow" | "long" = "short"): string {
  // 1 January 2024 was a Monday.
  return new Date(2024, 0, day).toLocaleDateString(useI18n.getState().locale, { weekday: width });
}

/** "Weekdays at 09:00 and 17:30", "Every day at 09:00", "Mon, Thu at 09:00", "Every 6 hours". */
export function scheduleSummary(s: Pick<BotSchedule, "kind" | "times" | "weekdays" | "everyMinutes">): string {
  if (s.kind === "interval") {
    const m = s.everyMinutes ?? 0;
    return m % 60 === 0 ? tn(m / 60, "Every hour", "Every {n} hours") : tn(m, "Every {n} minutes", "Every {n} minutes");
  }
  const times = fmt.list(s.times);
  const days = [...s.weekdays].sort((a, b) => a - b);
  if (days.length === 0 || days.length === 7) return t("Every day at {times}", { times });
  if (days.join() === "1,2,3,4,5") return t("Weekdays at {times}", { times });
  if (days.join() === "6,7") return t("Weekends at {times}", { times });
  return t("{days} at {times}", { days: days.map((d) => weekdayName(d)).join(", "), times });
}

/** Adds or replaces a schedule in a bot's list, in the order they were made. */
export function mergeSchedule(list: BotSchedule[], s: BotSchedule): BotSchedule[] {
  const i = list.findIndex((x) => x.id === s.id);
  if (i < 0) return [...list, s];
  const next = [...list];
  next[i] = s;
  return next;
}

/** The soonest next run of a bot's schedules that are on. */
export function soonest(list: BotSchedule[]): number | null {
  const runs = list.filter((s) => s.enabled && s.nextRunAt).map((s) => s.nextRunAt!);
  return runs.length ? Math.min(...runs) : null;
}

/** Every session that belongs to a bot (one per place it works in). */
export function botSessionIds(bots: Bot[]): Set<string> {
  return new Set(bots.flatMap((b) => b.places.map((p) => p.sessionId)));
}

/** A bot's session for a place (null: its workspace). */
export function placeSession(bot: Bot, projectId: string | null): string | null {
  return bot.places.find((p) => (p.projectId ?? null) === projectId)?.sessionId ?? null;
}

/** The bot's workspace folder for a name, as the desktop app makes it (bots.rs `folder_name`). */
export function workspaceFolder(name: string): string {
  let out = "";
  for (const ch of name.trim()) {
    // eslint-disable-next-line no-control-regex
    const c = /[\u0000-\u001f\u007f-\u009f/\\:*?"<>|]/.test(ch) ? "-" : /\s/.test(ch) ? " " : ch;
    if (c === " " && out.endsWith(" ")) continue;
    out += c;
  }
  out = [...out.replace(/^[. ]+/, "")].slice(0, 60).join("").replace(/[. ]+$/, "");
  return out === "" || out === "-" ? "Bot" : out;
}

/** Agents that can take the agentdeck MCP server, so their bots can hand work to other bots
 * (mcp.rs `supports`). Antigravity and the API agent can't; other bots can still hand them work. */
export function canDelegate(provider: string): boolean {
  return provider === "claude-code" || provider === "codex" || provider === "gemini-cli" || provider === "grok-build" || provider === "kimi-code";
}

/** The bot that handed a task over ("bot:<id>"), or null. */
export function fromBotId(source: string): string | null {
  return source.startsWith("bot:") ? source.slice(4) || null : null;
}

/** Tasks handed over by a task, from every bot's list we have, oldest first. */
export function childrenOf(parentId: string, lists: Record<string, BotTask[]>): BotTask[] {
  return Object.values(lists)
    .flat()
    .filter((x) => x.parentId === parentId)
    .sort((a, b) => a.createdAt - b.createdAt);
}

export function firstLine(text: string, max = 80): string {
  const line = text.trim().split("\n")[0]!.replace(/\s+/g, " ");
  return line.length > max ? `${line.slice(0, max - 1).trimEnd()}…` : line;
}

/** The queue as the bot page shows it: the running task, the queue in order, then the rest, newest first. */
export function orderTasks(tasks: BotTask[]): { running: BotTask | undefined; queued: BotTask[]; recent: BotTask[] } {
  const queued = tasks.filter((x) => x.status === "queued");
  return {
    running: tasks.find((x) => x.status === "running"),
    queued,
    recent: tasks
      .filter((x) => x.status !== "queued" && x.status !== "running")
      .sort((a, b) => (b.finishedAt ?? b.createdAt) - (a.finishedAt ?? a.createdAt)),
  };
}

/** Adds or replaces a task in a bot's list, keeping queued tasks in the order they arrived. */
export function mergeTask(list: BotTask[], task: BotTask): BotTask[] {
  const i = list.findIndex((x) => x.id === task.id);
  if (i >= 0) {
    const next = [...list];
    next[i] = task;
    return next;
  }
  return [...list, task];
}

/** "4:05" or "1:02:05" since `from`. */
export function elapsed(from: number, now: number): string {
  const s = Math.max(0, Math.floor((now - from) / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** Initials for a bot's mark: "PR Reviewer" → "PR", "docs" → "DO". */
export function botInitials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  const pick = words.length > 1 ? words[0]![0]! + words[1]![0]! : (words[0] ?? "?").slice(0, 2);
  return pick.toUpperCase();
}

/** Whether a bot name is free (names are unique, ignoring case). */
export function nameTaken(bots: Bot[], name: string, exceptId?: string): boolean {
  const n = name.trim().toLowerCase();
  return bots.some((b) => b.id !== exceptId && b.name.trim().toLowerCase() === n);
}
