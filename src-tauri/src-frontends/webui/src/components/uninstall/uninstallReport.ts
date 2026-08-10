import { getLanguage, t } from "../../i18n/index.ts";
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
  planned: t("components.uninstall.uninstallreport.message_001"),
  running_uninstaller: t("components.programdetail.message_013"),
  verifying_removal: t("components.uninstall.uninstallreport.message_003"),
  scanning_residues: t("components.uninstall.uninstallreport.message_004"),
  awaiting_cleanup_confirmation: t("components.uninstall.uninstallreport.message_005"),
  cleaning_residues: t("components.uninstall.uninstallreport.message_006"),
  completed: t("app.message_133"),
  cancelled: t("app.message_159"),
  failed: t("app.message_108"),
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
      return t("components.installmonitormanager.message_011");
    case "registry_value":
    case "RegistryValue":
      return t("components.installmonitormanager.message_012");
    case "appdata":
    case "AppData":
      return t("app.message_023");
    case "shortcut":
    case "Shortcut":
      return t("components.tracepanel.message_004");
    case "scheduled_task":
    case "ScheduledTask":
      return t("components.startupmanager.message_007");
    case "service":
    case "Service":
      return t("components.tracepanel.message_006");
    case "driver":
    case "Driver":
      return t("components.tracepanel.message_007");
    case "file":
    case "File":
      return t("components.uninstall.uninstallreport.message_017");
    default:
      return t("components.uninstall.uninstallreport.message_017");
  }
}

export function formatUninstallEventLog(
  event: UninstallJobEvent,
  now: Date = new Date(),
): string {
  const timestamp = now.toLocaleTimeString(getLanguage(), {
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
      return t("components.uninstall.uninstallreport.message_019");
    case "uninstaller_started":
      return t("components.uninstall.uninstallreport.message_020", { value0: payload.command_summary });
    case "uninstaller_completed":
      return t("components.uninstall.uninstallreport.message_021", { value0: payload.exit_code ?? t("components.uninstall.uninstallreport.message_027"), value1: payload.reboot_required ? t("components.uninstall.uninstallreport.message_028") : "" });
    case "removal_verified":
      return payload.removed
        ? t("components.uninstall.uninstallreport.message_022")
        : t("components.uninstall.uninstallreport.message_023");
    case "residues_scanned":
      return t("components.uninstall.uninstallreport.message_024", { value0: payload.count });
    case "cleanup_started":
      return t("components.uninstall.uninstallreport.message_025", { value0: payload.count });
    case "cleanup_completed":
      return t("components.uninstall.uninstallreport.message_026", { value0: payload.success_count, value1: payload.failed_count });
    case "finished":
      return payload.message;
  }
}
