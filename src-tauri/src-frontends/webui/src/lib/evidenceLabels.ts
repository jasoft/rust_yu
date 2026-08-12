import { t, type TranslationKey } from "../i18n/index.ts";

const policyActionKeys: Record<string, TranslationKey> = {
  scan: "components.evidencecenter.policy.action.scan",
  export: "components.evidencecenter.policy.action.export",
  backup: "components.evidencecenter.policy.action.backup",
  delete_after_confirmation: "components.evidencecenter.policy.action.delete_after_confirmation",
  list_backups: "components.evidencecenter.policy.action.list_backups",
  restore: "components.evidencecenter.policy.action.restore",
  retry_restore: "components.evidencecenter.policy.action.retry_restore",
};

export function policyActionLabel(action: string): string {
  const key = policyActionKeys[action];
  return key ? t(key) : t("components.evidencecenter.policy.action.other");
}
