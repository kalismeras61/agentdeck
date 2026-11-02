import { describe, expect, it } from "vitest";
import fixture from "./fixtures/relay-session.json";
import type { StoredEvent } from "../lib/claude";
import { parseEvents, type AgentEvent } from "../lib/events";
import { normalizeFor } from "../lib/normalize";

const ok = (raw: unknown[]) => {
  const parsed = parseEvents(raw);
  expect(parsed.filter((p) => !p.ok)).toEqual([]);
  return parsed.flatMap((p) => (p.ok ? [p.event] : []));
};
const relays = (events: AgentEvent[]) => events.flatMap((e) => (e.type === "relay" ? [e.payload] : []));

describe("relay between real Claude Code and Codex sessions", () => {
  const a = fixture.a as StoredEvent[];
  const b = fixture.b as StoredEvent[];
  const ea = ok(normalizeFor("claude-code", a, { sessionId: a[0]!.sessionId, projectPath: "/tmp/project" }));
  const eb = ok(normalizeFor("codex", b, { sessionId: b[0]!.sessionId, projectPath: "/tmp/project" }));

  it("shows the generated prompts as relay cards, never as user messages", () => {
    expect(ea.filter((e) => e.type === "user_message")).toEqual([]);
    expect(eb.filter((e) => e.type === "user_message")).toEqual([]);
  });

  it("records who started, each message with its round, and how it ended", () => {
    const ra = relays(ea);
    const rb = relays(eb);
    expect(ra[0]).toMatchObject({ kind: "started", starts: true, peerProvider: "codex", maxRounds: 6 });
    expect(rb[0]).toMatchObject({ kind: "started", starts: false, peerProvider: "claude-code" });
    expect(rb.find((r) => r.kind === "message")).toMatchObject({ fromProvider: "claude-code", round: 1 });
    expect(ra.find((r) => r.kind === "message")).toMatchObject({ fromProvider: "codex", round: 2 });
    for (const r of [ra, rb]) expect(r[r.length - 1]).toMatchObject({ kind: "ended", status: "done", rounds: 2 });
  });

  it("keeps each agent's own replies as its assistant text", () => {
    const text = (es: AgentEvent[]) => es.flatMap((e) => (e.type === "assistant_text" ? [e.payload.markdown] : []));
    expect(text(ea).some((t) => t.startsWith("Plan: apply the discount"))).toBe(true);
    expect(text(ea).some((t) => t.includes("DONE"))).toBe(true);
    expect(text(eb).some((t) => t.startsWith("Implemented it"))).toBe(true);
  });
});
