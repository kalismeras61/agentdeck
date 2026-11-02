import { describe, expect, it } from "vitest";
import fixture from "./fixtures/claude-session.json";
import { normalizeClaude, relPath, type StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";

const ctx = { sessionId: "s1", projectPath: "/Users/ada/code/demo" };
let seq = 0;
const ev = (source: StoredEvent["source"], kind: string, payload: Record<string, unknown>): StoredEvent => ({ sessionId: "s1", seq: ++seq, ts: 1_700_000_000_000 + seq, source, kind, payload });
const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  const bad = parsed.filter((p) => !p.ok);
  expect(bad).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const of = <T extends AgentEvent["type"]>(events: AgentEvent[], t: T) => events.filter((e): e is Extract<AgentEvent, { type: T }> => e.type === t);

describe("normalizeClaude on a real recorded session", () => {
  const stored = (fixture as StoredEvent[]).map((e) => ({ ...e }));
  const root = String((stored.find((e) => e.kind === "system/init")!.payload as { cwd: string }).cwd);
  const events = ok(normalizeClaude(stored, { sessionId: stored[0]!.sessionId, projectPath: root }));

  it("renders both prompts, the replies and the resume notice", () => {
    expect(of(events, "user_message").map((e) => e.payload.text)).toEqual(["please write the file", "write it again"]);
    expect(of(events, "assistant_text").length).toBeGreaterThanOrEqual(4);
    const notices = of(events, "notice").map((e) => e.payload.text);
    expect(notices.some((t) => t.startsWith("Claude Code") && t.includes("started"))).toBe(true);
    expect(notices.some((t) => t.includes("resumed"))).toBe(true);
  });

  it("records the first request as allowed and the second as denied", () => {
    expect(of(events, "permission").map((p) => [p.payload.tool, p.payload.state])).toEqual([
      ["Write", "allowed"],
      ["Write", "denied"],
    ]);
    const p = of(events, "permission")[0]!;
    expect(p.payload.paths).toEqual(["hello.txt"]);
    expect(p.payload.risk).toBe("low");
    expect(p.payload.supportsPersistentAllow).toBe(true);
    expect(p.payload.persistentLabel).toBe("Allow edits for this session");
    expect(p.payload.diff).toContain("+hello from agentdeck");
  });

  it("shows the approved write as a new file and the denied one as a failed step", () => {
    const changes = of(events, "changeset");
    expect(changes).toHaveLength(1);
    expect(changes[0]!.payload.files[0]).toMatchObject({ path: "hello.txt", change: "added" });
    const items = of(events, "tool_group").flatMap((g) => g.payload.items);
    expect(items.map((i) => i.status)).toEqual(["ok", "error"]);
    expect(items[1]!.meta).toBe("Denied");
  });

  it("is deterministic: the same log always gives the same events", () => {
    expect(normalizeClaude(stored, { sessionId: "x", projectPath: root })).toEqual(normalizeClaude(stored, { sessionId: "x", projectPath: root }));
  });
});

describe("normalizeClaude edge cases", () => {
  it("turns Edit results into diffs from the CLI's own patch and groups consecutive tools", () => {
    seq = 0;
    const log = [
      ev("agentdeck", "user_message", { text: "fix it" }),
      ev("claude", "assistant", { message: { content: [{ type: "tool_use", id: "t1", name: "Read", input: { file_path: "/Users/ada/code/demo/a.ts" } }] } }),
      ev("claude", "assistant", { message: { content: [{ type: "tool_use", id: "t2", name: "Edit", input: { file_path: "/Users/ada/code/demo/a.ts", old_string: "1", new_string: "2" } }] } }),
      ev("claude", "user", { message: { content: [{ type: "tool_result", tool_use_id: "t1", content: "file body" }] } }),
      ev("claude", "user", {
        message: { content: [{ type: "tool_result", tool_use_id: "t2", content: "ok" }] },
        tool_use_result: { filePath: "/Users/ada/code/demo/a.ts", structuredPatch: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: ["-const x = 1;", "+const x = 2;"] }] },
      }),
      ev("claude", "result/success", { is_error: false, result: "done" }),
    ];
    const events = ok(normalizeClaude(log, ctx));
    const groups = of(events, "tool_group");
    expect(groups).toHaveLength(1);
    expect(groups[0]!.payload.title).toBe("Read 1 file, changed 1 file");
    expect(groups[0]!.payload.items.map((i) => i.target)).toEqual(["a.ts", "a.ts"]);
    const cs = of(events, "changeset")[0]!;
    expect(cs.payload.files[0]).toMatchObject({ path: "a.ts", change: "modified" });
    expect(cs.payload.files[0]!.diff).toBe("@@ -1,1 +1,1 @@\n-const x = 1;\n+const x = 2;\n");
  });

  it("flags risky commands and paths outside the project", () => {
    seq = 0;
    const log = [
      ev("claude", "control_request/can_use_tool", { request_id: "r1", request: { subtype: "can_use_tool", tool_name: "Bash", input: { command: "rm -rf build" }, permission_suggestions: [] } }),
      ev("claude", "control_request/can_use_tool", { request_id: "r2", request: { subtype: "can_use_tool", tool_name: "Write", input: { file_path: "/etc/hosts", content: "x" } } }),
    ];
    const perms = of(ok(normalizeClaude(log, ctx)), "permission");
    expect(perms.map((p) => p.payload.risk)).toEqual(["high", "high"]);
    expect(perms[1]!.payload.paths).toEqual([]);
    expect(perms[1]!.payload.reason).toContain("outside the project");
    expect(perms[0]!.payload.supportsPersistentAllow).toBe(false);
  });

  it("expires unanswered requests when the process ends or the app restarts", () => {
    seq = 0;
    const log = [
      ev("claude", "control_request/can_use_tool", { request_id: "r1", request: { tool_name: "Bash", input: { command: "ls" } } }),
      ev("agentdeck", "app_restart", { message: "AgentDeck closed while this session was working." }),
    ];
    const events = ok(normalizeClaude(log, ctx));
    expect(of(events, "permission")[0]!.payload.state).toBe("expired");
  });

  it("reports unexpected exits with the CLI's stderr, and quiet exits silently", () => {
    seq = 0;
    const crash = ok(normalizeClaude([ev("agentdeck", "process_exit", { code: 1, expected: false, stderrTail: "Invalid API key" })], ctx));
    expect(of(crash, "error")[0]!.payload).toMatchObject({ title: "Claude Code stopped unexpectedly", detail: "Invalid API key" });
    const quiet = ok(normalizeClaude([ev("agentdeck", "process_exit", { code: 0, expected: true })], ctx));
    expect(quiet).toHaveLength(0);
  });

  it("turns TodoWrite into a progress card instead of a tool row", () => {
    seq = 0;
    const log = [
      ev("claude", "assistant", {
        message: { content: [{ type: "tool_use", id: "t", name: "TodoWrite", input: { todos: [{ content: "A", status: "completed" }, { content: "B", status: "in_progress" }, { content: "C", status: "pending" }] } }] },
      }),
    ];
    const events = ok(normalizeClaude(log, ctx));
    expect(of(events, "progress")[0]!.payload.steps.map((s) => s.status)).toEqual(["done", "active", "pending"]);
    expect(of(events, "tool_group")).toHaveLength(0);
  });

  it("ignores subagent traffic, unknown kinds and garbage payloads without throwing", () => {
    seq = 0;
    const log = [
      ev("claude", "assistant", { parent_tool_use_id: "t9", message: { content: [{ type: "text", text: "inside a subagent" }] } }),
      ev("claude", "some_future_kind", { anything: true }),
      ev("claude", "assistant", { message: "not an object" }),
      ev("claude", "user", { message: { content: [{ type: "tool_result", tool_use_id: "unknown", content: 5 }] } }),
    ];
    expect(ok(normalizeClaude(log, ctx))).toHaveLength(0);
  });

  it("keeps paths inside the project relative and refuses escapes", () => {
    expect(relPath("/Users/ada/code/demo/lib/a.dart", "/Users/ada/code/demo")).toBe("lib/a.dart");
    expect(relPath("/Users/ada/code/demo2/x", "/Users/ada/code/demo")).toBeUndefined();
    expect(relPath("../x", "/Users/ada/code/demo")).toBeUndefined();
  });
});

describe("tool group titles", () => {
  it("doesn't claim a change for steps that never ran", () => {
    seq = 0;
    const log = [
      ev("claude", "assistant", { message: { content: [{ type: "tool_use", id: "w", name: "Write", input: { file_path: "/Users/ada/code/demo/x.txt", content: "x" } }] } }),
      ev("claude", "control_request/can_use_tool", { request_id: "r", request: { tool_name: "Write", input: { file_path: "/Users/ada/code/demo/x.txt", content: "x" } } }),
    ];
    const pending = of(ok(normalizeClaude(log, ctx)), "tool_group")[0]!;
    expect(pending.payload.title).toBe("Starting a step");
    log.push(ev("agentdeck", "app_restart", { message: "closed" }));
    const after = of(ok(normalizeClaude(log, ctx)), "tool_group")[0]!;
    expect(after.payload.title).toBe("1 step didn't run");
    expect(after.payload.items[0]!.meta).toBe("Not run");
  });
});
