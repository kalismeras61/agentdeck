import { afterEach, describe, expect, it } from "vitest";
import { useI18n } from "../lib/i18n";
import { asOfText, limitLevel, liveWindows, percentText, resetShort, useLimits, windowDetail, windowLabel, windowSummary, type LimitWindow, type PlanLimits } from "../lib/limits";
import { sampleLimits } from "../data/demo";

// Local time, so the expected strings don't depend on the test machine's time zone.
const NOW = new Date(2026, 10, 3, 14, 5).getTime(); // Tue 3 Nov 2026, 14:05
const MIN = 60_000;

const five: LimitWindow = { kind: "five_hour", usedPercent: 34.4, resetsAt: new Date(2026, 10, 3, 18, 20).getTime(), status: "allowed" };
const week: LimitWindow = { kind: "weekly", usedPercent: 61, resetsAt: new Date(2026, 10, 6, 9, 0).getTime() };

afterEach(() => useI18n.setState({ locale: "en", catalog: {} }));

/** ICU versions differ in the spaces they put in dates and times (U+202F before AM/PM, U+00A0), and in
 * small wording; compare with ordinary spaces so the tests don't depend on the Node build. */
const plain = (s: string) => s.replace(/[\u00a0\u202f\u2009]/g, " ");

describe("plan limit formatting", () => {
  it("writes the line under the composer", () => {
    expect(windowLabel(five)).toBe("5h");
    expect(windowLabel(week)).toBe("Week");
    expect(windowLabel({ kind: "window_120", label: "2h", usedPercent: 1 })).toBe("2h");
    expect(plain(windowSummary(five, NOW))).toBe(plain("34% · resets 6:20 PM"));
    expect(plain(windowSummary(week, NOW))).toBe(plain("61% · resets Fri"));
    expect(plain(windowSummary({ ...week, resetsAt: new Date(2026, 10, 12, 9, 0).getTime() }, NOW))).toBe(plain("61% · resets Nov 12"));
    expect(plain(windowSummary({ kind: "weekly", usedPercent: 5 }, NOW))).toBe(plain("5%"));
  });

  it("follows the interface language", () => {
    useI18n.setState({ locale: "tr" });
    expect(plain(percentText(34))).toBe(plain("%34"));
    expect(plain(resetShort(five.resetsAt!, NOW))).toBe(plain("18:20"));
    expect(plain(resetShort(week.resetsAt!, NOW))).toBe(plain("Cum"));
    useI18n.setState({ locale: "de" });
    expect(plain(resetShort(five.resetsAt!, NOW))).toBe(plain("18:20"));
    expect(plain(percentText(61))).toBe(plain("61\u00a0%"));
  });

  it("explains the exact reset and when the numbers were read", () => {
    const detail = plain(windowDetail(five, NOW - 3 * MIN, NOW));
    expect(detail).toMatch(/^5-hour limit: 34% used\. Resets .*November 3.*6:20 PM\. As of 2:02 PM\.$/);
    expect(windowDetail({ ...five, status: "rejected", usedPercent: 100 }, NOW, NOW)).toMatch(/^5-hour limit: limit reached\. /);
    // Saved from an earlier day: the date is part of it.
    expect(plain(asOfText(new Date(2026, 10, 1, 9, 30).getTime(), NOW))).toBe(plain("Nov 1, 9:30 AM"));
  });

  it("warns at 80% and errors when the limit is reached", () => {
    expect(limitLevel(five)).toBe("ok");
    expect(limitLevel({ ...five, usedPercent: 80 })).toBe("warning");
    expect(limitLevel({ ...five, status: "allowed_warning" })).toBe("warning");
    expect(limitLevel({ ...five, usedPercent: 100 })).toBe("error");
    expect(limitLevel({ ...five, usedPercent: 12, status: "rejected" })).toBe("error");
  });

  it("hides windows that have reset, and nothing for agents without limits", () => {
    const l: PlanLimits = { provider: "codex", updatedAt: NOW, windows: [{ ...five, resetsAt: NOW - 1 }, week] };
    expect(liveWindows(l, NOW)).toEqual([week]);
    expect(liveWindows(undefined, NOW)).toEqual([]);
    expect(sampleLimits("gemini-cli", NOW)).toBeUndefined();
    expect(liveWindows(sampleLimits("claude-code", NOW), NOW)).toHaveLength(2);
  });

  it("keeps the newest snapshot per agent", () => {
    const put = useLimits.getState().put;
    put({ provider: "claude-code", updatedAt: 200, windows: [five] });
    put({ provider: "claude-code", updatedAt: 100, windows: [week] });
    expect(useLimits.getState().byProvider["claude-code"]!.windows).toEqual([five]);
  });
});
