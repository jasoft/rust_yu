import { describe, expect, it } from "vitest";
import {
  nextStageAfterScan,
  stageForBackendPhase,
  uninstallWorkflowSteps,
} from "./uninstallWorkflow";

describe("uninstall workflow navigation", () => {
  it("keeps the built-in uninstaller and scan on one page", () => {
    expect(stageForBackendPhase("confirm", "running_uninstaller")).toBe("scan");
    expect(stageForBackendPhase("scan", "verifying_removal")).toBe("scan");
    expect(stageForBackendPhase("scan", "scanning_residues")).toBe("scan");
    expect(stageForBackendPhase("scan", "awaiting_cleanup_confirmation")).toBe("scan");
    expect(stageForBackendPhase("scan", "completed")).toBe("scan");
  });

  it("only enters review from an explicit scan continuation", () => {
    expect(nextStageAfterScan("awaiting_cleanup_confirmation")).toBe("review");
    expect(nextStageAfterScan("completed")).toBe("complete");
  });

  it("keeps cleanup on the review page and removes legacy progress pages", () => {
    expect(stageForBackendPhase("review", "cleaning_residues")).toBe("review");
    expect(stageForBackendPhase("review", "completed")).toBe("complete");
    expect(uninstallWorkflowSteps).toEqual([
      "confirm",
      "scan",
      "review",
      "complete",
    ]);
  });
});
