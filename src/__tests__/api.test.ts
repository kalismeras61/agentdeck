import { describe, expect, it } from "vitest";
import fixture from "./fixtures/api-session.json";
import type { StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";
import { normalizeFor } from "../lib/normalize";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const of = <T extends AgentEvent["type"]>(events: AgentEvent[], t: T) => events.filter((e): e is Extract<AgentEvent, { type: T }> => e.type === t);

describe("AgentDeck's own agent on a model API (recorded session)", () => {
  const stored = fixture as StoredEvent[];
  const events = ok(normalizeFor("api", stored, { sessionId: stored[0]!.sessionId, projectPath: "/tmp/project" }));

  it("shows prompts and replies", () => {
    expect(of(events, "user_message").map((u) => u.payload.text)).toEqual(["please write the file", "please run the check"]);
    expect(of(events, "assistant_text").map((a) => a.payload.markdown)).toEqual([
      "I'll create the file.",
      "Done. The command printed agentdeck-ok and the file is in place.",
      "I'll run a quick check.",
      "The action was not approved, so I stopped.",
    ]);
  });

  it("asks before writing and running, with a diff preview and session-wide options", () => {
    const [write, run] = of(events, "permission");
    expect(write!.payload).toMatchObject({ tool: "write_file", command: "Create hello.txt", paths: ["hello.txt"], state: "allowed", persistentLabel: "Allow edits for this session" });
    expect(write!.payload.diff).toBe("@@ -0,0 +1,1 @@\n+hello from agentdeck\n");
    expect(run!.payload).toMatchObject({ tool: "run_command", command: "echo agentdeck-ok", risk: "medium", state: "denied" });
  });

  it("records the write as a new file and the denied command as not run", () => {
    expect(of(events, "changeset")[0]!.payload.files).toEqual([{ path: "hello.txt", change: "added", diff: "@@ -0,0 +1,1 @@\n+hello from agentdeck\n" }]);
    const groups = of(events, "tool_group");
    expect(groups[0]!.payload.items[0]).toMatchObject({ kind: "create", status: "ok", target: "hello.txt" });
    expect(groups[1]!.payload.items[0]).toMatchObject({ kind: "command", status: "error", meta: "Denied" });
  });
});
