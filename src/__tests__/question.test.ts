import { describe, expect, it } from "vitest";
import { normalizeClaude, type StoredEvent } from "../lib/claude";
import { normalizeCodex } from "../lib/codex";
import { parseEvents, type AgentEvent } from "../lib/events";

const ctx = { sessionId: "s1", projectPath: "/Users/ada/code/demo" };
let seq = 0;
const ev = (source: StoredEvent["source"], kind: string, payload: Record<string, unknown>): StoredEvent => ({
  sessionId: "s1",
  seq: ++seq,
  ts: 1_700_000_000_000 + seq,
  source,
  kind,
  payload,
});
const questions = (raw: unknown[]) =>
  parseEvents(raw).flatMap((p) => {
    expect(p.ok).toBe(true);
    return p.ok && p.event.type === "question" ? [p.event as Extract<AgentEvent, { type: "question" }>] : [];
  });

describe("agents' questions", () => {
  it("shows Claude Code's AskUserQuestion as a question card, then what you answered", () => {
    const ask = ev("claude", "control_request/can_use_tool", {
      request_id: "r1",
      request: {
        tool_name: "AskUserQuestion",
        input: {
          questions: [
            {
              question: "Which state library should the app use?",
              header: "State",
              multiSelect: false,
              options: [
                { label: "Riverpod", description: "Already used in two screens" },
                { label: "Bloc", description: "More boilerplate, explicit events" },
              ],
            },
          ],
        },
      },
    });
    const [pending] = questions(normalizeClaude([ask], ctx));
    expect(pending!.payload).toMatchObject({ requestId: "r1", state: "pending" });
    expect(pending!.payload.questions[0]).toMatchObject({ id: "Which state library should the app use?", header: "State", multiSelect: false, other: true });
    expect(pending!.payload.questions[0]!.options.map((o) => o.label)).toEqual(["Riverpod", "Bloc"]);

    const answered = ev("agentdeck", "question_answered", { requestId: "r1", answers: { "Which state library should the app use?": ["Riverpod"] }, skipped: false });
    const [done] = questions(normalizeClaude([ask, answered], ctx));
    expect(done!.payload.state).toBe("answered");
    expect(done!.payload.answers).toEqual({ "Which state library should the app use?": ["Riverpod"] });
  });

  it("shows Codex's request_user_input, free text when it has no options, skipped when you skip", () => {
    const ask = ev("codex", "user_input", {
      requestId: "c1",
      params: { questions: [{ id: "env", header: "Target", question: "Which environment should I deploy to?", options: null, isOther: false, isSecret: false }] },
    });
    const [q] = questions(normalizeCodex([ask], ctx));
    expect(q!.payload.questions[0]).toMatchObject({ id: "env", other: true, options: [] });
    const [skipped] = questions(normalizeCodex([ask, ev("agentdeck", "question_answered", { requestId: "c1", answers: null, skipped: true })], ctx));
    expect(skipped!.payload.state).toBe("skipped");
  });
});
