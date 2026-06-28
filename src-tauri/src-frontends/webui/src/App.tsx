import { useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { ProgramList } from "./components/ProgramList";
import { ProgramDetail } from "./components/ProgramDetail";
import { UninstallDialog } from "./components/UninstallDialog";
import { TracePanel } from "./components/TracePanel";
import { useProgramsStore } from "./stores/programs";

type NavItem = "programs" | "reports" | "settings";

export default function App() {
  const [nav, setNav] = useState<NavItem>("programs");
  const viewMode = useProgramsStore((s) => s.viewMode);
  const selectedProgram = useProgramsStore((s) => s.selectedProgram);

  return (
    <div className="flex h-screen bg-slate-900 text-white">
      <Sidebar active={nav} onNavigate={setNav} />

      <main className="flex flex-1 min-w-0">
        <div className="w-80 shrink-0 border-r border-slate-700">
          <ProgramList />
        </div>

        <div className="flex-1 min-w-0 bg-slate-900">
          {nav === "programs" ? (
            viewMode === "uninstall" ? (
              <UninstallDialog />
            ) : viewMode === "traces" ? (
              <TracePanel />
            ) : selectedProgram ? (
              <ProgramDetail />
            ) : (
              <EmptyState />
            )
          ) : nav === "reports" ? (
            <div className="flex items-center justify-center h-full text-slate-500">
              <span>卸载记录功能开发中...</span>
            </div>
          ) : (
            <div className="flex items-center justify-center h-full text-slate-500">
              <span>设置功能开发中...</span>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center h-full text-slate-500">
      <svg className="h-16 w-16 mb-4 text-slate-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
      </svg>
      <p className="text-sm">选择一个程序查看详情</p>
    </div>
  );
}
