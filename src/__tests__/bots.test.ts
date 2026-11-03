import { beforeEach, describe, expect, it } from "vitest";
import { agentdeckItem, agentdeckTool } from "../lib/agentdeckTools";
import {
  botInitials,
  botSessionIds,
  canDelegate,
  childrenOf,
  fromBotId,
  botState,
  botStatusLine,
  elapsed,
  mergeSchedule,
  mergeTask,
  nameTaken,
  orderTasks,
  placeSession,
  scheduleSummary,
  soonest,
  whenText,
  workspaceFolder,
} from "../lib/bots";
import type { BotSchedule } from "../lib/live";
import type { Bot, BotTask } from "../lib/live";
import { normalizeClaude, type StoredEvent } from "../lib/claude";
import { normalizeFor } from "../lib/normalize";
import { parseEvents } from "../lib/events";
import { listed, useStore } from "../state/store";

const initial = useStore.getState();

const bot = (over: Partial<Bot> = {}): Bot => ({
  id: "b-1",
  name: "Reviewer",
  role: "Reviews PRs",
  instructions: "",
  provider: "claude-code",
  model: "default",
  effort: null,
  profileId: null,
  projectId: "p-api",
  workspaceId: "p-ws",
  workspace: "/Users/me/AgentDeck/Bots/Reviewer",
  places: [],
  color: "violet",
  paused: false,
  createdAt: 1,
  updatedAt: 1,
  queued: 0,
  runningTaskId: null,
  runningText: null,
  runningProjectId: null,
  runningSessionId: null,
  nextRunAt: null,
  needsYou: false,
  ...over,
});

const task = (id: string, status: BotTask["status"], over: Partial<BotTask> = {}): BotTask => ({
  id,
  botId: "b-1",
  text: `Task ${id}`,
  source: "you",
  status,
  result: null,
  error: null,
  createdAt: 1,
  startedAt: null,
  finishedAt: null,
  parentId: null,
  projectId: null,
  sessionId: null,
  children: 0,
  ...over,
});

const sched = (over: Partial<BotSchedule> = {}): BotSchedule => ({
  id: "sc-1",
  botId: "b-1",
  text: "Check the build",
  projectId: null,
  kind: "daily",
  times: ["09:00"],
  weekdays: [],
  everyMinutes: null,
  enabled: true,
  lastRunAt: null,
  nextRunAt: null,
  lastTaskId: null,
  skippedAt: null,
  createdAt: 1,
  ...over,
});

describe("bot helpers", () => {
  it("says what a bot is doing, needing you first", () => {
    expect(botState(bot())).toBe("idle");
    expect(botStatusLine(bot())).toBe("Idle");
    expect(botStatusLine(bot({ queued: 3 }))).toBe("3 queued");
    expect(botStatusLine(bot({ paused: true }))).toBe("Paused");
    expect(botStatusLine(bot({ paused: true, queued: 2 }))).toBe("Paused, 2 queued");
    expect(botStatusLine(bot({ runningTaskId: "t", runningText: "Review PR 12\nand more", queued: 2 }))).toBe("Working on: Review PR 12");
    expect(botStatusLine(bot({ runningTaskId: "t", runningText: "x", needsYou: true }))).toBe("Needs you");
  });

  it("mentions the next scheduled run when idle", () => {
    const today9 = new Date();
    today9.setHours(23, 59, 0, 0);
    expect(botStatusLine(bot({ nextRunAt: today9.getTime() }))).toBe("Next: 23:59");
    expect(botStatusLine(bot({ nextRunAt: today9.getTime(), queued: 1 }))).toBe("1 queued");
    const later = new Date(2026, 10, 3, 9, 0).getTime();
    expect(whenText(later, new Date(2026, 10, 1, 12, 0).getTime())).toBe("Tue 09:00");
    expect(whenText(later, new Date(2026, 9, 20, 12, 0).getTime())).toBe("Nov 3 09:00");
  });

  it("sums up schedules", () => {
    expect(scheduleSummary(sched())).toBe("Every day at 09:00");
    expect(scheduleSummary(sched({ times: ["09:00", "17:30"], weekdays: [1, 2, 3, 4, 5] }))).toBe("Weekdays at 09:00 and 17:30");
    expect(scheduleSummary(sched({ weekdays: [6, 7] }))).toBe("Weekends at 09:00");
    expect(scheduleSummary(sched({ weekdays: [1, 4] }))).toBe("Mon, Thu at 09:00");
    expect(scheduleSummary(sched({ kind: "interval", times: [], everyMinutes: 360 }))).toBe("Every 6 hours");
    expect(scheduleSummary(sched({ kind: "interval", times: [], everyMinutes: 60 }))).toBe("Every hour");
    expect(scheduleSummary(sched({ kind: "interval", times: [], everyMinutes: 90 }))).toBe("Every 90 minutes");
    expect(soonest([sched({ nextRunAt: 5 }), sched({ id: "b", nextRunAt: 3, enabled: false }), sched({ id: "c", nextRunAt: 4 })])).toBe(4);
    expect(soonest([])).toBeNull();
    expect(mergeSchedule([sched()], sched({ text: "x" }))[0]!.text).toBe("x");
    expect(mergeSchedule([sched()], sched({ id: "sc-2" })).length).toBe(2);
  });

  it("finds a bot's sessions by place and names its workspace folder as the app does", () => {
    const b = bot({ places: [{ projectId: null, sessionId: "s-ws" }, { projectId: "p-api", sessionId: "s-api" }] });
    expect(placeSession(b, null)).toBe("s-ws");
    expect(placeSession(b, "p-api")).toBe("s-api");
    expect(placeSession(b, "p-other")).toBeNull();
    expect([...botSessionIds([b])]).toEqual(["s-ws", "s-api"]);
    // Same cases as bots.rs `folder_name`.
    expect(workspaceFolder("PR Reviewer")).toBe("PR Reviewer");
    expect(workspaceFolder("PR / Review: v2")).toBe("PR - Review- v2");
    expect(workspaceFolder("  ..hidden  ")).toBe("hidden");
    expect(workspaceFolder("../..")).toBe("Bot");
    expect(workspaceFolder("/")).toBe("Bot");
    expect(workspaceFolder("Çiçek   Bot.")).toBe("Çiçek Bot");
    expect(workspaceFolder("x".repeat(80)).length).toBe(60);
  });

  it("orders the queue: running, queued in order, then the newest finished", () => {
    const { running, queued, recent } = orderTasks([
      task("a", "done", { finishedAt: 5 }),
      task("b", "queued"),
      task("c", "running"),
      task("d", "failed", { finishedAt: 9 }),
      task("e", "queued"),
    ]);
    expect(running?.id).toBe("c");
    expect(queued.map((x) => x.id)).toEqual(["b", "e"]);
    expect(recent.map((x) => x.id)).toEqual(["d", "a"]);
  });

  it("merges task updates in place and adds new ones at the end", () => {
    const list = [task("a", "queued"), task("b", "queued")];
    expect(mergeTask(list, task("a", "running")).map((x) => `${x.id}:${x.status}`)).toEqual(["a:running", "b:queued"]);
    expect(mergeTask(list, task("c", "queued")).map((x) => x.id)).toEqual(["a", "b", "c"]);
  });

  it("formats elapsed time, initials and checks names", () => {
    expect(elapsed(0, 65_000)).toBe("1:05");
    expect(elapsed(0, 3_725_000)).toBe("1:02:05");
    expect(botInitials("PR Reviewer")).toBe("PR");
    expect(botInitials("docs")).toBe("DO");
    expect(nameTaken([bot()], " reviewer ")).toBe(true);
    expect(nameTaken([bot()], "Reviewer", "b-1")).toBe(false);
  });

  it("shows a bot's task as a labeled message instead of the generated prompt", () => {
    const prompt = "You're Reviewer, a bot in AgentDeck.\n\nTask from you (2026-11-03 14:05 UTC):\nReview PR 12";
    const ev = (seq: number, kind: string, payload: Record<string, unknown>) => ({ sessionId: "s1", seq, ts: seq, source: "agentdeck" as const, kind, payload });
    const raw = normalizeFor(
      "claude-code",
      [
        ev(1, "bot_task", { botId: "b-1", taskId: "t1", source: "you", text: "Review PR 12", prompt }),
        ev(2, "user_message", { text: prompt }),
        ev(3, "bot_needs_you", { botId: "b-1", requestId: "r1", why: "it deletes files" }),
      ],
      { sessionId: "s1", projectPath: "/p" },
    );
    const parsed = parseEvents(raw).filter((p) => p.ok);
    const users = parsed.flatMap((p) => (p.ok && p.event.type === "user_message" ? [p.event.payload] : []));
    expect(users).toEqual([{ text: "Review PR 12", label: "Task" }]);
    expect(parsed.some((p) => p.ok && p.event.type === "notice" && JSON.stringify(p.event.payload).includes("it deletes files"))).toBe(true);
  });
});

describe("bot store (demo)", () => {
  beforeEach(() => useStore.setState(initial, true));

  it("has labeled demo bots in the demo workspace and opens a bot page", () => {
    const st = useStore.getState();
    expect(Object.keys(st.bots)).toContain("b-demo-reviewer");
    st.openBot("b-demo-reviewer");
    expect(useStore.getState()).toMatchObject({ view: "bot", selectedBotId: "b-demo-reviewer" });
  });

  it("queues, cancels, retries and pauses demo tasks locally", async () => {
    const st = useStore.getState();
    expect(await st.addBotTask("b-demo-reviewer", "   ", null)).toBeNull();
    expect(await st.addBotTask("b-demo-reviewer", "Check the logout route", "p-api")).toBeNull();
    const added = useStore.getState().botTasks["b-demo-reviewer"]!.find((x) => x.text === "Check the logout route")!;
    expect(added.status).toBe("queued");
    expect(added.projectId).toBe("p-api");
    expect(useStore.getState().bots["b-demo-reviewer"]!.queued).toBe(3);
    st.cancelBotTask(added.id);
    expect(useStore.getState().botTasks["b-demo-reviewer"]!.find((x) => x.id === added.id)!.status).toBe("cancelled");
    st.retryBotTask(added.id);
    expect(useStore.getState().botTasks["b-demo-reviewer"]!.find((x) => x.id === added.id)!.status).toBe("queued");
    st.setBotPaused("b-demo-reviewer", true);
    expect(useStore.getState().bots["b-demo-reviewer"]!.paused).toBe(true);
  });

  it("can't make a bot without the desktop app, and deletes a demo bot", async () => {
    const st = useStore.getState();
    const err = await st.saveBot({
      name: "New",
      role: "",
      instructions: "",
      provider: "claude-code",
      model: "default",
      effort: null,
      profileId: null,
      projectId: null,
      color: "blue",
    });
    expect(err).toMatch(/on this Mac/);
    st.openBot("b-demo-docs");
    expect(await st.deleteBot("b-demo-docs")).toBeNull();
    expect(useStore.getState().bots["b-demo-docs"]).toBeUndefined();
    expect(useStore.getState().view).toBe("workspace");
  });

  it("has a demo bot with a workspace, two places and schedules", () => {
    const st = useStore.getState();
    const b = st.bots["b-demo-reviewer"]!;
    expect(b.places.map((p) => p.projectId)).toEqual([null, "p-api"]);
    const ws = st.projects.find((p) => p.id === b.workspaceId)!;
    expect(ws.hidden).toBe(true);
    expect(listed(ws)).toBe(false);
    expect(st.projects.filter(listed).some((p) => p.id === ws.id)).toBe(false);
    expect(st.sessions.some((s) => s.projectId === ws.id)).toBe(true);
    expect(st.botSchedules["b-demo-reviewer"]!.length).toBe(2);
    expect(st.bots["b-demo-docs"]!.nextRunAt).toBeGreaterThan(Date.now());
  });

  it("makes, switches, runs and deletes demo schedules locally", async () => {
    const st = useStore.getState();
    expect(
      await st.saveBotSchedule({
        botId: "b-demo-docs",
        text: "Check broken links",
        projectId: null,
        kind: "interval",
        times: [],
        weekdays: [],
        everyMinutes: 120,
        enabled: true,
      }),
    ).toBeNull();
    const made = useStore.getState().botSchedules["b-demo-docs"]!.find((x) => x.text === "Check broken links")!;
    expect(made.everyMinutes).toBe(120);
    expect(await st.setBotScheduleEnabled(made.id, false)).toBeNull();
    expect(useStore.getState().botSchedules["b-demo-docs"]!.find((x) => x.id === made.id)!.enabled).toBe(false);
    expect(await st.runBotSchedule(made.id)).toBeNull();
    const ran = useStore.getState().botTasks["b-demo-docs"]!.find((x) => x.text === "Check broken links")!;
    expect(ran.projectId).toBeNull();
    expect(ran.source).toBe("schedule");
    expect(await st.deleteBotSchedule(made.id)).toBeNull();
    expect(useStore.getState().botSchedules["b-demo-docs"]!.some((x) => x.id === made.id)).toBe(false);
    st.setBotPlace("b-demo-reviewer", null);
    expect(useStore.getState().botPlace["b-demo-reviewer"]).toBeNull();
  });

  it("keeps live task updates per bot", () => {
    const st = useStore.getState();
    st.upsertBot(bot({ id: "b-live" }));
    st.upsertBotTask(task("t1", "queued", { botId: "b-live" }));
    st.upsertBotTask(task("t1", "running", { botId: "b-live" }));
    st.upsertBotTask(task("t2", "queued", { botId: "b-live" }));
    expect(useStore.getState().botTasks["b-live"]!.map((x) => `${x.id}:${x.status}`)).toEqual(["t1:running", "t2:queued"]);
  });

  it("shows what a task handed to other bots, and keeps it current", async () => {
    const st = useStore.getState();
    st.upsertBotTask(task("p1", "running", { botId: "b-boss", children: 2 }));
    st.upsertBotTask(task("c2", "queued", { botId: "b-b", parentId: "p1", source: "bot:b-boss", createdAt: 3 }));
    st.upsertBotTask(task("c1", "running", { botId: "b-a", parentId: "p1", source: "bot:b-boss", createdAt: 2 }));
    expect(childrenOf("p1", useStore.getState().botTasks).map((x) => x.id)).toEqual(["c1", "c2"]);
    // Not in the desktop app: loaded from the lists the page has.
    await st.loadTaskChildren("p1");
    expect(useStore.getState().taskChildren["p1"]!.map((x) => x.id)).toEqual(["c1", "c2"]);
    st.upsertBotTask(task("c1", "done", { botId: "b-a", parentId: "p1", source: "bot:b-boss", createdAt: 2, result: "ok" }));
    expect(useStore.getState().taskChildren["p1"]!.find((x) => x.id === "c1")!.status).toBe("done");
    expect(fromBotId("bot:b-boss")).toBe("b-boss");
    expect(fromBotId("you")).toBeNull();
    expect(fromBotId("schedule")).toBeNull();
  });

  it("says which agents can hand work to other bots", () => {
    for (const p of ["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code"]) expect(canDelegate(p)).toBe(true);
    for (const p of ["antigravity", "api"]) expect(canDelegate(p)).toBe(false);
  });

  it("names the agentdeck tools in the conversation, whatever the agent calls them", () => {
    expect(agentdeckTool("mcp__agentdeck__delegate_task")).toBe("delegate_task");
    expect(agentdeckTool("mcp__agentdeck__nope")).toBeNull();
    expect(agentdeckTool("mcp__other__delegate_task")).toBeNull();
    expect(agentdeckTool("wait_for_tasks (agentdeck MCP Server)")).toBe("wait_for_tasks");
    expect(agentdeckTool("list_bots", "agentdeck")).toBe("list_bots");
    expect(agentdeckTool("list_bots", "github")).toBeNull();
    expect(agentdeckItem("delegate_task", { bot: "Docs Writer", task: "Update the changelog\nfor v2.3" })).toEqual({
      label: "Delegated to Docs Writer",
      target: "Update the changelog",
    });
    expect(agentdeckItem("delegate_task").label).toBe("Delegated a task");
    expect(agentdeckItem("wait_for_tasks", { task_ids: ["a", "b"] }).label).toBe("Waited for 2 tasks");
    expect(agentdeckItem("task_status", { task_ids: ["a"] }).label).toBe("Checked 1 task");
    expect(agentdeckItem("list_bots").label).toBe("Listed the bots");
    // Claude Code's tool calls, as stored.
    const events: StoredEvent[] = [
      {
        sessionId: "s",
        seq: 1,
        ts: 1,
        source: "claude",
        kind: "assistant",
        payload: {
          message: {
            content: [
              { type: "tool_use", id: "tu1", name: "mcp__agentdeck__delegate_task", input: { bot: "greeter", task: "Say hello" } },
              { type: "tool_use", id: "tu2", name: "mcp__agentdeck__wait_for_tasks", input: { task_ids: ["t-1"] } },
            ],
          },
        },
      },
    ];
    const out = normalizeClaude(events, { sessionId: "s", projectPath: "/p" }) as { type: string; payload: { items: { label: string; target?: string }[] } }[];
    const items = out.find((e) => e.type === "tool_group")!.payload.items;
    expect(items.map((i) => [i.label, i.target])).toEqual([
      ["Delegated to greeter", "Say hello"],
      ["Waited for 1 task", undefined],
    ]);
  });
});
