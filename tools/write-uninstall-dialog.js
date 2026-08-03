const fs = require("fs");
const path = require("path");

const target = path.join(
  __dirname,
  "..",
  "src-tauri",
  "src-frontends",
  "webui",
  "src",
  "components",
  "UninstallDialog.tsx"
);

const content = `import { useState, useRef } from "react";
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
          <h2 className="text-lg font-semibold text-white">\u786E\u8BA4\u5378\u8F7D</h2>
        </div>
        <Card className="flex-1">
          <CardContent className="p-6 space-y-4">
            <div className="flex items-center gap-3 text-yellow-400">
              <AlertTriangle className="h-8 w-8" />
              <div>
                <p className="text-base font-medium">\u5373\u5C06\u5378\u8F7D\u4EE5\u4E0B\u7A0B\u5E8F\uFF1A</p>
                <p className="text-2xl font-bold text-white mt-1">{selectedProgram.name}</p>
                <p className="text-sm text-slate-400 mt-1">
                  {selectedProgram.publisher ?? "\u672A\u77E5\u53D1\u5E03\u8005"} \u00B7 {selectedProgram.version ?? "\u672A\u77E5\u7248\u672C"}
                </p>
              </div>
            </div>
            <p className="text-sm text-slate-400">
              \u6B64\u64CD\u4F5C\u5C06\u6267\u884C\u7A0B\u5E8F\u81EA\u5E26\u7684\u5378\u8F7D\u547D\u4EE4\uFF0C\u5E76\u81EA\u52A8\u626B\u63CF\u6E05\u7406\u6B8B\u7559\u6587\u4EF6\u548C\u6CE8\u518C\u8868\u9879\u3002\u9700\u8981\u7BA1\u7406\u5458\u6743\u9650\u3002
            </p>
            <div className="flex gap-3 pt-2">
              <Button variant="destructive" onClick={handleUninstall}>\u786E\u8BA4\u5378\u8F7D</Button>
              <Button variant="secondary" onClick={() => setViewMode("detail")}>\u53D6\u6D88</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-4">
      <div className="flex items-center gap-3 mb-4">
        <h2 className="text-lg font-semibold text-white">\u5378\u8F7D\u8FDB\u5EA6</h2>
        {uninstalling && <Loader2 className="h-4 w-4 animate-spin text-blue-500" />}
      </div>
      <Card className="flex-1 flex flex-col">
        <CardHeader>
          <span className="text-sm text-slate-300">{selectedProgram.name}</span>
        </CardHeader>
        <CardContent className="flex-1 flex flex-col">
          {uninstallResult && (
            <div className={\`flex items-center gap-2 p-3 rounded-md mb-3 \${uninstallResult.success ? "bg-green-900/30 text-green-400" : "bg-red-900/30 text-red-400"}\`}>
              {uninstallResult.success ? <CheckCircle2 className="h-5 w-5" /> : <XCircle className="h-5 w-5" />}
              <span className="text-sm font-medium">{uninstallResult.message}</span>
            </div>
          )}
          <div className="flex-1 overflow-auto rounded-md bg-slate-900 p-3 font-mono text-xs">
            {logs.length === 0 && uninstalling && <div className="text-slate-500">\u51C6\u5907\u4E2D...</div>}
            {logs.map((log, i) => <div key={i} className="text-slate-300 py-0.5">{log}</div>)}
            <div ref={logEndRef} />
          </div>
          {!uninstalling && <div className="mt-3"><Button onClick={handleClose}>\u5B8C\u6210</Button></div>}
        </CardContent>
      </Card>
    </div>
  );
}

function formatProgressLog(event: UninstallProgress): string | null {
  switch (event.stage) {
    case "target_resolved":
      return "[\u5B9A\u4F4D] " + event.program.name + " | \u8DEF\u7531: " + event.route + " | \u547D\u4EE4: " + (event.uninstall_command ?? "\u65E0");
    case "uninstall_started":
      return "[\u5378\u8F7D] \u6267\u884C\u547D\u4EE4: " + event.command;
    case "uninstall_completed":
      return "[\u5378\u8F7D] \u5B8C\u6210, exit_code=" + (event.exit_code ?? "unknown") + ", \u9700\u91CD\u542F=" + event.reboot_required;
    case "scan_completed":
      return "[\u626B\u63CF] \u53D1\u73B0 " + event.traces.length + " \u4E2A\u6B8B\u7559\u75D5\u8FF9";
    case "clean_completed":
      return "[\u6E05\u7406] \u6210\u529F=" + event.success_count + ", \u5931\u8D25=" + event.failed_count;
    case "finished":
      return "[\u5B8C\u6210] " + event.message;
    default:
      return null;
  }
}
`;

fs.writeFileSync(target, content, "utf8");
console.log("Wrote UninstallDialog.tsx (" + content.length + " bytes)");
