import { useState, useRef } from "react";
import { Loader2, CheckCircle2, XCircle, ArrowLeft, AlertTriangle } from "lucide-react";
import { useProgramsStore } from "../stores/programs";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader } from "./ui/card";
import type { UninstallProgress } from "../types";

export function UninstallDialog() {
  const {
    selectedProgram, uninstalling, uninstallResult,
    uninstallProgram, setViewMode, resetUninstall, loadPrograms,
  } = useProgramsStore();
  const [logs, setLogs] = useState<string[]>([]);
  const [confirmed, setConfirmed] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);

  useTauriEvent<UninstallProgress>("uninstall-progress", (event) => {
    const msg = formatProgressLog(event);
    if (msg) {
      setLogs((prev) => [...prev, msg]);
      setTimeout(() => logEndRef.current?.scrollIntoView({ behavior: "smooth" }), 50);
    }
  });

  if (!selectedProgram) return null;

  const handleUninstall = () => {
    setConfirmed(true);
    setLogs([]);
    uninstallProgram({
      program_name: selectedProgram.name,
      clean_after: true,
      confirm: true,
      timeout_secs: 120,
    });
  };

  const handleClose = () => {
    resetUninstall();
    loadPrograms({ refresh: true });
    setViewMode("detail");
  };

  if (!confirmed) {
    return (
      <div className="flex flex-col h-full p-4">
        <div className="flex items-center gap-3 mb-4">
          <button onClick={() => setViewMode("detail")} className="text-slate-400 hover:text-white">
            <ArrowLeft className="h-4 w-4" />
          </button>
          <h2 className="text-lg font-semibold text-white">确认卸载</h2>
        </div>
        <Card className="flex-1">
          <CardContent className="p-6 space-y-4">
            <div className="flex items-center gap-3 text-yellow-400">
              <AlertTriangle className="h-8 w-8" />
              <div>
                <p className="text-base font-medium">即将卸载以下程序：</p>
                <p className="text-2xl font-bold text-white mt-1">{selectedProgram.name}</p>
                <p className="text-sm text-slate-400 mt-1">
                  {selectedProgram.publisher ?? "未知发布者"} · {selectedProgram.version ?? "未知版本"}
                </p>
              </div>
            </div>
            <p className="text-sm text-slate-400">
              此操作将执行程序自带的卸载命令，并自动扫描清理残留文件和注册表项。需要管理员权限。
            </p>
            <div className="flex gap-3 pt-2">
              <Button variant="destructive" onClick={handleUninstall}>确认卸载</Button>
              <Button variant="secondary" onClick={() => setViewMode("detail")}>取消</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-4">
      <div className="flex items-center gap-3 mb-4">
        <h2 className="text-lg font-semibold text-white">卸载进度</h2>
        {uninstalling && <Loader2 className="h-4 w-4 animate-spin text-blue-500" />}
      </div>
      <Card className="flex-1 flex flex-col">
        <CardHeader>
          <span className="text-sm text-slate-300">{selectedProgram.name}</span>
        </CardHeader>
        <CardContent className="flex-1 flex flex-col">
          {uninstallResult && (
            <div className={`flex items-center gap-2 p-3 rounded-md mb-3 ${uninstallResult.success ? "bg-green-900/30 text-green-400" : "bg-red-900/30 text-red-400"}`}>
              {uninstallResult.success ? <CheckCircle2 className="h-5 w-5" /> : <XCircle className="h-5 w-5" />}
              <span className="text-sm font-medium">{uninstallResult.message}</span>
            </div>
          )}
          <div className="flex-1 overflow-auto rounded-md bg-slate-900 p-3 font-mono text-xs">
            {logs.length === 0 && uninstalling && <div className="text-slate-500">准备中...</div>}
            {logs.map((log, i) => <div key={i} className="text-slate-300 py-0.5">{log}</div>)}
            <div ref={logEndRef} />
          </div>
          {!uninstalling && <div className="mt-3"><Button onClick={handleClose}>完成</Button></div>}
        </CardContent>
      </Card>
    </div>
  );
}

function formatProgressLog(event: UninstallProgress): string | null {
  switch (event.stage) {
    case "target_resolved":
      return "[定位] " + event.program.name + " | 路由: " + event.route + " | 命令: " + (event.uninstall_command ?? "无");
    case "uninstall_started":
      return "[卸载] 执行命令: " + event.command;
    case "uninstall_completed":
      return "[卸载] 完成, exit_code=" + (event.exit_code ?? "unknown") + ", 需重启=" + event.reboot_required;
    case "scan_completed":
      return "[扫描] 发现 " + event.traces.length + " 个残留痕迹";
    case "clean_completed":
      return "[清理] 成功=" + event.success_count + ", 失败=" + event.failed_count;
    case "finished":
      return "[完成] " + event.message;
    default:
      return null;
  }
}
