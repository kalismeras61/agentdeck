import { describe, expect, it } from "vitest";
import { demoTimeRows } from "../data/teamDemo";
import { dayOf, groupBy, hm, lastDays, perDay, sharedIn, sum, toCsv, total, type TimeRow } from "../lib/time";

const H = 3_600_000;
const M = 60_000;
const row = (day: string, branch: string | null, sessionId: string, a: number, w: number, y: number): TimeRow => ({ projectId: "p", sessionId, branch, day, agentMs: a, waitingMs: w, youMs: y });

describe("time", () => {
  it("formats durations", () => {
    expect(hm(0)).toBe("0m");
    expect(hm(20_000)).toBe("20s");
    expect(hm(12 * M)).toBe("12m");
    expect(hm(H + 5 * M)).toBe("1h 05m");
    expect(hm(26 * H)).toBe("26h 00m");
  });

  it("knows the last days, today last", () => {
    const now = new Date(2026, 10, 2, 9).getTime();
    expect(lastDays(3, now)).toEqual(["2026-10-31", "2026-11-01", "2026-11-02"]);
    expect(dayOf(now)).toBe("2026-11-02");
  });

  it("sums by branch and by day", () => {
    const rows = [row("2026-11-10", "main", "a", H, 0, 10 * M), row("2026-11-11", "fix", "b", 30 * M, 5 * M, 0), row("2026-11-11", "main", "c", H, 0, 0)];
    const g = groupBy(rows, (r) => r.branch ?? "");
    expect(g.map((x) => [x.key, total(x.totals)])).toEqual([
      ["main", 2 * H + 10 * M],
      ["fix", 35 * M],
    ]);
    const days = perDay(rows, ["2026-11-09", "2026-11-10", "2026-11-11"]);
    expect(days.map((d) => d.totals.agentMs)).toEqual([0, H, H + 30 * M]);
    expect(sum(rows)).toEqual({ agentMs: 2 * H + 30 * M, waitingMs: 5 * M, youMs: 10 * M });
  });

  it("writes a timesheet", () => {
    const csv = toCsv([row("2026-11-11", "fix/a", "s2", 90 * M, 0, 15 * M), row("2026-11-10", null, "s1", H, 6 * M, 0)], {
      project: () => "shop, app",
      session: (id) => (id === "s1" ? 'Say "hi"' : "Fix"),
    });
    expect(csv.split("\n")).toEqual([
      "date,project,branch,session,agent_hours,waiting_hours,your_hours",
      '2026-11-10,"shop, app",,"Say ""hi""",1.00,0.10,0.00',
      '2026-11-11,"shop, app",fix/a,Fix,1.50,0.00,0.25',
      "",
    ]);
  });

  it("keeps teammates' rows in range and the demo stays sensible", () => {
    const t = [{ branch: "x", day: "2026-11-11", agentMs: 1, waitingMs: 0, youMs: 0 }, { branch: "x", day: "2026-11-01", agentMs: 1, waitingMs: 0, youMs: 0 }];
    expect(sharedIn(t, ["2026-11-11"])).toHaveLength(1);
    expect(sharedIn(undefined, ["2026-11-11"])).toEqual([]);
    const demo = demoTimeRows(new Date(2026, 10, 11, 15).getTime());
    expect(demo.length).toBeGreaterThan(20);
    expect(demo.every((r) => r.agentMs > 0 && r.youMs > 0 && r.agentMs < 24 * H)).toBe(true);
  });
});
