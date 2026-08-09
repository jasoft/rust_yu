import type {
  Trace,
  UninstallEventPayload,
  UninstallJobEvent,
  UninstallPhase,
} from "../../types";

export type TraceCategory = "files" | "user_data" | "registry" | "system";

export interface TraceCategorySummary {
  category: TraceCategory;
  count: number;
  bytes: number;
}

const phaseLabels: Record<UninstallPhase, string> = {
  planned: "准备",
  running_uninstaller: "卸载程序",
  verifying_removal: "移除验证",
  scanning_residues: "残留扫描",
  awaiting_cleanup_confirmation: "等待确认",
  cleaning_residues: "残留清理",
  completed: "已完成",
  cancelled: "已取消",
  failed: "失败",
};

export function traceCategory(trace: Trace): TraceCategory {
  switch (trace.trace_type as string) {
    case "appdata":
    case "AppData":
      return "user_data";
    case "registry_key":
    case "registry_value":
    case "RegistryKey":
    case "RegistryValue":
      return "registry";
    case "scheduled_task":
    case "service":
    case "driver":
    case "ScheduledTask":
    case "Service":
    case "Driver":
      return "system";
    default:
      return "files";
  }
}

export function summarizeTraces(traces: Trace[]): TraceCategorySummary[] {
  const summary: Record<TraceCategory, TraceCategorySummary> = {
    files: { category: "files", count: 0, bytes: 0 },
    user_data: { category: "user_data", count: 0, bytes: 0 },
    registry: { category: "registry", count: 0, bytes: 0 },
    system: { category: "system", count: 0, bytes: 0 },
  };

  for (const trace of traces) {
    const bucket = summary[traceCategory(trace)];
    bucket.count += 1;
    bucket.bytes += trace.size ?? 0;
  }

  return Object.values(summary);
}

export function formatTraceType(traceType: string): string {
  switch (traceType) {
    case "registry_key":
    case "RegistryKey":
      return "注册表键";
    case "registry_value":
    case "RegistryValue":
      return "注册表值";
    case "appdata":
    case "AppData":
      return "用户数据";
    case "shortcut":
    case "Shortcut":
      return "快捷方式";
    case "scheduled_task":
    case "ScheduledTask":
      return "计划任务";
    case "service":
    case "Service":
      return "系统服务";
    case "driver":
    case "Driver":
      return "驱动程序";
    case "file":
    case "File":
      return "文件或目录";
    default:
      return "文件或目录";
  }
}

export function formatUninstallEventLog(
  event: UninstallJobEvent,
  now: Date = new Date(),
): string {
  const timestamp = now.toLocaleTimeString("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const prefix = `[${timestamp}] [${phaseLabels[event.phase]}]`;
  return `${prefix} ${formatEventPayload(event.payload)}`;
}

function formatEventPayload(payload: UninstallEventPayload): string {
  switch (payload.kind) {
    case "planned":
      return "已生成卸载快照，等待用户确认";
    case "uninstaller_started":
      return `启动受控卸载器：${payload.command_summary}`;
    case "uninstaller_completed":
      return `内置卸载器结束，退出码 ${payload.exit_code ?? "未知"}${payload.reboot_required ? "，需要重启" : ""}`;
    case "removal_verified":
      return payload.removed
        ? "程序移除验证通过，开始扫描残留位置"
        : "程序移除验证未通过，停止后续清理";
    case "residues_scanned":
      return `扫描完成，发现 ${payload.count} 个残留项目`;
    case "cleanup_started":
      return `开始清理用户确认的 ${payload.count} 个项目`;
    case "cleanup_completed":
      return `清理完成，成功 ${payload.success_count} 个，失败 ${payload.failed_count} 个`;
    case "finished":
      return payload.message;
  }
}
