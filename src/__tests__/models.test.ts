import { describe, expect, it } from "vitest";
import { modelName } from "../lib/models";

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
