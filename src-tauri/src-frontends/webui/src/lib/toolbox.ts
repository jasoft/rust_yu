import { t } from "../i18n/index.ts";
import {
  Activity,
  AppWindow,
  FileText,
  ShieldCheck,
  Sparkles,
  SquareActivity,
  Zap,
  type LucideIcon,
} from "lucide-react";

export type ToolboxTarget = "health" | "startup" | "cleaner" | "backups" | "monitor" | "reports" | "plugins";

export interface ToolboxItem {
  id: ToolboxTarget;
  title: string;
  description: string;
  detail: string;
  keywords: string[];
  icon: LucideIcon;
  tone: string;
}

export const toolboxItems: readonly ToolboxItem[] = [
  { id: "health", title: t("app.message_033"), description: t("lib.toolbox.message_002"), detail: t("lib.toolbox.message_003"), keywords: [t("lib.toolbox.message_004"), t("lib.toolbox.message_005"), t("lib.toolbox.message_006"), t("lib.toolbox.message_007"), t("lib.toolbox.message_008")], icon: Activity, tone: "blue" },
  { id: "startup", title: t("app.message_034"), description: t("lib.toolbox.message_010"), detail: t("lib.toolbox.message_011"), keywords: [t("lib.toolbox.message_012"), t("lib.toolbox.message_013"), t("components.startupmanager.message_010"), t("components.startupmanager.message_008"), "runonce"], icon: Zap, tone: "amber" },
  { id: "cleaner", title: t("app.message_035"), description: t("lib.toolbox.message_017"), detail: t("lib.toolbox.message_018"), keywords: [t("lib.toolbox.message_019"), t("lib.toolbox.message_020"), t("lib.toolbox.message_021"), t("lib.toolbox.message_022"), "winapp2"], icon: Sparkles, tone: "violet" },
  { id: "backups", title: t("app.message_036"), description: t("lib.toolbox.message_024"), detail: t("lib.toolbox.message_025"), keywords: [t("lib.toolbox.message_026"), t("components.backupcenter.message_033"), t("lib.toolbox.message_028"), t("app.message_024"), t("lib.toolbox.message_030")], icon: ShieldCheck, tone: "green" },
  { id: "monitor", title: t("app.message_038"), description: t("lib.toolbox.message_032"), detail: t("lib.toolbox.message_033"), keywords: [t("lib.toolbox.message_034"), t("lib.toolbox.message_035"), t("lib.toolbox.message_036"), t("lib.toolbox.message_037"), "trace"], icon: SquareActivity, tone: "cyan" },
  { id: "reports", title: t("app.message_040"), description: t("lib.toolbox.message_039"), detail: t("lib.toolbox.message_040"), keywords: [t("lib.toolbox.message_041"), t("lib.toolbox.message_042"), t("lib.toolbox.message_043"), t("lib.toolbox.message_044"), t("app.message_108")], icon: FileText, tone: "slate" },
  { id: "plugins", title: t("app.message_041"), description: t("lib.toolbox.message_047"), detail: t("lib.toolbox.message_048"), keywords: [t("lib.toolbox.message_049"), t("lib.toolbox.message_050"), t("lib.toolbox.message_051"), "chrome", "edge"], icon: AppWindow, tone: "rose" },
];

export function filterToolboxItems(query: string): ToolboxItem[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return [...toolboxItems];
  return toolboxItems.filter((item) => [item.title, item.description, item.detail, ...item.keywords].join(" ").toLocaleLowerCase().includes(normalized));
}
