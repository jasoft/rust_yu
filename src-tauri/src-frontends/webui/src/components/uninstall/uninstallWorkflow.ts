import type { UninstallPhase } from "../../types";

export type UninstallWorkflowStage =
  | "apps"
  | "confirm"
  | "scan"
  | "review"
  | "complete";

export const uninstallWorkflowSteps = [
  "confirm",
  "scan",
  "review",
  "complete",
] as const satisfies readonly Exclude<UninstallWorkflowStage, "apps">[];

/**
 * 后端可以连续完成卸载、核验和扫描，但前端不能因此跳过扫描结果。
 * 内置卸载、移除核验和残留扫描始终停留在同一个扫描页；
 * 扫描结束后只有用户明确点击下一步才进入复核。清理也留在复核页，
 * 完成后才进入最终报告。
 */
export function stageForBackendPhase(
  current: UninstallWorkflowStage,
  phase: UninstallPhase,
): UninstallWorkflowStage {
  switch (phase) {
    case "running_uninstaller":
    case "verifying_removal":
    case "scanning_residues":
    case "awaiting_cleanup_confirmation":
      return "scan";
    case "cleaning_residues":
      return "review";
    case "completed":
      return current === "review" ? "complete" : "scan";
    case "failed":
    case "cancelled":
    case "planned":
    default:
      return current;
  }
}

export function nextStageAfterScan(
  phase: UninstallPhase | undefined,
): UninstallWorkflowStage {
  return phase === "awaiting_cleanup_confirmation" ? "review" : "complete";
}
