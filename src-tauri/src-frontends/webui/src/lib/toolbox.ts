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
  { id: "health", title: "软件健康", description: "从本机清单、自启动和使用记录中找出需要复核的项目。", detail: "只读分析 · 手动更新入口", keywords: ["健康", "评分", "重复", "更新", "使用记录"], icon: Activity, tone: "blue" },
  { id: "startup", title: "自启动管理", description: "集中查看注册表、启动文件夹、服务和计划任务中的启动项。", detail: "先预览 · 按来源授权 · 可回滚", keywords: ["启动", "开机", "服务", "任务", "runonce"], icon: Zap, tone: "amber" },
  { id: "cleaner", title: "系统清理", description: "先分析规则命中的缓存和临时文件，再由你确认清理范围。", detail: "分析优先 · 低置信度默认保留", keywords: ["清理", "缓存", "临时", "垃圾", "winapp2"], icon: Sparkles, tone: "violet" },
  { id: "backups", title: "备份与恢复", description: "查看清理前的文件和注册表备份，在不覆盖新数据的前提下恢复。", detail: "只创建、不覆盖 · 失败可重试", keywords: ["备份", "恢复", "回滚", "注册表", "安全"], icon: ShieldCheck, tone: "green" },
  { id: "monitor", title: "安装监控", description: "用安装前后快照和差异报告记录文件、注册表与进程证据。", detail: "本机快照 · 可导出 Trace 证据", keywords: ["安装", "监控", "快照", "差异", "trace"], icon: SquareActivity, tone: "cyan" },
  { id: "reports", title: "卸载记录", description: "重开已完成任务，复核成功和失败项，并导出完整报告。", detail: "JSON · HTML · TXT · 本地保存", keywords: ["报告", "历史", "记录", "导出", "失败"], icon: FileText, tone: "slate" },
  { id: "plugins", title: "浏览器插件", description: "扫描已安装浏览器的扩展，关闭浏览器后再确认移除。", detail: "仅处理可识别扩展 · 关闭浏览器后执行", keywords: ["浏览器", "插件", "扩展", "chrome", "edge"], icon: AppWindow, tone: "rose" },
];

export function filterToolboxItems(query: string): ToolboxItem[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return [...toolboxItems];
  return toolboxItems.filter((item) => [item.title, item.description, item.detail, ...item.keywords].join(" ").toLocaleLowerCase().includes(normalized));
}
