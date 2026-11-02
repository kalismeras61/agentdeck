import { describe, expect, it } from "vitest";
import fixture from "./fixtures/gemini-session.json";
import type { StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";
import { normalizeFor } from "../lib/normalize";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const of = <T extends AgentEvent["type"]>(events: AgentEvent[], t: T) => events.filter((e): e is Extract<AgentEvent, { type: T }> => e.type === t);

describe("normalizeAcp on a real recorded Gemini CLI session", () => {
  const stored = fixture as StoredEvent[];
  const root = String((stored.find((e) => e.kind === "update/tool_call")!.payload as { locations: { path: string }[] }).locations[0]!.path).replace(/\/hello\.txt$/, "");
  const events = ok(normalizeFor("gemini-cli", stored, { sessionId: stored[0]!.sessionId, projectPath: root }));

  it("shows start and resume, both prompts and the replies", () => {
    const notices = of(events, "notice").map((n) => n.payload.text);
    expect(notices[0]).toMatch(/^Gemini CLI 0\.\d+\.\d+ started\.$/);
    expect(notices).toContain("Gemini CLI resumed this conversation.");
    expect(of(events, "user_message").map((u) => u.payload.text)).toEqual(["please write the file", "write it again"]);
    expect(of(events, "assistant_text").map((a) => a.payload.markdown)).toEqual([
      "I'll create the file.",
      "Done. The command printed agentdeck-ok and the file is in place.",
      "I'll create the file.",
      "The action was not approved, so I stopped.",
    ]);
  });

  it("turns approvals into permission cards with Gemini's own session option", () => {
    const perms = of(events, "permission");
    expect(perms.map((p) => p.payload.state)).toEqual(["allowed", "denied"]);
    expect(perms[0]!.payload).toMatchObject({ tool: "edit", paths: ["hello.txt"], risk: "low", supportsPersistentAllow: true, persistentLabel: "Allow for this session" });
    expect(perms[0]!.payload.diff).toContain("+hello from agentdeck");
  });

  it("records the allowed write as a new file and the denied one as not run", () => {
    expect(of(events, "changeset").map((c) => c.payload.files)).toEqual([[{ path: "hello.txt", change: "added", diff: "@@ -0,0 +1,1 @@\n+hello from agentdeck\n" }]]);
    const groups = of(events, "tool_group");
    expect(groups[0]!.payload.items[0]).toMatchObject({ kind: "create", status: "ok" });
    expect(groups[1]!.payload.items[0]).toMatchObject({ status: "error", meta: "Denied" });
  });
});
