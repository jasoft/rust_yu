import { t } from "../../i18n/index.ts";
import type { UninstallResult } from "../../types";

/**
 * 卸载失败时优先使用 invoke 的错误；如果后端已经返回失败结果，则回退到结果消息。
 * 这样错误可以留在当前卸载进度页，而不必依赖用户返回应用列表查看全局错误。
 */
export function getUninstallFailureMessage(
  commandError: string | null,
  result: UninstallResult | null,
): string | null {
  if (commandError?.trim()) return commandError;
  if (result && !result.success) return result.message || t("components.uninstall.uninstallfeedback.message_001");
  return null;
}
