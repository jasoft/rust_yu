import { t } from "../i18n/index.ts";
import {
  ArrowLeft,
  CheckCircle2,
  Trash2,
  Loader2,
  FileCode,
  Database,
  FolderOpen,
  Link2,
} from "lucide-react";
import { useState } from "react";
import { useProgramsStore } from "../stores/programs";
import { Button } from "./ui/button";
import { Card, CardContent } from "./ui/card";
import { Badge } from "./ui/badge";
import { formatBytes } from "../lib/utils";
import type { Trace } from "../types";

const traceTypeLabels: Record<string, { label: string; icon: typeof FileCode; variant: "default" | "secondary" | "warning" | "success" }> = {
  registry_key: { label: t("app.message_024"), icon: Database, variant: "default" },
  registry_value: { label: t("components.installmonitormanager.message_012"), icon: Database, variant: "default" },
  file: { label: t("components.installmonitormanager.message_009"), icon: FileCode, variant: "warning" },
  appdata: { label: "AppData", icon: FolderOpen, variant: "success" },
  shortcut: { label: t("components.tracepanel.message_004"), icon: Link2, variant: "secondary" },
  scheduled_task: { label: t("components.startupmanager.message_007"), icon: FileCode, variant: "warning" },
  service: { label: t("components.tracepanel.message_006"), icon: FileCode, variant: "warning" },
  driver: { label: t("components.tracepanel.message_007"), icon: FileCode, variant: "warning" },
};

export function TracePanel() {
  const [cleanConfirmOpen, setCleanConfirmOpen] = useState(false);
  const {
    traces,
    tracesLoading,
    selectedTraces,
    cleanResults,
    selectedProgram,
    toggleTrace,
    toggleAllTraces,
    cleanTraces,
    resetTraces,
  } = useProgramsStore();

  if (tracesLoading) {
    return (
      <div className="flex items-center justify-center h-full text-slate-500">
        <Loader2 className="h-5 w-5 animate-spin mr-2" />

        {t("components.tracepanel.message_008")}
      </div>
    );
  }

  const selectableCount = traces.filter((trace) => !trace.is_critical).length;
  const allSelected = selectableCount > 0 && selectedTraces.size === selectableCount;
  const hasCleaned = cleanResults.length > 0;

  return (
    <div className="flex min-w-0 flex-col h-full overflow-y-auto overflow-x-hidden">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700">
        <button onClick={resetTraces} className="text-slate-400 hover:text-white">
          <ArrowLeft className="h-4 w-4" />
        </button>
        <h2 className="text-lg font-semibold text-white">{t("components.tracepanel.message_009")}</h2>
        <span className="text-sm text-slate-400">{selectedProgram?.name}</span>
      </div>

      <div className="min-w-0 flex-1 space-y-4 p-4">
        {/* 操作栏 */}
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="sm" onClick={toggleAllTraces} disabled={selectableCount === 0}>
            {allSelected ? t("components.cleanerpage.message_030") : t("components.tracepanel.message_011")}
          </Button>
          <span className="text-xs text-slate-500">

            {t("components.browserpluginspage.message_019")} {selectedTraces.size} / {selectableCount}  {t("app.message_167")}
            {traces.length > selectableCount && t("components.tracepanel.message_014", { value0: traces.length - selectableCount })}
          </span>
          <div className="flex-1" />
          {!hasCleaned && (
            <Button
              variant="destructive"
              size="sm"
              disabled={selectedTraces.size === 0}
              onClick={() => setCleanConfirmOpen(true)}
            >
              <Trash2 className="h-4 w-4" />

              {t("components.tracepanel.message_015")}{selectedTraces.size})
            </Button>
          )}
        </div>

        {cleanConfirmOpen && !hasCleaned && (
          <Card className="border-amber-500/50">
            <CardContent className="space-y-3 py-4">
              <p className="text-sm font-medium text-amber-300">{t("components.tracepanel.message_016")}</p>
              <p className="text-xs text-slate-400">

                {t("components.tracepanel.message_017")} {selectedTraces.size}  {t("components.tracepanel.message_018")}
              </p>
              <div className="flex gap-2">
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => {
                    setCleanConfirmOpen(false);
                    void cleanTraces(true);
                  }}
                >

                  {t("app.message_245")}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setCleanConfirmOpen(false)}
                >

                  {t("app.message_089")}
                </Button>
              </div>
            </CardContent>
          </Card>
        )}

        {/* 清理结果 */}
        {hasCleaned && (
          <Card>
            <CardContent className="flex items-center gap-2 py-3">
              <CheckCircle2 className="h-5 w-5 text-green-400" />
              <span className="text-sm text-green-400">

                {t("app.message_261")} {cleanResults.filter((r) => r.success).length}  {t("components.tracepanel.message_022")} {formatBytes(cleanResults.reduce((sum, r) => sum + r.bytes_freed, 0))}
              </span>
            </CardContent>
          </Card>
        )}

        {/* 痕迹列表 */}
        {traces.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-slate-500">
            <CheckCircle2 className="h-8 w-8 mb-2" />
            <span>{t("components.tracepanel.message_023")}</span>
          </div>
        ) : (
          <div className="space-y-2">
            {traces.map((trace) => (
              <TraceItem
                key={trace.id}
                trace={trace}
                selected={selectedTraces.has(trace.id)}
                onToggle={() => toggleTrace(trace.id)}
                cleaned={hasCleaned}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TraceItem({
  trace,
  selected,
  onToggle,
  cleaned,
}: {
  trace: Trace;
  selected: boolean;
  onToggle: () => void;
  cleaned: boolean;
}) {
  const meta = traceTypeLabels[trace.trace_type] ?? {
    label: trace.trace_type,
    icon: FileCode,
    variant: "secondary" as const,
  };
  const Icon = meta.icon;

  return (
    <button
      onClick={onToggle}
      disabled={cleaned || trace.is_critical}
      className={`flex min-w-0 w-full items-start gap-3 rounded-md border p-3 text-left transition-colors ${
        selected
          ? "border-blue-500/50 bg-blue-900/10"
          : "border-slate-700 bg-slate-800 hover:bg-slate-700/50"
      }`}
    >
      <input
        type="checkbox"
        checked={selected}
        readOnly
        disabled={cleaned || trace.is_critical}
        tabIndex={-1}
        className="pointer-events-none mt-1"
      />
      <Icon className="h-4 w-4 mt-0.5 text-slate-400 shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <Badge variant={meta.variant}>{meta.label}</Badge>
          <Badge variant="outline">{trace.confidence}</Badge>
          {trace.is_critical && <Badge variant="secondary">{t("app.message_250")}</Badge>}
          {trace.size != null && (
            <span className="text-xs text-slate-500">{formatBytes(trace.size)}</span>
          )}
        </div>
        <p className="text-xs text-slate-300 break-all">{trace.path}</p>
        {trace.description && (
          <p className="text-xs text-slate-500 mt-1">{trace.description}</p>
        )}
      </div>
    </button>
  );
}

