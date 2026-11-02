import { describe, expect, it } from "vitest";
import { layoutGraph } from "../lib/gitGraph";

describe("layoutGraph", () => {
  it("keeps a straight history in one lane", () => {
    const rows = layoutGraph([
      { hash: "c", parents: ["b"] },
      { hash: "b", parents: ["a"] },
      { hash: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[0]!.segments).toEqual([{ from: 0, to: 0, y1: 1, y2: 2, color: 0 }]);
    expect(rows[2]!.segments).toEqual([{ from: 0, to: 0, y1: 0, y2: 1, color: 0 }]);
  });

  it("opens a lane for a merged branch and closes it where it forked", () => {
    // m merges f2 into main; f2 <- f1 branched off a.
    const rows = layoutGraph([
      { hash: "m", parents: ["b", "f2"] },
      { hash: "f2", parents: ["f1"] },
      { hash: "b", parents: ["a"] },
      { hash: "f1", parents: ["a"] },
      { hash: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 1, 0, 1, 0]);
    // The merge commit draws a line down its lane and one over to the branch's lane.
    expect(rows[0]!.segments).toContainEqual({ from: 0, to: 1, y1: 1, y2: 2, color: 1 });
    // f1's parent is already expected in lane 0, so its line bends back into it.
    expect(rows[3]!.segments).toContainEqual({ from: 1, to: 0, y1: 1, y2: 2, color: 0 });
    expect(rows[4]!.width).toBe(1);
  });

  it("puts separate branch tips side by side", () => {
    const rows = layoutGraph([
      { hash: "x", parents: ["a"] },
      { hash: "y", parents: ["a"] },
      { hash: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 1, 0]);
    expect(Math.max(...rows.map((r) => r.width))).toBe(2);
  });

  it("keeps the main line on the left when a merged branch forked from it", () => {
    // git log --topo-order for: main a-b-c, feature a-f1-f2, m merges both, x on top.
    const rows = layoutGraph([
      { hash: "x", parents: ["m"] },
      { hash: "m", parents: ["c", "f2"] },
      { hash: "f2", parents: ["f1"] },
      { hash: "f1", parents: ["b"] },
      { hash: "c", parents: ["b"] },
      { hash: "b", parents: ["a"] },
      { hash: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 1, 1, 0, 0, 0]);
    // At c, the branch's lane bends into the main line.
    expect(rows[4]!.segments).toContainEqual({ from: 1, to: 0, y1: 0, y2: 2, color: 1 });
  });
});
