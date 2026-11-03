import { beforeEach, describe, expect, it } from "vitest";
import { botInitials, botState, botStatusLine, elapsed, mergeTask, nameTaken, orderTasks } from "../lib/bots";
import type { Bot, BotTask } from "../lib/live";
import { normalizeFor } from "../lib/normalize";
import { parseEvents } from "../lib/events";
import { useStore } from "../state/store";

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
  sessionId: null,
  color: "violet",
  paused: false,
  createdAt: 1,
  updatedAt: 1,
  queued: 0,
  runningTaskId: null,
  runningText: null,
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
    expect(await st.addBotTask("b-demo-reviewer", "   ")).toBeNull();
    expect(await st.addBotTask("b-demo-reviewer", "Check the logout route")).toBeNull();
    const added = useStore.getState().botTasks["b-demo-reviewer"]!.find((x) => x.text === "Check the logout route")!;
    expect(added.status).toBe("queued");
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
      projectId: "p-api",
      color: "blue",
    });
    expect(err).toMatch(/on this Mac/);
    st.openBot("b-demo-docs");
    expect(await st.deleteBot("b-demo-docs")).toBeNull();
    expect(useStore.getState().bots["b-demo-docs"]).toBeUndefined();
    expect(useStore.getState().view).toBe("workspace");
  });

  it("keeps live task updates per bot", () => {
    const st = useStore.getState();
    st.upsertBot(bot({ id: "b-live" }));
    st.upsertBotTask(task("t1", "queued", { botId: "b-live" }));
    st.upsertBotTask(task("t1", "running", { botId: "b-live" }));
    st.upsertBotTask(task("t2", "queued", { botId: "b-live" }));
    expect(useStore.getState().botTasks["b-live"]!.map((x) => `${x.id}:${x.status}`)).toEqual(["t1:running", "t2:queued"]);
  });
});
