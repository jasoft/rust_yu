import { useEffect, useState } from "react";
import {
  AppWindow,
  Archive,
  Box,
  Check,
  ChevronDown,
  CircleHelp,
  Database,
  FileCode2,
  FileText,
  Folder,
  FolderOpen,
  Grid2X2,
  Info,
  Maximize2,
  Minus,
  Package,
  Play,
  RefreshCw,
  RotateCcw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  SquareActivity,
  Trash2,
  TriangleAlert,
  Wrench,
  X,
  Zap,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getProgramIconSrc } from "./lib/icon";
import {
  countProgramsBySource,
  filterPrograms,
  programSourceOptions,
  type ProgramSourceFilter,
} from "./lib/programFilters";
import { useProgramsStore } from "./stores/programs";
import { StartupManager } from "./components/StartupManager";
import { CleanerPage } from "./components/CleanerPage";
import { BrowserPluginsPage } from "./components/BrowserPluginsPage";
import { getUninstallFailureMessage } from "./components/uninstall/uninstallFeedback";
import type {
  CleanResult,
  InstalledProgram,
  Trace,
  UninstallJobEvent,
} from "./types";

type Stage = "apps" | "confirm" | "progress" | "scan" | "review" | "complete";
type NavKey = "apps" | "startup" | "cleaner" | "traces" | "monitor" | "plugins" | "tools" | "settings" | "about";

interface UiProgram {
  id: string;
  name: string;
  publisher: string;
  version: string;
  size: string;
  installed: string;
  location: string;
  icon: string;
  iconClass: string;
  source?: InstalledProgram;
}

const mockPrograms: UiProgram[] = [
  { id: "chrome", name: "Google Chrome", publisher: "Google LLC", version: "126.0", size: "1.12 GB", installed: "2025/05/10", location: "C:\\Program Files\\Google\\Chrome", icon: "G", iconClass: "chrome" },
  { id: "7zip", name: "7-Zip 24.09", publisher: "Igor Pavlov", version: "24.09", size: "5.52 MB", installed: "2025/05/18", location: "C:\\Program Files\\7-Zip", icon: "7z", iconClass: "sevenzip" },
  { id: "vc", name: "Microsoft Visual C++ 2015–2022", publisher: "Microsoft Corporation", version: "14.42", size: "20.5 MB", installed: "2025/04/22", location: "C:\\Program Files\\Microsoft Visual Studio", icon: "++", iconClass: "visual" },
  { id: "edge", name: "Microsoft Edge", publisher: "Microsoft Corporation", version: "126.0", size: "654 MB", installed: "2025/05/11", location: "C:\\Program Files (x86)\\Microsoft\\Edge", icon: "e", iconClass: "edge" },
  { id: "notepad", name: "Notepad++ (64-bit x64)", publisher: "Notepad++ Team", version: "8.6.7", size: "16.0 MB", installed: "2025/05/04", location: "C:\\Program Files\\Notepad++", icon: "N", iconClass: "notepad" },
  { id: "terminal", name: "Windows Terminal", publisher: "Microsoft Corporation", version: "1.20", size: "74.1 MB", installed: "2025/05/06", location: "C:\\Program Files\\WindowsApps", icon: ">_", iconClass: "terminal" },
  { id: "discord", name: "Discord", publisher: "Discord Inc.", version: "1.0", size: "106 MB", installed: "2025/04/30", location: "C:\\Users\\Admin\\AppData\\Local\\Discord", icon: "D", iconClass: "discord" },
  { id: "everything", name: "Everything 1.4.1.1026 (x64)", publisher: "voidtools", version: "1.4.1", size: "3.14 MB", installed: "2025/05/03", location: "C:\\Program Files\\Everything", icon: "Q", iconClass: "everything" },
];

const demoTraces: Trace[] = [
  { id: "t1", program_name: "7-Zip 24.09", trace_type: "file", path: "C:\\Program Files\\7-Zip\\", exists: true, size: 5_570_560, confidence: "high", description: "程序安装目录" },
  { id: "t2", program_name: "7-Zip 24.09", trace_type: "appdata", path: "C:\\Users\\Admin\\AppData\\Roaming\\7-Zip\\", exists: true, size: 3_276_800, confidence: "high", description: "用户配置文件" },
  { id: "t3", program_name: "7-Zip 24.09", trace_type: "appdata", path: "C:\\Users\\Admin\\AppData\\Local\\7-Zip\\", exists: true, size: 1_310_720, confidence: "medium", description: "本地缓存" },
  { id: "t4", program_name: "7-Zip 24.09", trace_type: "file", path: "C:\\Users\\Admin\\AppData\\Local\\Temp\\7zFM\\", exists: true, size: 430_080, confidence: "low", description: "临时目录，可能被其他程序使用" },
  { id: "t5", program_name: "7-Zip 24.09", trace_type: "shortcut", path: "C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\7-Zip.lnk", exists: true, size: 379_000, confidence: "medium", description: "开始菜单快捷方式" },
  { id: "t6", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\7zFM.exe", exists: true, size: 3_200, confidence: "high", description: "应用程序注册项" },
  { id: "t7", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKCU\\Software\\Classes\\7z.*", exists: true, size: 196_000, confidence: "medium", description: "文件关联" },
  { id: "t8", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.7z", exists: true, size: 56_000, confidence: "low", description: "用户文件关联历史" },
];

const initialLogs = [
  "[10:24:31] 创建系统还原点…",
  "[10:24:33] 还原点创建成功",
  "[10:24:34] 启动卸载程序：C:\\Program Files\\7-Zip\\Uninstall.exe",
  "[10:24:35] 正在停止相关服务…",
  "[10:24:36] 删除文件：C:\\Program Files\\7-Zip\\7z.exe",
  "[10:24:36] 删除文件：C:\\Program Files\\7-Zip\\7zFM.exe",
  "[10:24:37] 删除文件：C:\\Program Files\\7-Zip\\7-zip.chm",
];

function getInitialStage(): Stage {
  const value = new URLSearchParams(window.location.search).get("stage");
  return value === "confirm" || value === "progress" || value === "scan" || value === "review" || value === "complete" ? value : "apps";
}

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

function toUiProgram(program: InstalledProgram): UiProgram {
  return {
    id: program.id,
    name: program.name,
    publisher: program.publisher ?? "未知发布者",
    version: program.display_version ?? program.version ?? "—",
    size: formatBytes(program.size ?? program.estimated_size),
    installed: program.install_date ?? "—",
    location: program.install_location ?? "—",
    icon: program.name.slice(0, 2).toUpperCase(),
    iconClass: "generic",
    source: program,
  };
}

function formatBytes(bytes: number | null | undefined) {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 2)} ${units[unit]}`;
}

export default function App() {
  const [stage, setStage] = useState<Stage>(getInitialStage);
  const [activeNav, setActiveNav] = useState<NavKey>("apps");
  const [selectedId, setSelectedId] = useState("7zip");
  const [query, setQuery] = useState("");
  const [restorePoint, setRestorePoint] = useState(true);
  const [scanAfter, setScanAfter] = useState(true);
  const [progress, setProgress] = useState(42);
  const [logs, setLogs] = useState(initialLogs);
  const [traces, setTraces] = useState<Trace[]>(demoTraces);
  const [selectedTraceIds, setSelectedTraceIds] = useState<Set<string>>(
    () => new Set(demoTraces.filter((trace) => trace.confidence === "high").map((trace) => trace.id)),
  );
  const [cleanConfirm, setCleanConfirm] = useState(false);
  const [cleanResults, setCleanResults] = useState<CleanResult[]>([]);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [manualScanPending, setManualScanPending] = useState(false);
  const programs = useProgramsStore((state) => state.programs);
  const loading = useProgramsStore((state) => state.loading);
  const metadataLoading = useProgramsStore((state) => state.metadataLoading);
  const error = useProgramsStore((state) => state.error);
  const sourceFilter = useProgramsStore((state) => state.sourceFilter);
  const scannedTraces = useProgramsStore((state) => state.traces);
  const tracesLoading = useProgramsStore((state) => state.tracesLoading);
  const reloadPrograms = useProgramsStore((state) => state.reloadPrograms);
  const setSearchQuery = useProgramsStore((state) => state.setSearchQuery);
  const setSourceFilter = useProgramsStore((state) => state.setSourceFilter);
  const scanTraces = useProgramsStore((state) => state.scanTraces);
  const planUninstall = useProgramsStore((state) => state.planUninstall);
  const executeUninstall = useProgramsStore((state) => state.executeUninstall);
  const cleanUninstallResidues = useProgramsStore((state) => state.cleanUninstallResidues);
  const finishUninstall = useProgramsStore((state) => state.finishUninstall);
  const uninstallResult = useProgramsStore((state) => state.uninstallResult);
  const uninstallJob = useProgramsStore((state) => state.uninstallJob);
  const uninstallFailure = getUninstallFailureMessage(error, uninstallResult);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    const initialize = async () => {
      await reloadPrograms();
      if (cancelled) return;
    };
    void initialize();
    return () => {
      cancelled = true;
    };
  }, [reloadPrograms]);

  useEffect(() => {
    if (!manualScanPending || tracesLoading) return;
    setTraces(scannedTraces);
    setSelectedTraceIds(new Set(scannedTraces.filter((trace) => trace.confidence === "high").map((trace) => trace.id)));
    setManualScanPending(false);
    setStage("review");
  }, [manualScanPending, scannedTraces, tracesLoading]);

  useEffect(() => {
    if (programs.length === 0 || programs.some((program) => program.id === selectedId)) return;
    const preferred = programs.find((program) => /7-?zip/i.test(program.name)) ?? programs[0];
    if (preferred) setSelectedId(preferred.id);
  }, [programs, selectedId]);

  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const previewStages: Stage[] = ["apps", "confirm", "progress", "scan", "review", "complete"];
    const handlePreviewShortcut = (event: KeyboardEvent) => {
      if (!event.ctrlKey || !event.altKey) return;
      const index = Number(event.key) - 1;
      if (index >= 0 && index < previewStages.length) {
        event.preventDefault();
        setStage(previewStages[index]);
        setActiveNav("apps");
      }
    };
    window.addEventListener("keydown", handlePreviewShortcut);
    return () => window.removeEventListener("keydown", handlePreviewShortcut);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<UninstallJobEvent>("uninstall-job-progress", (event) => {
        const payload = event.payload;
        if (uninstallJob && payload.job_id !== uninstallJob.snapshot.job_id) return;
        if (payload.phase === "running_uninstaller") {
          setProgress(38);
          setLogs((current) => [...current, `[卸载] 已启动受控卸载器`]);
        } else if (payload.phase === "verifying_removal") {
          setProgress(72);
          setLogs((current) => [...current, `[验证] 正在确认程序是否已移除`]);
        } else if (payload.phase === "scanning_residues") {
          setProgress(86);
          setLogs((current) => [...current, `[扫描] 正在查找残留`]);
        } else if (payload.phase === "completed") {
          setProgress(100);
          setStage("complete");
        }
      }).then((fn) => {
        unlisten = fn;
      }),
    );
    return () => unlisten?.();
  }, [scanAfter, uninstallJob]);

  useEffect(() => {
    if (!isTauriRuntime() && stage === "progress") {
      const timer = window.setInterval(() => setProgress((value) => Math.min(value + 2, 84)), 650);
      return () => window.clearInterval(timer);
    }
  }, [stage]);

  useEffect(() => {
    if (uninstallResult?.success && !scanAfter) setStage("complete");
  }, [uninstallResult, scanAfter]);

  const sourcePrograms = isTauriRuntime()
    ? programs
    : mockPrograms
        .map((program) => program.source)
        .filter((program): program is InstalledProgram => Boolean(program));
  const visibleSourcePrograms = isTauriRuntime()
    ? filterPrograms(sourcePrograms, sourceFilter, query)
    : [];
  const allPrograms = isTauriRuntime() ? programs.map(toUiProgram) : mockPrograms;
  const filteredPrograms = isTauriRuntime()
    ? visibleSourcePrograms.map(toUiProgram)
    : allPrograms.filter((program) => `${program.name} ${program.publisher}`.toLowerCase().includes(query.toLowerCase()));
  const selectedProgram =
    filteredPrograms.find((program) => program.id === selectedId) ??
    filteredPrograms[0] ??
    allPrograms[0] ??
    mockPrograms[1];
  const completedProgram = uninstallJob
    ? toUiProgram(uninstallJob.snapshot.program)
    : selectedProgram;
  const sourceCounts = isTauriRuntime()
    ? countProgramsBySource(programs)
    : { all: mockPrograms.length, registry: mockPrograms.length, msi: 0, store: 0 };

  const handleSearch = (value: string) => {
    setQuery(value);
    if (isTauriRuntime()) {
      setSearchQuery(value);
    }
  };

  const handleManualScan = () => {
    setStage("scan");
    if (isTauriRuntime() && selectedProgram.source) {
      setManualScanPending(true);
      void scanTraces(selectedProgram.source.name);
    } else {
      setTraces(demoTraces);
      setSelectedTraceIds(new Set(demoTraces.filter((trace) => trace.confidence === "high").map((trace) => trace.id)));
      window.setTimeout(() => setStage("review"), 900);
    }
  };

  const startUninstall = () => {
    setStage("progress");
    setProgress(8);
    setLogs(["[10:24:31] 正在准备卸载任务…"]);
    if (isTauriRuntime() && selectedProgram.source) {
      void (async () => {
        const planned = await planUninstall(selectedProgram.source!.id);
        if (!planned) return;
        const executed = await executeUninstall(planned.snapshot.job_id, 120);
        if (!executed) return;
        const residueItems = executed.residue_review.traces;
        setTraces(residueItems);
        setSelectedTraceIds(new Set());
        if (executed.phase === "awaiting_cleanup_confirmation") setStage("review");
        else if (executed.phase === "completed") setStage("complete");
      })();
    } else {
      window.setTimeout(() => {
        setProgress(42);
        setLogs(initialLogs);
      }, 500);
    }
  };

  const continueDemo = () => {
    if (stage === "progress") setStage("scan");
    else if (stage === "scan") setStage("review");
  };

  const cleanSelected = async () => {
    const chosen = traces.filter((trace) => selectedTraceIds.has(trace.id));
    if (isTauriRuntime()) {
      const jobId = useProgramsStore.getState().uninstallJob?.snapshot.job_id;
      if (!jobId) return;
      const result = await cleanUninstallResidues(jobId, {
        trace_ids: chosen.map((trace) => trace.id),
        confirm: true,
      });
      if (!result) return;
    } else {
      setCleanResults(chosen.map((trace) => ({ trace_id: trace.id, path: trace.path, success: true, error: null, bytes_freed: trace.size ?? 0 })));
    }
    setCleanConfirm(false);
    setStage("complete");
  };

  return (
    <div className={`app-frame ${isTauriRuntime() ? "native-frame" : ""}`}>
      {!isTauriRuntime() && <TitleBar />}
      <div className="app-body">
        <Sidebar active={activeNav} onNavigate={(next) => { setActiveNav(next); if (next === "apps") setStage("apps"); }} />
        <main className="workspace">
          {activeNav === "startup" ? (
            <StartupManager />
          ) : activeNav === "cleaner" ? (
            <CleanerPage />
          ) : activeNav === "plugins" ? (
            <BrowserPluginsPage />
          ) : activeNav !== "apps" ? (
            <PlaceholderPage active={activeNav} />
          ) : stage === "apps" ? (
            <AppsStage
              programs={filteredPrograms}
              selected={selectedProgram}
              selectedId={selectedProgram.id}
              loading={loading}
              metadataLoading={metadataLoading}
              error={error}
              query={query}
              sourceFilter={sourceFilter}
              sourceCounts={sourceCounts}
              onQuery={handleSearch}
              onSourceFilter={setSourceFilter}
              onSelect={setSelectedId}
              onUninstall={() => setStage("confirm")}
              onScan={handleManualScan}
              onDetails={() => setDetailsOpen(true)}
              onRefresh={() => isTauriRuntime() && void reloadPrograms({ refresh: true })}
            />
          ) : stage === "confirm" ? (
            <ConfirmStage
              program={selectedProgram}
              restorePoint={restorePoint}
              scanAfter={scanAfter}
              onRestorePoint={setRestorePoint}
              onScanAfter={setScanAfter}
              onCancel={() => setStage("apps")}
              onStart={startUninstall}
            />
          ) : stage === "progress" ? (
            <ProgressStage program={selectedProgram} progress={progress} logs={logs} error={uninstallFailure} onContinue={continueDemo} />
          ) : stage === "scan" ? (
            <ScanStage onContinue={continueDemo} />
          ) : stage === "review" ? (
            <ReviewStage
              traces={traces}
              selectedIds={selectedTraceIds}
              onToggle={(id) => setSelectedTraceIds((current) => {
                const next = new Set(current);
                if (next.has(id)) next.delete(id); else next.add(id);
                return next;
              })}
              allowBack={!uninstallJob}
              onBack={() => setStage("apps")}
              onSkip={() => {
                const jobId = useProgramsStore.getState().uninstallJob?.snapshot.job_id;
                if (jobId) void finishUninstall(jobId).then(() => setStage("complete"));
              }}
              onRescan={handleManualScan}
              onClean={() => setCleanConfirm(true)}
              confirmOpen={cleanConfirm}
              onConfirmClose={() => setCleanConfirm(false)}
              onConfirm={() => void cleanSelected()}
            />
          ) : (
            <CompleteStage
              program={completedProgram}
              results={cleanResults}
              onDone={() => { setStage("apps"); setCleanResults([]); }}
            />
          )}
        </main>
      </div>
      {detailsOpen && <ProgramInfoModal program={selectedProgram} onClose={() => setDetailsOpen(false)} />}
    </div>
  );
}

function TitleBar() {
  const act = async (name: "minimize" | "maximize" | "close") => {
    if (!isTauriRuntime()) return;
    const appWindow = getCurrentWindow();
    try {
      if (name === "minimize") await appWindow.minimize();
      else if (name === "maximize") await appWindow.toggleMaximize();
      else await appWindow.close();
    } catch (error) {
      console.error(`窗口操作失败: ${name}`, error);
    }
  };

  const startDragging = async (event: React.MouseEvent<HTMLElement>) => {
    if (!isTauriRuntime() || event.button !== 0) return;
    if ((event.target as HTMLElement).closest(".window-actions")) return;
    try {
      await getCurrentWindow().startDragging();
    } catch (error) {
      console.error("启动窗口拖动失败", error);
    }
  };

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onMouseDown={(event) => void startDragging(event)}
      onDoubleClick={() => void act("maximize")}
    >
      <div className="brand" data-tauri-drag-region>
        <span className="brand-mark"><ShieldCheck size={16} /></span>
        <span>RustYu</span>
      </div>
      <div className="window-actions">
        <button aria-label="最小化" onClick={() => void act("minimize")}><Minus size={15} /></button>
        <button aria-label="最大化" onClick={() => void act("maximize")}><Maximize2 size={12} /></button>
        <button aria-label="关闭" className="close" onClick={() => void act("close")}><X size={15} /></button>
      </div>
    </header>
  );
}

const navItems: { id: NavKey; label: string; icon: typeof Package }[] = [
  { id: "apps", label: "已安装应用", icon: Grid2X2 },
  { id: "startup", label: "自启动管理", icon: Zap },
  { id: "cleaner", label: "系统清理", icon: Sparkles },
  { id: "traces", label: "软件残留", icon: Archive },
  { id: "monitor", label: "安装监控", icon: SquareActivity },
  { id: "plugins", label: "浏览器插件", icon: AppWindow },
  { id: "tools", label: "工具箱", icon: Wrench },
];

function Sidebar({ active, onNavigate }: { active: NavKey; onNavigate: (key: NavKey) => void }) {
  return (
    <aside className="sidebar">
      <nav>
        {navItems.map((item) => <NavButton key={item.id} {...item} active={active === item.id} onClick={() => onNavigate(item.id)} />)}
      </nav>
      <nav className="sidebar-bottom">
        <NavButton id="settings" label="设置" icon={Settings} active={active === "settings"} onClick={() => onNavigate("settings")} />
        <NavButton id="about" label="关于" icon={CircleHelp} active={active === "about"} onClick={() => onNavigate("about")} />
      </nav>
    </aside>
  );
}

function NavButton({ label, icon: Icon, active, onClick }: { id: NavKey; label: string; icon: typeof Package; active: boolean; onClick: () => void }) {
  return <button className={`nav-button ${active ? "active" : ""}`} onClick={onClick}><Icon size={17} /><span>{label}</span></button>;
}

function SectionHeader({ title, subtitle, action }: { title: string; subtitle?: string; action?: React.ReactNode }) {
  return <div className="section-header"><div><h1>{title}</h1>{subtitle && <p>{subtitle}</p>}</div>{action}</div>;
}

function AppsStage(props: {
  programs: UiProgram[]; selected: UiProgram; selectedId: string; loading: boolean; metadataLoading: boolean;
  error: string | null; query: string; sourceFilter: ProgramSourceFilter;
  sourceCounts: Record<ProgramSourceFilter, number>;
  onQuery: (value: string) => void; onSourceFilter: (value: ProgramSourceFilter) => void; onSelect: (id: string) => void;
  onUninstall: () => void; onScan: () => void; onDetails: () => void; onRefresh: () => void;
}) {
  return (
    <div className="page apps-page">
      <SectionHeader title="已安装应用" action={<button className="icon-button" disabled={props.loading || props.metadataLoading} title={props.metadataLoading ? "正在生成图标缓存" : "刷新"} onClick={props.onRefresh}><RefreshCw className={props.loading || props.metadataLoading ? "spinning" : ""} size={17} /></button>} />
      <div className="toolbar">
        <label className="search-box"><Search size={16} /><input value={props.query} onChange={(event) => props.onQuery(event.target.value)} placeholder="搜索应用、发布者或关键词…" /></label>
        <div className="filter-pills">
          {programSourceOptions.map((option) => (
            <button key={option.id} className={props.sourceFilter === option.id ? "selected" : ""} onClick={() => props.onSourceFilter(option.id)}>
              {option.label}<span>{props.sourceCounts[option.id]}</span>
            </button>
          ))}
        </div>
      </div>
      <div className="apps-layout">
        <div className="program-table card-surface">
          <div className="table-head"><span>名称 <ChevronDown size={12} /></span><span>发布者</span><span>大小</span><span>安装日期 <ChevronDown size={12} /></span></div>
          <div className="table-body">
            {props.error ? <div className="table-message error">{props.error}</div> : props.loading && props.programs.length === 0 ? <div className="table-message">正在读取已安装程序…</div> : props.programs.length === 0 ? <div className="table-message">没有找到符合条件的程序</div> : props.programs.map((program) => (
              <button key={program.id} className={`program-row ${props.selectedId === program.id ? "selected" : ""}`} onClick={() => props.onSelect(program.id)}>
                <span className="program-name"><AppIcon program={program} /><strong>{program.name}</strong></span>
                <span>{program.publisher}</span><span>{program.size}</span><span>{program.installed}</span>
              </button>
            ))}
          </div>
          <div className="table-footer">显示 {props.programs.length} 个应用</div>
        </div>
        <aside className="program-detail card-surface">
          <div className="detail-app"><AppIcon program={props.selected} large /><h2>{props.selected.name}</h2><p>{props.selected.publisher}</p><span>{props.selected.size}</span></div>
          <dl><div><dt>版本</dt><dd>{props.selected.version}</dd></div><div><dt>安装日期</dt><dd>{props.selected.installed}</dd></div><div><dt>启动次数</dt><dd>82</dd></div><div><dt>启动项</dt><dd>无</dd></div></dl>
          <p className="install-path">{props.selected.location}</p>
          <div className="detail-actions">
            <button onClick={props.onDetails}><Info size={14} />详细信息</button>
            <button onClick={props.onScan}><Search size={14} />扫描残留</button>
          </div>
          <button className="primary-button uninstall-button" onClick={props.onUninstall}><Trash2 size={16} />卸载</button>
        </aside>
      </div>
    </div>
  );
}

function AppIcon({ program, large = false }: { program: UiProgram; large?: boolean }) {
  const iconSrc = program.source ? getProgramIconSrc(program.source) : null;
  const storeIconClass = program.source?.install_source === "store" ? " store-icon" : "";
  if (iconSrc) return <img className={`app-icon${large ? " large" : ""}${storeIconClass}`} src={iconSrc} alt="" />;
  return <span className={`app-icon ${large ? "large" : ""} ${program.iconClass}`}>{program.icon}</span>;
}

function ConfirmStage(props: {
  program: UiProgram; restorePoint: boolean; scanAfter: boolean;
  onRestorePoint: (value: boolean) => void; onScanAfter: (value: boolean) => void; onCancel: () => void; onStart: () => void;
}) {
  return (
    <div className="page focused-page">
      <SectionHeader title="确认卸载" subtitle="请在继续前检查卸载范围和安全选项" />
      <div className="confirm-card card-surface">
        <div className="confirm-program"><AppIcon program={props.program} large /><div><h2>{props.program.name}</h2><p>{props.program.publisher}<span>·</span>{props.program.size}</p></div></div>
        <div className="warning-banner"><TriangleAlert size={21} /><div><strong>检测到中等风险</strong><p>程序包含系统级组件。建议创建系统还原点，以便出现问题时恢复。</p></div></div>
        <div className="risk-box"><h3>风险影响</h3><div><span>程序文件将被删除</span><b className="risk-medium">中等</b></div><div><span>注册表项将被修改</span><b className="risk-medium">中等</b></div><div><span>可能影响其他程序</span><b className="risk-low">低</b></div><div><span>个人数据影响</span><b>无</b></div></div>
        <div className="option-list"><CheckOption checked={props.restorePoint} onChange={props.onRestorePoint} title="创建系统还原点" hint="推荐" /><CheckOption checked={props.scanAfter} onChange={props.onScanAfter} title="卸载后扫描残留" hint="推荐" /></div>
        <div className="dialog-actions"><button className="secondary-button" onClick={props.onCancel}>取消</button><button className="primary-button" onClick={props.onStart}><Play size={15} fill="currentColor" />开始卸载</button></div>
      </div>
    </div>
  );
}

function CheckOption({ checked, onChange, title, hint }: { checked: boolean; onChange: (value: boolean) => void; title: string; hint: string }) {
  return <label className="check-option"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><span className="fake-check"><Check size={13} /></span><span>{title}<small>（{hint}）</small></span></label>;
}

function ProgressStage({ program, progress, logs, error, onContinue }: { program: UiProgram; progress: number; logs: string[]; error: string | null; onContinue: () => void }) {
  const nativeWorkflow = isTauriRuntime();
  const failed = Boolean(error);
  const steps = [
    ["创建系统还原点", "done"], ["分析卸载程序", "done"], ["运行内置卸载程序", failed ? "failed" : "active"], ["删除程序文件", "todo"], ["清理注册表项", "todo"], ["卸载完成", "todo"],
  ];
  return (
    <div className="page progress-page">
      <SectionHeader title={failed ? `${program.name} 卸载失败` : `正在卸载 ${program.name}`} subtitle={failed ? "卸载没有继续，请查看下方错误信息。" : "请保持应用开启，我们会在任务完成后通知你"} />
      <div className="progress-top">
        <div className={`progress-ring ${failed ? "failed" : nativeWorkflow ? "indeterminate" : "preview-progress"}`}><div><strong>{failed ? "!" : nativeWorkflow ? "…" : `${progress}%`}</strong><span>{failed ? "卸载失败" : nativeWorkflow ? "等待真实卸载状态…" : "正在删除文件…"}</span><small>{failed ? "请查看下方错误信息" : nativeWorkflow ? "请保持窗口开启" : "00:00:28"}</small></div></div>
        <div className="steps-list">{steps.map(([label, state]) => <div key={label} className={`step ${state}`}><span>{state === "done" ? <Check size={12} /> : state === "failed" ? <TriangleAlert size={11} /> : ""}</span><strong>{label}</strong><em>{state === "done" ? "完成" : state === "active" ? "进行中" : state === "failed" ? "失败" : ""}</em></div>)}</div>
      </div>
      <div className={`log-panel card-surface${failed ? " error-state" : ""}`}>
        <div className="panel-title"><span>实时日志</span><button>清空</button></div>
        {error && <div className="uninstall-error" role="alert"><TriangleAlert size={16} /><div><strong>卸载失败</strong><span>{error}</span></div></div>}
        <div className="log-content">{error && <div className="error-log">[错误] {error}</div>}{logs.map((log, index) => <div key={`${log}-${index}`}>{log}</div>)}</div>
      </div>
      <div className="page-footnote"><Info size={15} /><span>该过程可能需要一些时间，请耐心等待。</span>{!isTauriRuntime() && <button onClick={onContinue}>预览下一阶段</button>}</div>
    </div>
  );
}

function ScanStage({ onContinue }: { onContinue: () => void }) {
  const locations = [
    { icon: Folder, title: "程序文件 (Program Files)", path: "C:\\Program Files\\", count: "1,248 项" },
    { icon: Box, title: "用户数据 (AppData)", path: "%LOCALAPPDATA%\\", count: "632 项" },
    { icon: Database, title: "注册表 (Registry)", path: "HKEY_CURRENT_USER", count: "487 项" },
  ];
  return (
    <div className="page scan-page">
      <SectionHeader title="扫描残留" subtitle="正在保守地查找与该程序明确关联的项目" />
      <div className="scan-main">
        <div className="radar"><div className="radar-sweep" /><span className="radar-dot one" /><span className="radar-dot two" /><span className="radar-dot three" /></div>
        <div className="scan-locations"><h3>扫描位置</h3>{locations.map(({ icon: Icon, title, path, count }) => <div className="scan-location" key={title}><Icon size={20} /><div><strong>{title}</strong><span>{path}</span></div><em>正在扫描…</em><b>{count}</b></div>)}</div>
      </div>
      <div className="scan-stats card-surface"><h3>扫描统计</h3><div><span>已扫描项<strong>1,842</strong></span><span>发现项<strong className="blue">正在统计…</strong></span><span>预计大小<strong className="blue">正在计算…</strong></span><span>用时<strong>00:00:15</strong></span></div></div>
      <div className="page-footnote"><Info size={15} /><span>我们仅显示与已卸载程序相关的项目，系统关键项将被自动排除。</span>{!isTauriRuntime() && <button onClick={onContinue}>查看扫描结果</button>}</div>
    </div>
  );
}

function ReviewStage(props: {
  traces: Trace[]; selectedIds: Set<string>; onToggle: (id: string) => void; onBack: () => void; onClean: () => void;
  allowBack: boolean; onSkip: () => void; onRescan: () => void; confirmOpen: boolean; onConfirmClose: () => void; onConfirm: () => void;
}) {
  const fileTraces = props.traces.filter((trace) => trace.trace_type !== "registry_key" && trace.trace_type !== "registry_value");
  const registryTraces = props.traces.filter((trace) => trace.trace_type === "registry_key" || trace.trace_type === "registry_value");
  const selectedSize = props.traces.filter((trace) => props.selectedIds.has(trace.id)).reduce((sum, trace) => sum + (trace.size ?? 0), 0);
  return (
    <div className="page review-page">
      <SectionHeader title="检查残留项" subtitle={`发现 ${props.traces.length} 个项目（共 ${formatBytes(props.traces.reduce((sum, trace) => sum + (trace.size ?? 0), 0))}），已选择 ${props.selectedIds.size} 个项目`} action={<button className="link-button" onClick={props.onRescan}><RotateCcw size={14} />重新扫描</button>} />
      <div className="review-tabs"><button className="active">全部 ({props.traces.length})</button><button>文件系统 ({fileTraces.length})</button><button>注册表 ({registryTraces.length})</button></div>
      <div className="trace-table card-surface">
        <TraceGroup title={`文件系统 (${fileTraces.length} 项)`} traces={fileTraces} selectedIds={props.selectedIds} onToggle={props.onToggle} />
        <TraceGroup title={`注册表 (${registryTraces.length} 项)`} traces={registryTraces} selectedIds={props.selectedIds} onToggle={props.onToggle} />
      </div>
      <div className="review-footer"><span>可回收空间：<strong>{formatBytes(selectedSize)}</strong></span><div><button className="secondary-button" onClick={props.onSkip}>跳过清理</button>{props.allowBack && <button className="secondary-button" onClick={props.onBack}>返回</button>}<button className="primary-button" disabled={props.selectedIds.size === 0} onClick={props.onClean}><Trash2 size={16} />清理所选</button></div></div>
      {props.confirmOpen && <div className="modal-backdrop"><div className="safety-modal"><span className="modal-icon"><TriangleAlert size={24} /></span><h2>确认清理所选残留？</h2><p>将永久删除 {props.selectedIds.size} 个已确认项目。低置信度项目不会被自动选择，此操作无法自动撤销。</p><div><button className="secondary-button" onClick={props.onConfirmClose}>取消</button><button className="danger-button" onClick={props.onConfirm}>确认清理</button></div></div></div>}
    </div>
  );
}

function TraceGroup({ title, traces, selectedIds, onToggle }: { title: string; traces: Trace[]; selectedIds: Set<string>; onToggle: (id: string) => void }) {
  return <section className="trace-group"><h3>{title}</h3><div className="trace-head"><span /><span>名称 / 路径</span><span>大小</span><span>置信度</span></div>{traces.map((trace) => <label className="trace-row" key={trace.id}><input type="checkbox" checked={selectedIds.has(trace.id)} onChange={() => onToggle(trace.id)} /><span className="fake-check"><Check size={12} /></span><TraceIcon type={trace.trace_type} /><span className="trace-name"><strong>{trace.description ?? "残留项目"}</strong><small>{trace.path}</small></span><span>{formatBytes(trace.size)}</span><ConfidenceBadge value={trace.confidence} /></label>)}</section>;
}

function TraceIcon({ type }: { type: Trace["trace_type"] }) {
  const Icon = type === "registry_key" || type === "registry_value" ? Database : type === "shortcut" ? FileCode2 : Folder;
  return <span className="trace-icon"><Icon size={15} /></span>;
}

function ConfidenceBadge({ value }: { value: Trace["confidence"] }) {
  const label = value === "high" ? "高置信度" : value === "medium" ? "中置信度" : "低置信度";
  return <span className={`confidence ${value}`}>{label}</span>;
}

function CompleteStage({ program, results, onDone }: { program: UiProgram; results: CleanResult[]; onDone: () => void }) {
  const removed = results.length || 56;
  const freed = results.length ? results.reduce((sum, result) => sum + result.bytes_freed, 0) : 18_900_000;
  return (
    <div className="page complete-page">
      <SectionHeader title="卸载完成" />
      <div className="success-heading"><span><Check size={36} /></span><div><h2>{program.name} 已成功卸载</h2><p>所有选定项目已清理完成。</p></div></div>
      <div className="summary-grid card-surface"><div><span>释放空间</span><strong className="green">{formatBytes(freed)}</strong></div><div><span>删除项目</span><strong className="blue">{removed} 个</strong></div><div><span>跳过项</span><strong className="orange">22 个</strong></div><div><span>用时</span><strong>00:01:24</strong></div></div>
      <div className="report-panel card-surface"><div className="panel-title"><span>详细报告</span><button>导出日志</button></div><div className="log-content"><div>[10:24:31] 开始卸载：{program.name}</div><div>[10:24:33] 创建系统还原点成功</div><div>[10:24:50] 扫描完成，发现 78 个残留项目（24.6 MB）</div><div>[10:25:02] 清理完成，删除 {removed} 个项目</div><div>[10:25:05] 操作完成</div></div></div>
      <div className="complete-actions"><button className="secondary-button"><FileText size={16} />查看报告</button><button className="primary-button" onClick={onDone}><Check size={16} />完成</button></div>
    </div>
  );
}

function ProgramInfoModal({ program, onClose }: { program: UiProgram; onClose: () => void }) {
  const source = program.source;
  const sourceLabel = source?.install_source === "msi" ? "MSI" : source?.install_source === "store" ? "Microsoft Store" : source?.install_source === "registry" ? "注册表" : "未知";
  const rows: Array<[string, string | null | undefined, boolean?]> = [
    ["程序 ID", source?.id ?? program.id, true],
    ["发布者", source?.publisher ?? program.publisher],
    ["版本", source?.display_version ?? source?.version ?? program.version],
    ["安装来源", sourceLabel],
    ["卸载类型", source?.uninstall_kind],
    ["安装日期", source?.install_date ?? program.installed],
    ["程序大小", program.size],
    ["安装位置", source?.install_location ?? program.location, true],
    ["卸载命令", source?.uninstall_string, true],
    ["静默卸载", source?.quiet_uninstall_string, true],
    ["注册表路径", source?.uninstall_registry_key_path, true],
  ];
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section className="program-info-modal" onMouseDown={(event) => event.stopPropagation()}>
        <header>
          <div><AppIcon program={program} large /><span><h2>{program.name}</h2><p>{program.publisher}</p></span></div>
          <button aria-label="关闭详细信息" onClick={onClose}><X size={17} /></button>
        </header>
        <div className="program-info-rows">
          {rows.filter(([, value]) => Boolean(value)).map(([label, value, mono]) => (
            <div key={label}><dt>{label}</dt><dd className={mono ? "mono" : ""}>{value}</dd></div>
          ))}
        </div>
        <footer>
          {source?.install_location && <a href={`file:///${source.install_location}`} target="_blank" rel="noreferrer"><FolderOpen size={15} />打开安装目录</a>}
          <button className="secondary-button" onClick={onClose}>关闭</button>
        </footer>
      </section>
    </div>
  );
}

function PlaceholderPage({ active }: { active: NavKey }) {
  const names: Record<NavKey, string> = { apps: "已安装应用", startup: "自启动管理", cleaner: "系统清理", traces: "软件残留", monitor: "安装监控", plugins: "浏览器插件", tools: "工具箱", settings: "设置", about: "关于" };
  return <div className="placeholder-page"><span><Sparkles size={30} /></span><h1>{names[active]}</h1><p>该模块将在后续版本中提供。</p></div>;
}
