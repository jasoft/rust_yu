import type { UninstallPhase } from "../../types";

export type UninstallWorkflowStage =
  | "apps"
  | "confirm"
  | "uninstall"
  | "scan"
  | "review"
  | "cleanup"
  | "complete";

export const uninstallWorkflowSteps = [
  "confirm",
  "uninstall",
  "scan",
  "review",
  "cleanup",
  "complete",
] as const satisfies readonly Exclude<UninstallWorkflowStage, "apps">[];

/**
 * 后端可以连续完成卸载、核验和扫描，但前端不能因此跳过扫描结果。
 * `awaiting_cleanup_confirmation` 和从扫描直接完成（零残留）都停在扫描页，
 * 直到用户明确点击下一步；只有清理完成才自动进入最终报告。
 */
export function stageForBackendPhase(
  current: UninstallWorkflowStage,
  phase: UninstallPhase,
): UninstallWorkflowStage {
  switch (phase) {
    case "running_uninstaller":
    case "verifying_removal":
      return "uninstall";
    case "scanning_residues":
    case "awaiting_cleanup_confirmation":
      return "scan";
    case "cleaning_residues":
      return "cleanup";
    case "completed":
      return current === "cleanup" ? "complete" : "scan";
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
