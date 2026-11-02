import { describe, expect, it } from "vitest";
import fixture from "./fixtures/codex-session.json";
import type { StoredEvent } from "../lib/claude";
import { normalizeCodex } from "../lib/codex";
import { parseEvents, type AgentEvent } from "../lib/events";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const of = <T extends AgentEvent["type"]>(events: AgentEvent[], t: T) => events.filter((e): e is Extract<AgentEvent, { type: T }> => e.type === t);

let seq = 0;
const ev = (source: StoredEvent["source"], kind: string, payload: Record<string, unknown>): StoredEvent => ({ sessionId: "s1", seq: ++seq, ts: 1_700_000_000_000 + seq, source, kind, payload });
const ctx = { sessionId: "s1", projectPath: "/Users/ada/code/demo" };

describe("normalizeCodex on a real recorded app-server session", () => {
  const stored = fixture as StoredEvent[];
  const events = ok(normalizeCodex(stored, { sessionId: stored[0]!.sessionId, projectPath: "/tmp/project" }));

  it("renders both prompts, replies, and the start and resume notices", () => {
    expect(of(events, "user_message").map((e) => e.payload.text)).toEqual(["please run the check", "run it again"]);
    expect(of(events, "assistant_text").length).toBe(4);
    const notices = of(events, "notice").map((e) => e.payload.text);
    expect(notices.some((t) => t.startsWith("Codex started"))).toBe(true);
    expect(notices.some((t) => t.startsWith("Codex resumed"))).toBe(true);
  });

  it("records the first approval as allowed and the second as denied, with distinct ids", () => {
    const perms = of(events, "permission");
    expect(perms.map((p) => p.payload.state)).toEqual(["allowed", "denied"]);
    expect(new Set(perms.map((p) => p.payload.requestId)).size).toBe(2);
    expect(perms.every((p) => p.payload.command === "touch marker.txt && echo agentdeck-ok")).toBe(true);
    // Codex offered no session-scoped option here, so none is shown.
    expect(perms.every((p) => !p.payload.supportsPersistentAllow)).toBe(true);
  });

  it("shows the command that ran with its output, and the denied one as not run", () => {
    const groups = of(events, "tool_group");
    expect(groups.map((g) => g.payload.title)).toEqual(["Ran 1 command", "1 step didn't run"]);
    expect(groups[0]!.payload.items[0]!.output).toContain("agentdeck-ok");
    expect(groups[1]!.payload.items[0]!).toMatchObject({ status: "error", meta: "Denied", label: "Run" });
  });
});

describe("normalizeCodex edge cases", () => {
  it("expires a request Codex withdrew before anyone answered", () => {
    const events = ok(
      normalizeCodex(
        [
          ev("codex", "approval", { requestId: "g-1", method: "item/commandExecution/requestApproval", params: { command: "rm -rf build", cwd: "/Users/ada/code/demo", availableDecisions: ["accept", "decline"] } }),
          ev("codex", "serverRequest/resolved", { requestId: "g-1" }),
        ],
        ctx,
      ),
    );
    const [perm] = of(events, "permission");
    expect(perm!.payload.state).toBe("expired");
    expect(perm!.payload.risk).toBe("high");
  });

  it("offers allow-for-session only when Codex lists acceptForSession", () => {
    const events = ok(
      normalizeCodex(
        [ev("codex", "approval", { requestId: "g-2", method: "item/commandExecution/requestApproval", params: { command: "/bin/bash -lc 'npm test'", availableDecisions: ["accept", "acceptForSession", "decline"] } })],
        ctx,
      ),
    );
    const [perm] = of(events, "permission");
    expect(perm!.payload.supportsPersistentAllow).toBe(true);
    expect(perm!.payload.command).toBe("npm test");
    expect(perm!.payload.state).toBe("pending");
  });

  it("turns a file change into a changeset and a failed turn into an error", () => {
    const events = ok(
      normalizeCodex(
        [
          ev("codex", "item/completed", {
            item: { id: "f1", type: "fileChange", status: "completed", changes: [{ path: "/Users/ada/code/demo/src/a.ts", kind: { type: "update" }, diff: "@@ -1,1 +1,1 @@\n-const a = 1;\n+const a = 2;\n" }] },
          }),
          ev("codex", "turn/completed", { turn: { id: "t1", status: "failed", error: { message: "usage limit reached" } } }),
        ],
        ctx,
      ),
    );
    const [cs] = of(events, "changeset");
    expect(cs!.payload.files[0]!.path).toBe("src/a.ts");
    expect(of(events, "error")[0]!.payload.message).toContain("usage limit");
  });
});
