import { useEffect, useRef, useState } from "react";
import { Loader2, ArrowLeft, AlertTriangle } from "lucide-react";
import { useProgramsStore } from "../stores/programs";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { ResidueReview } from "./uninstall/ResidueReview";
import { isModalLocked } from "./uninstall/uninstallState";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader } from "./ui/card";
import type { UninstallJobEvent } from "../types";

/** 旧 detail 路由使用的卸载对话框；工作流与主 Apps 页面共享同一 coordinator job。 */
export function UninstallDialog() {
  const selectedProgram = useProgramsStore((state) => state.selectedProgram);
  const setViewMode = useProgramsStore((state) => state.setViewMode);
  const planUninstall = useProgramsStore((state) => state.planUninstall);
  const executeUninstall = useProgramsStore((state) => state.executeUninstall);
  const cleanUninstallResidues = useProgramsStore((state) => state.cleanUninstallResidues);
  const finishUninstall = useProgramsStore((state) => state.finishUninstall);
  const resetUninstall = useProgramsStore((state) => state.resetUninstall);
  const job = useProgramsStore((state) => state.uninstallJob);
  const uninstalling = useProgramsStore((state) => state.uninstalling);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [logs, setLogs] = useState<string[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (selectedProgram && !job) void planUninstall(selectedProgram.id);
  }, [job, planUninstall, selectedProgram]);

  useTauriEvent<UninstallJobEvent>("uninstall-job-progress", (event) => {
    if (job && event.job_id !== job.snapshot.job_id) return;
    setLogs((previous) => [...previous, `[${event.sequence}] ${event.phase}`]);
    setTimeout(() => logEndRef.current?.scrollIntoView({ behavior: "smooth" }), 50);
  });

  if (!selectedProgram || !job) return null;

  const locked = isModalLocked(job.phase);
  const traces = job.residue_review.traces;
  const close = () => {
    if (locked) return;
    resetUninstall();
    setViewMode("detail");
  };

  if (job.phase === "planned") {
    return (
      <div className="flex h-full flex-col p-4">
        <div className="mb-4 flex items-center gap-3">
          <button onClick={close} className="text-slate-400 hover:text-white"><ArrowLeft className="h-4 w-4" /></button>
          <h2 className="text-lg font-semibold text-white">确认卸载</h2>
        </div>
        <Card className="flex-1"><CardContent className="space-y-4 p-6">
          <div className="flex items-center gap-3 text-yellow-400"><AlertTriangle className="h-8 w-8" /><div><p>即将卸载：</p><p className="text-2xl font-bold text-white">{job.snapshot.program.name}</p></div></div>
          <p className="text-sm text-slate-400">{job.snapshot.route} 卸载器结束后，程序会先验证移除，再展示残留审查。</p>
          <div className="flex gap-3"><Button variant="destructive" onClick={() => void executeUninstall(job.snapshot.job_id)}>确认卸载</Button><Button variant="secondary" onClick={close}>取消</Button></div>
        </CardContent></Card>
      </div>
    );
  }

  if (job.phase === "awaiting_cleanup_confirmation") {
    return (
      <div className="flex h-full flex-col p-4"><Card className="flex-1"><CardHeader><span className="text-sm text-slate-300">{job.snapshot.program.name}</span></CardHeader><CardContent>
        <ResidueReview traces={traces} selectedIds={selectedIds} onToggle={(id) => setSelectedIds((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; })} onSkip={() => void finishUninstall(job.snapshot.job_id)} onClean={() => void cleanUninstallResidues(job.snapshot.job_id, { trace_ids: [...selectedIds], confirm: true })} />
      </CardContent></Card></div>
    );
  }

  return (
    <div className="flex h-full flex-col p-4"><Card className="flex-1 flex flex-col"><CardHeader><span className="text-sm text-slate-300">{job.snapshot.program.name}</span>{uninstalling && <Loader2 className="h-4 w-4 animate-spin text-blue-500" />}</CardHeader><CardContent className="flex flex-1 flex-col">
      <div className="flex-1 overflow-auto rounded-md bg-slate-900 p-3 font-mono text-xs text-slate-300">{logs.map((log, index) => <div key={`${log}-${index}`}>{log}</div>)}<div ref={logEndRef} /></div>
      {!locked && <div className="mt-3"><Button onClick={close}>完成</Button></div>}
    </CardContent></Card></div>
  );
}
