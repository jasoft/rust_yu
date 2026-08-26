import { describe, expect, it } from "vitest";
import {
  analysisWorkflowSteps,
  isAnalysisWorkflowStage,
} from "../src/components/analysis/analysisWorkflow";

describe("deep analysis workflow", () => {
  it("uses a dedicated three-stage flow instead of the uninstall wizard", () => {
    expect(analysisWorkflowSteps).toEqual([
      "analysis-scan",
      "analysis-review",
      "analysis-complete",
    ]);
  });

  it("keeps uninstall and analysis stage routing separate", () => {
    expect(isAnalysisWorkflowStage("scan")).toBe(false);
    expect(isAnalysisWorkflowStage("review")).toBe(false);
    expect(isAnalysisWorkflowStage("analysis-scan")).toBe(true);
    expect(isAnalysisWorkflowStage("analysis-review")).toBe(true);
    expect(isAnalysisWorkflowStage("analysis-complete")).toBe(true);
  });
});
