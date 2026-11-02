import { describe, expect, it } from "vitest";
import { normalizeClaude, type StoredEvent } from "../lib/claude";
import { parseEvents } from "../lib/events";
import { screenshotPath } from "../lib/images";

const ctx = { sessionId: "s1", projectPath: "/Users/ada/code/demo" };
let seq = 0;
const ev = (source: StoredEvent["source"], kind: string, payload: Record<string, unknown>): StoredEvent => ({ sessionId: "s1", seq: ++seq, ts: 1_700_000_000_000 + seq, source, kind, payload });

describe("images the agents make or look at", () => {
  it("finds the file a screenshot command writes", () => {
    expect(screenshotPath("xcrun simctl io booted screenshot shots/home.png", "/p")).toBe("/p/shots/home.png");
    expect(screenshotPath("screencapture -x /tmp/screen.jpg", "/p")).toBe("/tmp/screen.jpg");
    expect(screenshotPath("npx playwright screenshot http://localhost:3000 './out/landing page.png'", "/p")).toBe("/p/out/landing page.png");
    expect(screenshotPath("adb exec-out screencap -p > ./device.png", "/p")).toBe("/p/device.png");
    expect(screenshotPath("cp logo.png dist/", "/p")).toBeNull();
  });

  it("shows the picture Claude Code read, and the screenshot it saved", () => {
    const events = [
      ev("claude", "assistant", { message: { content: [{ type: "tool_use", id: "r1", name: "Read", input: { file_path: "/Users/ada/code/demo/shot.png" } }] } }),
      ev("claude", "user", { message: { content: [{ type: "tool_result", tool_use_id: "r1", content: [{ type: "image", source: { type: "base64", media_type: "image/png", data: "iVBORw0KGgo=" } }] }] } }),
      ev("claude", "assistant", { message: { content: [{ type: "tool_use", id: "b1", name: "Bash", input: { command: "xcrun simctl io booted screenshot after.png" } }] } }),
      ev("claude", "user", { message: { content: [{ type: "tool_result", tool_use_id: "b1", content: "Wrote screenshot to: after.png" }] } }),
    ];
    const images = parseEvents(normalizeClaude(events, ctx)).flatMap((p) => (p.ok && p.event.type === "image" ? [p.event.payload] : []));
    expect(images).toEqual([
      expect.objectContaining({ src: "data:image/png;base64,iVBORw0KGgo=", caption: "shot.png" }),
      expect.objectContaining({ path: "/Users/ada/code/demo/after.png", caption: "after.png" }),
    ]);
  });
});
