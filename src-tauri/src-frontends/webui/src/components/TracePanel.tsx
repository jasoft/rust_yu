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
import { useProgramsStore } from "../stores/programs";
import { Button } from "./ui/button";
import { Card, CardContent } from "./ui/card";
import { Badge } from "./ui/badge";
import { formatBytes } from "../lib/utils";
import type { Trace } from "../types";

const traceTypeLabels: Record<string, { label: string; icon: typeof FileCode; variant: "default" | "secondary" | "warning" | "success" }> = {
  registry_key: { label: "注册表", icon: Database, variant: "default" },
  registry_value: { label: "注册表值", icon: Database, variant: "default" },
  file: { label: "文件", icon: FileCode, variant: "warning" },
  appdata: { label: "AppData", icon: FolderOpen, variant: "success" },
  shortcut: { label: "快捷方式", icon: Link2, variant: "secondary" },
};

export function TracePanel() {
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
        扫描中...
      </div>
    );
  }

  const allSelected = traces.length > 0 && selectedTraces.size === traces.length;
  const hasCleaned = cleanResults.length > 0;

  return (
    <div className="flex flex-col h-full overflow-auto">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700">
        <button onClick={resetTraces} className="text-slate-400 hover:text-white">
          <ArrowLeft className="h-4 w-4" />
        </button>
        <h2 className="text-lg font-semibold text-white">残留扫描结果</h2>
        <span className="text-sm text-slate-400">{selectedProgram?.name}</span>
      </div>

      <div className="flex-1 p-4 space-y-4">
        {/* 操作栏 */}
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="sm" onClick={toggleAllTraces}>
            {allSelected ? "取消全选" : "全选"}
          </Button>
          <span className="text-xs text-slate-500">
            已选 {selectedTraces.size} / {traces.length} 项
          </span>
          <div className="flex-1" />
          {!hasCleaned && (
            <Button
              variant="destructive"
              size="sm"
              disabled={selectedTraces.size === 0}
              onClick={() => cleanTraces(true)}
            >
              <Trash2 className="h-4 w-4" />
              清理选中项 ({selectedTraces.size})
            </Button>
          )}
        </div>

        {/* 清理结果 */}
        {hasCleaned && (
          <Card>
            <CardContent className="flex items-center gap-2 py-3">
              <CheckCircle2 className="h-5 w-5 text-green-400" />
              <span className="text-sm text-green-400">
                已清理 {cleanResults.filter((r) => r.success).length} 项，
                释放 {formatBytes(cleanResults.reduce((sum, r) => sum + r.bytes_freed, 0))}
              </span>
            </CardContent>
          </Card>
        )}

        {/* 痕迹列表 */}
        {traces.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-slate-500">
            <CheckCircle2 className="h-8 w-8 mb-2" />
            <span>未发现残留痕迹</span>
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
      disabled={cleaned}
      className={`flex w-full items-start gap-3 rounded-md border p-3 text-left transition-colors ${
        selected
          ? "border-blue-500/50 bg-blue-900/10"
          : "border-slate-700 bg-slate-800 hover:bg-slate-700/50"
      }`}
    >
      <input
        type="checkbox"
        checked={selected}
        onChange={onToggle}
        disabled={cleaned}
        className="mt-1"
      />
      <Icon className="h-4 w-4 mt-0.5 text-slate-400 shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <Badge variant={meta.variant}>{meta.label}</Badge>
          <Badge variant="outline">{trace.confidence}</Badge>
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

