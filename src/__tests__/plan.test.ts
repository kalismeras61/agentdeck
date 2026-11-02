import { describe, expect, it } from "vitest";
import fixture from "./fixtures/claude-plan-session.json";
import type { StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";
import { normalizeFor } from "../lib/normalize";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const of = <T extends AgentEvent["type"]>(events: AgentEvent[], t: T) => events.filter((e): e is Extract<AgentEvent, { type: T }> => e.type === t);

describe("plan mode on a real recorded Claude Code session", () => {
  const stored = fixture as StoredEvent[];
  const events = ok(normalizeFor("claude-code", stored, { sessionId: stored[0]!.sessionId, projectPath: "/tmp/project" }));

  it("refuses a change in plan mode without asking, and says why", () => {
    const [blocked] = of(events, "permission");
    expect(blocked!.payload).toMatchObject({ tool: "Write", state: "denied", autoReason: "Refused by plan mode" });
    const [group] = of(events, "tool_group");
    expect(group!.payload.items[0]).toMatchObject({ label: "Held back", meta: "Plan mode", status: "error" });
  });

  it("turns each ExitPlanMode into a plan card: first sent back, then approved", () => {
    const plans = of(events, "plan");
    expect(plans.map((p) => p.payload.state)).toEqual(["kept", "approved"]);
    expect(plans[0]!.payload.markdown).toContain("Apply the discount before tax");
    expect(plans[0]!.payload.filePath).toMatch(/^~\/\.claude\/plans\/.+\.md$/);
  });

  it("hides plan bookkeeping (plan file write, tool loading) from the steps", () => {
    const targets = of(events, "tool_group").flatMap((g) => g.payload.items.map((i) => i.target ?? ""));
    expect(targets.some((t) => t.includes(".claude/plans"))).toBe(false);
    expect(of(events, "tool_group").some((g) => g.payload.items.some((i) => /ToolSearch|ExitPlanMode/.test(i.label)))).toBe(false);
  });

  it("announces leaving plan mode, then the approved write goes through", () => {
    expect(of(events, "notice").some((n) => n.payload.text.startsWith("Plan mode off"))).toBe(true);
    expect(of(events, "changeset")[0]!.payload.files[0]!.path).toBe("hello.txt");
  });

  it("shows the start notice once, not on every turn", () => {
    const notices = of(events, "notice").map((n) => n.payload.text);
    expect(notices.filter((t) => t.startsWith("Claude Code")).length).toBe(1);
  });
});

describe("hand-off between sessions", () => {
  let seq = 0;
  const ev = (sessionId: string, source: StoredEvent["source"], kind: string, payload: Record<string, unknown>): StoredEvent => ({ sessionId, seq: ++seq, ts: 1_700_000_000_000 + seq, source, kind, payload });

  it("marks the plan as handed off and links the new session on the source side", () => {
    const stored = fixture as StoredEvent[];
    const sid = stored[0]!.sessionId;
    // Cut the log at the second plan request and hand it to Codex instead of approving.
    const cut = stored.findIndex((e, i) => e.kind === "control_request/can_use_tool" && i > 20);
    const rid = String(stored[cut]!.payload.request_id);
    const log = [
      ...stored.slice(0, cut + 1),
      { ...ev(sid, "agentdeck", "handoff_out", { toSessionId: "s-codex", toProvider: "codex", plan: "## Plan", planRequestId: rid }), ts: stored[cut]!.ts + 1 },
      { ...ev(sid, "agentdeck", "permission_decision", { requestId: rid, decision: "deny" }), ts: stored[cut]!.ts + 2 },
    ];
    const events = ok(normalizeFor("claude-code", log, { sessionId: sid, projectPath: "/tmp/project" }));
    const plans = of(events, "plan");
    expect(plans[1]!.payload).toMatchObject({ state: "handed_off", handedTo: { provider: "codex", sessionId: "s-codex" } });
    // The plan card carries the link, so no separate hand-off row.
    expect(of(events, "handoff")).toEqual([]);
  });

  it("shows the incoming plan as a card instead of the long generated prompt", () => {
    const prompt = "Implement this plan. Claude Code wrote it in another AgentDeck session …\n\n## Plan";
    const events = ok(
      normalizeFor(
        "codex",
        [
          ev("s-codex", "agentdeck", "handoff_in", { fromSessionId: "s-claude", fromProvider: "claude-code", fromTitle: "Fix the discount", plan: "## Plan", prompt }),
          ev("s-codex", "agentdeck", "user_message", { text: prompt }),
          ev("s-codex", "agentdeck", "user_message", { text: "also update the README" }),
        ],
        { sessionId: "s-codex", projectPath: "/tmp/project" },
      ),
    );
    expect(of(events, "handoff")[0]!.payload).toMatchObject({ direction: "in", provider: "claude-code", title: "Fix the discount" });
    expect(of(events, "user_message").map((u) => u.payload.text)).toEqual(["also update the README"]);
  });
});
