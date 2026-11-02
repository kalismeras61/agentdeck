import { describe, expect, it } from "vitest";
import fixture from "./fixtures/flow-session.json";
import type { StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";
import { normalizeFor } from "../lib/normalize";
import { agentInfo, BUILT_IN, lineup } from "../lib/workflows";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const flows = (events: AgentEvent[]) => events.flatMap((e) => (e.type === "flow" ? [e.payload] : []));

/** Recorded from real Claude Code (plan, review) and Codex (build) CLIs against the local mocks. */
describe("workflow: Claude Code plans, Codex builds, Claude Code reviews", () => {
  const plan = fixture.plan as StoredEvent[];
  const build = fixture.build as StoredEvent[];
  const review = fixture.review as StoredEvent[];
  const ctx = (s: StoredEvent[]) => ({ sessionId: s[0]!.sessionId, projectPath: "/tmp/project" });
  const ep = ok(normalizeFor("claude-code", plan, ctx(plan)));
  const eb = ok(normalizeFor("codex", build, ctx(build)));
  const er = ok(normalizeFor("claude-code", review, ctx(review)));

  it("shows the generated prompts as workflow cards, never as user messages", () => {
    for (const es of [ep, eb, er]) expect(es.filter((e) => e.type === "user_message")).toEqual([]);
  });

  it("tells every session the line-up and its own stage", () => {
    const started = [ep, eb, er].map((es) => flows(es)[0]);
    expect(started.map((s) => s?.kind === "started" && s.stage)).toEqual(["plan", "build", "review"]);
    const s0 = started[0]!;
    expect(s0.kind === "started" && s0.stages.map((x) => `${x.stage}:${x.name}`)).toEqual(["plan:Claude Code", "build:Codex", "review:Claude Code"]);
  });

  it("passes the plan to the builder and marks the plan card as taken, with a link", () => {
    const card = ep.find((e) => e.type === "plan");
    expect(card?.type === "plan" && card.payload.state).toBe("workflow");
    expect(card?.type === "plan" && card.payload.handedTo).toMatchObject({ provider: "codex", sessionId: build[0]!.sessionId });
    const inputs = flows(eb).filter((f) => f.kind === "input");
    expect(inputs.map((i) => i.kind === "input" && i.label)).toEqual(["Plan from Claude Code", "Review notes from Claude Code"]);
    expect(inputs[0]?.kind === "input" && inputs[0].markdown).toContain("Apply the discount before tax");
  });

  it("sends the builder's reports to the reviewer and ends on approval", () => {
    const inputs = flows(er).filter((f) => f.kind === "input");
    expect(inputs.map((i) => i.kind === "input" && i.label)).toEqual(["Report from Codex", "Report from Codex"]);
    for (const es of [ep, eb, er]) expect(flows(es).at(-1)).toMatchObject({ kind: "ended", status: "done", reason: "Claude Code approved the work" });
    const text = er.flatMap((e) => (e.type === "assistant_text" ? [e.payload.markdown] : []));
    expect(text.at(-1)).toContain("APPROVED");
  });
});

describe("workflow templates", () => {
  it("names stages by agent, and models by their profile", () => {
    const profiles = [{ id: "ds", name: "DeepSeek", kind: "anthropic" as const, baseUrl: "https://api.deepseek.com/anthropic", model: "deepseek-v4-pro", smallModel: "", createdAt: 0 }];
    expect(lineup(BUILT_IN[0]!, profiles)).toBe("Claude Code → Codex → Claude Code");
    expect(lineup({ planAgent: "", buildAgent: "profile:ds", reviewAgent: "codex" }, profiles)).toBe("DeepSeek → Codex");
    expect(agentInfo("profile:ds", profiles)).toEqual({ name: "DeepSeek", provider: "claude-code" });
    expect(agentInfo("profile:gone", profiles)?.name).toBe("A deleted model");
  });
});
