import { describe, expect, it } from "vitest";
import type { StoredEvent } from "../lib/claude";
import { parseEvents } from "../lib/events";
import { normalizeFor } from "../lib/normalize";

const msg = (payload: Record<string, unknown>): StoredEvent => ({ sessionId: "s", seq: 1, ts: 1, source: "agentdeck", kind: "user_message", payload });

describe("attachments on a message", () => {
  it("are shown with every agent, and the note for the agent is not", () => {
    for (const provider of ["claude-code", "codex", "gemini-cli", "api"] as const) {
      const out = parseEvents(
        normalizeFor(provider, [msg({ text: "what is this", attachments: ["shot.png", "src/"], note: "\n\nAttached files: …" })], { sessionId: "s", projectPath: "/p" }),
      );
      expect(out[0]).toMatchObject({ ok: true, event: { type: "user_message", payload: { text: "what is this", attachments: ["shot.png", "src/"] } } });
    }
  });

  it("leaves plain messages without an attachment list", () => {
    const out = parseEvents(normalizeFor("claude-code", [msg({ text: "hi", attachments: [] })], { sessionId: "s", projectPath: "/p" }));
    expect(out[0]?.ok && out[0].event.type === "user_message" && out[0].event.payload.attachments).toBeUndefined();
  });
});

describe("imported sessions", () => {
  const ev = (seq: number, kind: string, payload: Record<string, unknown>, ts = 1000 + seq): StoredEvent => ({ sessionId: "s", seq, ts, source: "agentdeck", kind, payload });

  it("show the agent's earlier messages with their own times, then continue", () => {
    const out = parseEvents(
      normalizeFor(
        "claude-code",
        [
          ev(1, "imported", { from: "Claude Code", messages: 2, resumable: true }, 500),
          ev(2, "imported_message", { role: "user", text: "why does login fail?" }, 500),
          ev(3, "imported_message", { role: "assistant", text: "The token expired." }, 600),
        ],
        { sessionId: "s", projectPath: "/p" },
      ),
    );
    expect(out.filter((p) => !p.ok)).toEqual([]);
    const events = out.flatMap((p) => (p.ok ? [p.event] : []));
    expect(events.map((e) => e.type)).toEqual(["notice", "user_message", "assistant_text"]);
    expect(events[0]?.type === "notice" && events[0].payload.text).toContain("continues the same conversation");
    expect(events[2]?.ts).toBe(600);
  });

  it("say when the agent has to start fresh", () => {
    const out = parseEvents(normalizeFor("gemini-cli", [ev(1, "imported", { from: "Gemini CLI", messages: 1, resumable: false })], { sessionId: "s", projectPath: "/p" }));
    expect(out[0]?.ok && out[0].event.type === "notice" && out[0].event.payload.text).toContain("starts a fresh conversation");
  });
});
