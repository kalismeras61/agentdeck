import { describe, expect, it } from "vitest";
import { appCommandsFor, isRemoteControl } from "../lib/commands";
import type { StoredEvent } from "../lib/claude";
import { normalizeFor } from "../lib/normalize";

const ev = (seq: number, kind: string, payload: Record<string, unknown>): StoredEvent => ({ sessionId: "s1", seq, ts: seq * 1000, source: "agentdeck", kind, payload });

describe("Remote Control", () => {
  it("is offered in Claude Code sessions only", () => {
    expect(appCommandsFor("claude-code").some((c) => c.name === "remote-control")).toBe(true);
    for (const p of ["codex", "gemini-cli", "antigravity"]) expect(appCommandsFor(p).some((c) => c.name === "remote-control")).toBe(false);
    expect(appCommandsFor("codex").some((c) => c.name === "plan")).toBe(true);
  });

  it("is started by typing /remote-control or /rc on its own", () => {
    for (const s of ["/remote-control", " /rc ", "/RC"]) expect(isRemoteControl(s)).toBe(true);
    for (const s of ["/rc please", "remote-control", "/remote", "tell me about /rc"]) expect(isRemoteControl(s)).toBe(false);
  });

  it("shows when it started and ended, with what was said there in between", () => {
    const items = normalizeFor(
      "claude-code",
      [
        ev(1, "remote_control", { state: "on" }),
        ev(2, "imported_message", { role: "user", text: "From my phone: add a test" }),
        ev(3, "imported_message", { role: "assistant", text: "Added the test." }),
        ev(4, "remote_control", { state: "off", messages: 2 }),
        ev(5, "remote_control", { state: "on" }),
        ev(6, "remote_control", { state: "off", messages: 0 }),
      ],
      { sessionId: "s1", projectPath: "/p" },
    ) as { type: string; payload: { text?: string; markdown?: string } }[];
    expect(items.map((i) => i.type)).toEqual(["notice", "user_message", "assistant_text", "notice", "notice", "notice"]);
    expect(items[0]!.payload.text).toMatch(/^Remote Control is on/);
    expect(items[3]!.payload.text).toBe("Remote Control ended. The 2 messages above came from there.");
    expect(items[5]!.payload.text).toBe("Remote Control ended.");
  });
});
