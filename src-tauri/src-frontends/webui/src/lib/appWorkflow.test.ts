import { describe, expect, it } from "vitest";
import { completedSuccessfully, selectAvailableItem, shouldFinalizeSkippedCleanupLocally, toggleAllSelectableIds } from "./appWorkflow";

describe("selectAvailableItem", () => {
  it("does not show details for an item hidden by filters", () => {
    expect(selectAvailableItem([], "missing")).toBeNull();
  });

  it("keeps the selected visible item or falls back to the first visible item", () => {
    const visible = [{ id: "first" }, { id: "selected" }];
    expect(selectAvailableItem(visible, "selected")).toBe(visible[1]);
    expect(selectAvailableItem(visible, "hidden")).toBe(visible[0]);
  });
});

describe("completedSuccessfully", () => {
  it("accepts only a completed backend job", () => {
    expect(completedSuccessfully(null)).toBe(false);
    expect(completedSuccessfully({ phase: "failed" })).toBe(false);
    expect(completedSuccessfully({ phase: "completed" })).toBe(true);
  });
});

describe("shouldFinalizeSkippedCleanupLocally", () => {
  it("finishes locally when the review has no backend job", () => {
    expect(shouldFinalizeSkippedCleanupLocally(true, null)).toBe(true);
    expect(shouldFinalizeSkippedCleanupLocally(true, undefined)).toBe(true);
  });

  it("uses the backend for a native uninstall job", () => {
    expect(shouldFinalizeSkippedCleanupLocally(true, "job-1")).toBe(false);
  });

  it("keeps browser preview cleanup local", () => {
    expect(shouldFinalizeSkippedCleanupLocally(false, "preview-job")).toBe(true);
  });
});

describe("toggleAllSelectableIds", () => {
  const traces = [
    { id: "high" },
    { id: "low" },
    { id: "critical", is_critical: true },
  ];

  it("selects every non-critical item regardless of confidence review", () => {
    expect([...toggleAllSelectableIds(traces, new Set(["high"]))]).toEqual(["high", "low"]);
  });

  it("clears the selection when all cleanable items are selected", () => {
    expect(toggleAllSelectableIds(traces, new Set(["high", "low"]))).toEqual(new Set());
  });
});
