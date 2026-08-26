export type AnalysisWorkflowStage =
  | "analysis-scan"
  | "analysis-review"
  | "analysis-complete";

export const analysisWorkflowSteps = [
  "analysis-scan",
  "analysis-review",
  "analysis-complete",
] as const satisfies readonly AnalysisWorkflowStage[];

export function isAnalysisWorkflowStage(
  stage: string,
): stage is AnalysisWorkflowStage {
  return stage === "analysis-scan"
    || stage === "analysis-review"
    || stage === "analysis-complete";
}
