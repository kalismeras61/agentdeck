import { describe, expect, it } from "vitest";
import { demoSessions } from "../data/demo";
import { diffStats, parseEvent, parseEvents, parseUnifiedDiff } from "../lib/events";

const base = { sessionId: "s1", provider: "claude-code" as const };

describe("event validation", () => {
  it("accepts every demo event except the deliberately malformed one", () => {
    const all = demoSessions.flatMap((s) => parseEvents(s.raw));
    const bad = all.filter((p) => !p.ok);
    expect(bad).toHaveLength(1);
    expect(bad[0]).toMatchObject({ ok: false, eventId: "s-ratelimit-bad" });
  });

  it("rejects script and remote image sources", () => {
    for (const src of ["javascript:alert(1)", "https://evil.example/x.png", "data:text/html,<script>"]) {
      const p = parseEvent({ ...base, eventId: "i", ts: 1, type: "image", payload: { src, alt: "", width: 1, height: 1 } });
      expect(p.ok).toBe(false);
    }
  });

  it("cuts overlong text instead of dropping the event", () => {
    const target = `bash -lc '${"x".repeat(3000)}'`;
    const p = parseEvent({
      ...base,
      eventId: "t",
      ts: 1,
      type: "tool_group",
      payload: { title: "Ran a command", items: [{ id: "1", kind: "command", label: "Run", target, status: "ok" }] },
    });
    expect(p.ok).toBe(true);
    if (p.ok && p.event.type === "tool_group") {
      const t = p.event.payload.items[0]!.target!;
      expect(t.length).toBe(1024);
      expect(t.endsWith("…")).toBe(true);
    }
  });

  it("rejects file paths that escape the project", () => {
    const p = parseEvent({ ...base, eventId: "c", ts: 1, type: "changeset", payload: { files: [{ path: "../../etc/passwd", change: "modified", diff: "" }] } });
    expect(p.ok).toBe(false);
  });

  it("turns unknown event types and garbage into explicit failures instead of throwing", () => {
    expect(parseEvent(null).ok).toBe(false);
    expect(parseEvent("text").ok).toBe(false);
    expect(parseEvent({ ...base, eventId: "x", ts: 1, type: "nope", payload: {} }).ok).toBe(false);
  });

  it("drops duplicate events and orders by timestamp", () => {
    const e = (id: string, ts: number) => ({ ...base, eventId: id, ts, type: "assistant_text", payload: { markdown: id } });
    const out = parseEvents([e("b", 2), e("a", 1), e("b", 2), e("c", 3)]);
    expect(out.map((p) => (p.ok ? p.event.eventId : "?"))).toEqual(["a", "b", "c"]);
  });
});

describe("diff helpers", () => {
  const diff = "@@ -1,3 +1,3 @@\n keep\n-old\n+new\n+added\n";
  it("counts additions and deletions", () => {
    expect(diffStats(diff)).toEqual({ additions: 2, deletions: 1 });
  });
  it("tracks old and new line numbers", () => {
    const lines = parseUnifiedDiff(diff);
    expect(lines.map((l) => [l.kind, l.oldNo, l.newNo])).toEqual([
      ["hunk", undefined, undefined],
      ["ctx", 1, 1],
      ["del", 2, undefined],
      ["add", undefined, 2],
      ["add", undefined, 3],
    ]);
  });
});

describe("milestone 2 helpers", () => {
  it("rebuilds both sides of a diff", async () => {
    const { sidesFromDiff } = await import("../lib/events");
    expect(sidesFromDiff("@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n")).toEqual({ original: "keep\nold\ntail", modified: "keep\nnew\ntail" });
  });

  it("validates progress cards and rejects unknown step states", () => {
    const ok = parseEvent({ ...base, eventId: "p1", ts: 1, type: "progress", payload: { title: "Plan", steps: [{ label: "a", status: "done" }] } });
    const bad = parseEvent({ ...base, eventId: "p2", ts: 1, type: "progress", payload: { title: "Plan", steps: [{ label: "a", status: "maybe" }] } });
    expect(ok.ok).toBe(true);
    expect(bad.ok).toBe(false);
  });

  it("accepts every gallery sample except the five rejected on purpose", async () => {
    const { gallerySections } = await import("../data/gallery");
    const parsed = gallerySections.flatMap((s) => parseEvents(s.events));
    expect(parsed.filter((p) => !p.ok)).toHaveLength(5);
  });
});
