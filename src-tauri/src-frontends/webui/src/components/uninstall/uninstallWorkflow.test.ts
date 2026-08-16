import { describe, expect, it } from "vitest";
import {
  nextStageAfterScan,
  stageForBackendPhase,
  uninstallWorkflowSteps,
} from "./uninstallWorkflow";

describe("uninstall workflow navigation", () => {
  it("keeps scan results visible until the user continues", () => {
    expect(stageForBackendPhase("uninstall", "scanning_residues")).toBe("scan");
    expect(stageForBackendPhase("scan", "awaiting_cleanup_confirmation")).toBe("scan");
    expect(stageForBackendPhase("scan", "completed")).toBe("scan");
  });

  it("only enters review from an explicit scan continuation", () => {
    expect(nextStageAfterScan("awaiting_cleanup_confirmation")).toBe("review");
    expect(nextStageAfterScan("completed")).toBe("complete");
  });

  it("shows cleanup as a real stage before completion", () => {
    expect(stageForBackendPhase("review", "cleaning_residues")).toBe("cleanup");
    expect(stageForBackendPhase("cleanup", "completed")).toBe("complete");
    expect(uninstallWorkflowSteps).toEqual([
      "confirm",
      "uninstall",
      "scan",
      "review",
      "cleanup",
      "complete",
    ]);
  });
});
