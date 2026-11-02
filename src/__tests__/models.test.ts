import { describe, expect, it } from "vitest";
import { effortName, effortsFor, modelName } from "../lib/models";

describe("modelName", () => {
  const models = [
    { id: "default", label: "Default (recommended)" },
    { id: "gpt-6.1-sol", label: "GPT-6.1-Sol", description: "Workhorse" },
  ];
  it("uses the agent's own label, and the raw id for models it didn't list", () => {
    expect(modelName(models, "default")).toBe("Default model");
    expect(modelName(models, "gpt-6.1-sol")).toBe("GPT-6.1-Sol");
    expect(modelName(models, "my-finetune")).toBe("my-finetune");
    expect(modelName(undefined, "opus")).toBe("opus");
  });
});

describe("efforts", () => {
  const models = [
    { id: "default", label: "Default", efforts: ["low", "medium", "high", "xhigh", "max"] },
    { id: "haiku", label: "Haiku" },
  ];
  it("lists the selected model's levels, none for a model without them or an unknown id", () => {
    expect(effortsFor(models, "default")).toHaveLength(5);
    expect(effortsFor(models, "")).toHaveLength(5);
    expect(effortsFor(models, "haiku")).toEqual([]);
    expect(effortsFor(models, "my-finetune")).toEqual([]);
    expect(effortsFor(undefined, "default")).toEqual([]);
  });
  it("names known levels and shows others as the agent spells them", () => {
    expect(effortName("xhigh")).toBe("Extra high");
    expect(effortName("turbo")).toBe("turbo");
  });
});
