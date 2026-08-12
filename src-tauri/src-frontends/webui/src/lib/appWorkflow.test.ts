import { describe, expect, it } from "vitest";
import { completedSuccessfully, selectAvailableItem } from "./appWorkflow";

describe("selectAvailableItem", () => {
  const preview = { id: "preview" };

  it("does not invent a native selection when no program is available", () => {
    expect(selectAvailableItem([], [], "missing", null)).toBeNull();
  });

  it("allows an explicit fallback only for preview mode", () => {
    expect(selectAvailableItem([], [], "missing", preview)).toBe(preview);
  });
});

describe("completedSuccessfully", () => {
  it("accepts only a completed backend job", () => {
    expect(completedSuccessfully(null)).toBe(false);
    expect(completedSuccessfully({ phase: "failed" })).toBe(false);
    expect(completedSuccessfully({ phase: "completed" })).toBe(true);
  });
});
