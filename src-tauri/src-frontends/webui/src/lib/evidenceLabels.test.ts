import { describe, expect, it } from "vitest";
import { policyActionLabel } from "./evidenceLabels";

describe("policyActionLabel", () => {
  it("does not expose backend action identifiers", () => {
    for (const action of ["scan", "export", "backup", "delete_after_confirmation", "list_backups", "restore", "retry_restore"]) {
      expect(policyActionLabel(action)).not.toBe(action);
    }
  });

  it("does not expose an unknown identifier", () => {
    expect(policyActionLabel("future_internal_action")).not.toContain("future_internal_action");
  });
});
