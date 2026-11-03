import { describe, expect, it } from "vitest";
import { stageState } from "../lib/workflows";
import type { FlowRow } from "../lib/live";

const flow = (over: Partial<FlowRow>): FlowRow => ({
  id: "f", projectId: "p", name: "w", task: "t",
  planSession: "a", buildSession: "b", reviewSession: "c",
  gatePlan: false, maxReviews: 3, reviews: 0, stage: "plan", status: "active",
  reason: null, plan: null, createdAt: 1, updatedAt: 1, ...over,
});

describe("stageState", () => {
  it("marks stages before the current one done and the rest pending", () => {
    const f = flow({ stage: "review" });
    expect(["a", "b", "c"].map((s) => stageState(f, s))).toEqual(["done", "done", "current"]);
    expect(["a", "b", "c"].map((s) => stageState(flow({ stage: "approve" }), s))).toEqual(["current", "pending", "pending"]);
  });
  it("marks every stage done when the workflow finished, and where it stopped otherwise", () => {
    expect(["a", "b", "c"].map((s) => stageState(flow({ stage: "review", status: "done" }), s))).toEqual(["done", "done", "done"]);
    expect(["a", "b", "c"].map((s) => stageState(flow({ stage: "build", status: "stopped" }), s))).toEqual(["done", "stopped", "pending"]);
  });
  it("ignores sessions that aren't in the workflow", () => {
    expect(stageState(flow({}), "x")).toBeUndefined();
  });
});
