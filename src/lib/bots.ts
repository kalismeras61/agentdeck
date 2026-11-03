/**
 * Bots on the frontend: what a bot is doing in a few words, the order its tasks are shown in,
 * and the colors of its mark. The desktop app owns bots and runs their queues.
 */
import type { Bot, BotColor, BotTask } from "./live";
import { t, tn } from "./i18n";

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
      return t("Idle");
  }
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
