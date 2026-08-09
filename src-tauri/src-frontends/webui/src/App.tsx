import { useEffect, useRef, useState } from "react";
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
  ListChecks,
  Loader2,
  Maximize2,
  Minus,
  Pause,
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
import { useForceUninstallStore } from "./stores/forceUninstall";
import { useBatchUninstallStore } from "./stores/batchUninstall";
import { StartupManager } from "./components/StartupManager";
import { CleanerPage } from "./components/CleanerPage";
import { BrowserPluginsPage } from "./components/BrowserPluginsPage";
import { getUninstallFailureMessage } from "./components/uninstall/uninstallFeedback";
import {
  formatTraceType,
  formatUninstallEventLog,
  summarizeTraces,
} from "./components/uninstall/uninstallReport";
import type {
  BatchUninstallItem,
  CleanResult,
  InstalledProgram,
  Trace,
  UninstallJobEvent,
  UninstallJob,
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
  if (bytes === null || bytes === undefined) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 2)} ${units[unit]}`;
}

function formatLocalLog(message: string) {
  return `[${new Date().toLocaleTimeString("zh-CN", { hour12: false })}] ${message}`;
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
  const [scanStatus, setScanStatus] = useState<"idle" | "scanning" | "complete">("idle");
  const scanStartedAtRef = useRef<number | null>(null);
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
  const forcePlan = useForceUninstallStore((state) => state.plan);
  const forceResult = useForceUninstallStore((state) => state.result);
  const forceLoading = useForceUninstallStore((state) => state.loading);
  const forceError = useForceUninstallStore((state) => state.error);
  const planForceTarget = useForceUninstallStore((state) => state.planTarget);
  const cleanForceSelected = useForceUninstallStore((state) => state.cleanSelected);
  const resetForce = useForceUninstallStore((state) => state.reset);
  const batchItems = useBatchUninstallStore((state) => state.items);
  const batchActive = useBatchUninstallStore((state) => state.active);
  const batchPaused = useBatchUninstallStore((state) => state.paused);
  const batchError = useBatchUninstallStore((state) => state.error);
  const startBatchQueue = useBatchUninstallStore((state) => state.startQueue);
  const pauseBatchQueue = useBatchUninstallStore((state) => state.pauseQueue);
  const resumeBatchQueue = useBatchUninstallStore((state) => state.resumeQueue);
  const cancelBatchQueue = useBatchUninstallStore((state) => state.cancelQueue);
  const resetBatchQueue = useBatchUninstallStore((state) => state.reset);
  const [batchSelectedIds, setBatchSelectedIds] = useState<Set<string>>(new Set());
  const [batchOpen, setBatchOpen] = useState(false);
  const [forceOpen, setForceOpen] = useState(false);

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
    setScanStatus("complete");
    setStage("review");
  }, [manualScanPending, scannedTraces, tracesLoading]);

  useEffect(() => {
    if (programs.length === 0 || programs.some((program) => program.id === selectedId)) return;
    const preferred = programs.find((program) => /7-?zip/i.test(program.name)) ?? programs[0];
    if (preferred) setSelectedId(preferred.id);
  }, [programs, selectedId]);

  useEffect(() => {
    const available = new Set(programs.map((program) => program.id));
    setBatchSelectedIds((current) => {
      const next = new Set([...current].filter((id) => available.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [programs]);

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
        setLogs((current) => [...current, formatUninstallEventLog(payload)]);
        if (payload.phase === "running_uninstaller") {
          setProgress(38);
        } else if (payload.phase === "verifying_removal") {
          setProgress(72);
        } else if (payload.phase === "scanning_residues") {
          setProgress(86);
          setScanStatus("scanning");
          scanStartedAtRef.current = Date.now();
          setLogs((current) => [
            ...current,
            formatLocalLog("[残留扫描] 检查安装目录、程序文件与快捷方式"),
            formatLocalLog("[残留扫描] 检查 APPDATA / LOCALAPPDATA 用户数据"),
            formatLocalLog("[残留扫描] 检查 HKCU / HKLM 注册表与文件关联"),
            formatLocalLog("[残留扫描] 检查明确关联的服务、任务与系统集成"),
          ]);
          setStage("scan");
        } else if (payload.phase === "awaiting_cleanup_confirmation") {
          setScanStatus("complete");
        } else if (payload.phase === "cleaning_residues") {
          setProgress(94);
        } else if (payload.phase === "failed" || payload.phase === "cancelled") {
          setScanStatus("complete");
          setStage("progress");
        } else if (payload.phase === "completed") {
          setProgress(100);
        }
      }).then((fn) => {
        unlisten = fn;
      }),
    );
    return () => unlisten?.();
  }, [uninstallJob]);

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
  const selectedBatchPrograms = sourcePrograms.filter((program) => batchSelectedIds.has(program.id));

  const toggleBatchSelection = (programId: string) => {
    if (batchActive) return;
    setBatchSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(programId)) next.delete(programId);
      else next.add(programId);
      return next;
    });
  };

  const openBatchUninstall = () => {
    if (batchActive || batchPaused) {
      setBatchOpen(true);
      return;
    }
    if (batchSelectedIds.size === 0) return;
    resetBatchQueue();
    setBatchOpen(true);
  };

  const startBatchUninstall = async () => {
    if (!isTauriRuntime() || selectedBatchPrograms.length === 0) return;
    await startBatchQueue(selectedBatchPrograms);
    if (useBatchUninstallStore.getState().items.some((item) => item.status === "completed")) {
      void reloadPrograms({ refresh: true });
    }
  };

  const closeBatchUninstall = () => {
    setBatchOpen(false);
    if (!batchActive && !batchPaused) {
      resetBatchQueue();
      setBatchSelectedIds(new Set());
    }
  };

  const closeForceUninstall = () => {
    if (forceResult?.success) void reloadPrograms({ refresh: true });
    setForceOpen(false);
    resetForce();
  };

  const handleSearch = (value: string) => {
    setQuery(value);
    if (isTauriRuntime()) {
      setSearchQuery(value);
    }
  };

  const handleManualScan = () => {
    setStage("scan");
    setTraces([]);
    setScanStatus("scanning");
    scanStartedAtRef.current = Date.now();
    if (isTauriRuntime() && selectedProgram.source) {
      setManualScanPending(true);
      void scanTraces(selectedProgram.source.name);
    } else {
      setTraces(demoTraces);
      setSelectedTraceIds(new Set(demoTraces.filter((trace) => trace.confidence === "high").map((trace) => trace.id)));
      window.setTimeout(() => {
        setScanStatus("complete");
        setStage("review");
      }, 900);
    }
  };

  const startUninstall = () => {
    setStage("progress");
    setProgress(8);
    setTraces([]);
    setLogs([`[${new Date().toLocaleTimeString("zh-CN", { hour12: false })}] [准备] 正在生成卸载快照…`]);
    setScanStatus("idle");
    scanStartedAtRef.current = null;
    if (isTauriRuntime() && selectedProgram.source) {
      void (async () => {
        const planned = await planUninstall(selectedProgram.source!.id);
        if (!planned) return;
        const executed = await executeUninstall(planned.snapshot.job_id, 120);
        if (!executed) return;
        const residueItems = executed.residue_review.traces;
        setTraces(residueItems);
        setSelectedTraceIds(new Set());
        const categoryLabels = { files: "文件系统", user_data: "用户数据", registry: "注册表", system: "系统集成" };
        const scanSummaryLogs = summarizeTraces(residueItems)
          .filter((summary) => summary.count > 0)
          .map((summary) => formatLocalLog(`[扫描结果] ${categoryLabels[summary.category]}：${summary.count} 项，${formatBytes(summary.bytes)}`));
        setLogs((current) => [...current, ...scanSummaryLogs]);
        setScanStatus("complete");
        const scanElapsed = scanStartedAtRef.current === null ? 1_000 : Date.now() - scanStartedAtRef.current;
        const revealResults = () => {
          if (executed.phase === "awaiting_cleanup_confirmation") setStage("review");
          else if (executed.phase === "completed") setStage("complete");
        };
        window.setTimeout(revealResults, Math.max(0, 850 - scanElapsed));
      })();
    } else {
      window.setTimeout(() => {
        setProgress(42);
        setLogs(initialLogs);
      }, 500);
    }
  };

  const continueDemo = () => {
    if (stage === "progress") {
      setScanStatus("scanning");
      setStage("scan");
    } else if (stage === "scan") {
      setScanStatus("complete");
      setStage("review");
    }
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
      if (result.phase === "failed") {
        setStage("progress");
        return;
      }
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
              onForceUninstall={() => { resetForce(); setForceOpen(true); }}
              batchSelectedIds={batchSelectedIds}
              batchActive={batchActive}
              onToggleBatch={toggleBatchSelection}
              onOpenBatch={openBatchUninstall}
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
            <ScanStage program={selectedProgram} traces={traces} logs={logs} status={scanStatus} onContinue={continueDemo} />
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
              traces={traces}
              job={uninstallJob}
              logs={logs}
              onDone={() => { setStage("apps"); setCleanResults([]); }}
            />
          )}
        </main>
      </div>
      {detailsOpen && <ProgramInfoModal program={selectedProgram} onClose={() => setDetailsOpen(false)} />}
      {batchOpen && <BatchUninstallModal
        selectedPrograms={selectedBatchPrograms}
        items={batchItems}
        active={batchActive}
        paused={batchPaused}
        error={batchError}
        onStart={() => void startBatchUninstall()}
        onPause={pauseBatchQueue}
        onResume={resumeBatchQueue}
        onCancel={cancelBatchQueue}
        onClose={closeBatchUninstall}
      />}
      {forceOpen && <ForceUninstallModal
        initialPath={selectedProgram.source?.install_location ?? ""}
        plan={forcePlan}
        result={forceResult}
        loading={forceLoading}
        error={forceError}
        onPlan={planForceTarget}
        onClean={cleanForceSelected}
        onClose={closeForceUninstall}
      />}
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
  onForceUninstall: () => void;
  batchSelectedIds: Set<string>; batchActive: boolean; onToggleBatch: (id: string) => void; onOpenBatch: () => void;
}) {
  return (
    <div className="page apps-page">
      <SectionHeader title="已安装应用" action={<div className="header-actions"><button className="secondary-button compact-button" disabled={!props.batchActive && props.batchSelectedIds.size === 0} onClick={props.onOpenBatch}><ListChecks size={14} />{props.batchActive ? "查看队列" : `批量卸载${props.batchSelectedIds.size ? ` (${props.batchSelectedIds.size})` : ""}`}</button><button className="secondary-button compact-button" onClick={props.onForceUninstall}><Wrench size={14} />强制卸载</button><button className="icon-button" disabled={props.loading || props.metadataLoading} title={props.metadataLoading ? "正在生成图标缓存" : "刷新"} onClick={props.onRefresh}><RefreshCw className={props.loading || props.metadataLoading ? "spinning" : ""} size={17} /></button></div>} />
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
          <div className="table-head"><span>选择</span><span>名称 <ChevronDown size={12} /></span><span>发布者</span><span>大小</span><span>安装日期 <ChevronDown size={12} /></span></div>
          <div className="table-body">
            {props.error ? <div className="table-message error">{props.error}</div> : props.loading && props.programs.length === 0 ? <div className="table-message">正在读取已安装程序…</div> : props.programs.length === 0 ? <div className="table-message">没有找到符合条件的程序</div> : props.programs.map((program) => (
              <div key={program.id} className={`program-row ${props.selectedId === program.id ? "selected" : ""}`} role="button" tabIndex={0} aria-selected={props.selectedId === program.id} onClick={() => props.onSelect(program.id)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); props.onSelect(program.id); } }}>
                <span className="program-select"><input type="checkbox" aria-label={`选择 ${program.name}`} checked={props.batchSelectedIds.has(program.id)} disabled={props.batchActive} onClick={(event) => event.stopPropagation()} onChange={() => props.onToggleBatch(program.id)} /></span>
                <span className="program-name"><AppIcon program={program} /><strong>{program.name}</strong></span>
                <span>{program.publisher}</span><span>{program.size}</span><span>{program.installed}</span>
              </div>
            ))}
          </div>
          <div className="table-footer">显示 {props.programs.length} 个应用{props.batchSelectedIds.size > 0 ? ` · 已选择 ${props.batchSelectedIds.size} 个` : ""}</div>
        </div>
        <aside className="program-detail card-surface">
          <div className="detail-app"><AppIcon program={props.selected} large /><h2>{props.selected.name}</h2><p>{props.selected.publisher}</p><span>{props.selected.size}</span></div>
          <dl><div><dt>版本</dt><dd>{props.selected.version}</dd></div><div><dt>安装日期</dt><dd>{props.selected.installed}</dd></div><div><dt>启动次数</dt><dd>82</dd></div><div><dt>启动项</dt><dd>无</dd></div></dl>
          <p className="install-path">{props.selected.location}</p>
          <div className="detail-actions">
            <button onClick={props.onDetails}><Info size={14} />详细信息</button>
            <button onClick={props.onScan}><Search size={14} />扫描残留</button>
            <button onClick={props.onForceUninstall}><Wrench size={14} />强制卸载</button>
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

function ScanStage({
  program,
  traces,
  logs,
  status,
  onContinue,
}: {
  program: UiProgram;
  traces: Trace[];
  logs: string[];
  status: "idle" | "scanning" | "complete";
  onContinue: () => void;
}) {
  const summaries = summarizeTraces(traces);
  const locationMeta = [
    { category: "files" as const, icon: Folder, title: "程序文件", path: program.location || "Program Files / 安装目录", hint: "安装目录、缓存与快捷方式" },
    { category: "user_data" as const, icon: Box, title: "用户数据", path: "%APPDATA% · %LOCALAPPDATA%", hint: "配置、缓存与用户级数据" },
    { category: "registry" as const, icon: Database, title: "注册表", path: "HKCU · HKLM\\Software", hint: "卸载项、设置与文件关联" },
    { category: "system" as const, icon: ShieldCheck, title: "系统集成", path: "服务 · 任务 · 驱动", hint: "仅检查明确关联的系统项" },
  ];
  const isComplete = status === "complete";
  const latestLogs = logs.slice(-4);
  return (
    <div className={`page scan-page ${isComplete ? "scan-complete" : "scan-running"}`}>
      <SectionHeader title={isComplete ? "扫描完成" : "正在扫描残留"} subtitle={`${program.name} · ${isComplete ? "扫描结果已经准备好，请检查后再决定清理范围" : "正在保守地查找与该程序明确关联的项目"}`} />
      <div className="scan-main">
        <div className={`radar ${isComplete ? "done" : ""}`}><div className="radar-sweep" /><span className="radar-dot one" /><span className="radar-dot two" /><span className="radar-dot three" /><div className="radar-center"><strong>{isComplete ? traces.length : "…"}</strong><span>{isComplete ? "发现项目" : "分析中"}</span></div></div>
        <div className="scan-locations"><h3>扫描位置 <span>{isComplete ? "4 个区域已完成" : "实时扫描中"}</span></h3>{locationMeta.map(({ category, icon: Icon, title, path, hint }) => {
          const summary = summaries.find((item) => item.category === category);
          return <div className={`scan-location ${isComplete ? "done" : "active"}`} key={category}><Icon size={20} /><div><strong>{title}</strong><span>{path}</span><small>{hint}</small></div><em>{isComplete ? "已完成" : "正在扫描…"}</em><b>{isComplete ? `${summary?.count ?? 0} 项` : "探测中"}</b></div>;
        })}</div>
      </div>
      <div className="scan-stats card-surface"><h3>扫描统计 <span>{isComplete ? "已生成可审查快照" : "扫描引擎正在建立快照"}</span></h3><div><span>扫描区域<strong>4 / 4</strong></span><span>发现项目<strong className="blue">{isComplete ? traces.length : "分析中"}</strong></span><span>预计大小<strong className="blue">{isComplete ? formatBytes(traces.reduce((sum, trace) => sum + (trace.size ?? 0), 0)) : "计算中"}</strong></span><span>扫描策略<strong>{isComplete ? "保守匹配" : "安全筛选"}</strong></span></div></div>
      <div className="scan-live-log card-surface"><div className="panel-title"><span><Loader2 size={13} className={isComplete ? "" : "spinning"} />扫描引擎实时日志</span><strong>{isComplete ? "扫描结果已锁定" : "正在工作"}</strong></div><div>{latestLogs.length > 0 ? latestLogs.map((log, index) => <p key={`${log}-${index}`}>{log}</p>) : <p>正在初始化扫描器…</p>}</div></div>
      <div className="page-footnote"><Info size={15} /><span>我们仅显示与已卸载程序明确相关的项目，系统关键项和低置信度项目不会被自动删除。</span>{isComplete && <button onClick={onContinue}>查看扫描结果 <ChevronDown size={13} /></button>}</div>
    </div>
  );
}

function BatchUninstallModal({
  selectedPrograms,
  items,
  active,
  paused,
  error,
  onStart,
  onPause,
  onResume,
  onCancel,
  onClose,
}: {
  selectedPrograms: InstalledProgram[];
  items: BatchUninstallItem[];
  active: boolean;
  paused: boolean;
  error: string | null;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onClose: () => void;
}) {
  const started = items.length > 0;
  const queued = items.filter((item) => item.status === "queued").length;
  const completed = items.filter((item) => item.status === "completed").length;
  const failed = items.filter((item) => item.status === "failed").length;
  const cancelled = items.filter((item) => item.status === "cancelled").length;
  const statusLabel = {
    queued: "排队中",
    planning: "准备中",
    running: "卸载中",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  } as const;

  return <div className="modal-backdrop" onMouseDown={onClose}>
    <section className="batch-uninstall-modal" onMouseDown={(event) => event.stopPropagation()}>
      <header>
        <div><span className="batch-modal-mark"><ListChecks size={20} /></span><span><h2>批量卸载队列</h2><p>{started ? "每项独立执行，按顺序运行并保留结果" : "先确认范围，再逐项调用原厂卸载器"}</p></span></div>
        <button aria-label="关闭批量卸载队列" onClick={onClose}><X size={17} /></button>
      </header>
      {!started ? <div className="batch-preview">
        <div className="batch-warning"><TriangleAlert size={17} /><span>批量模式只运行每个程序的原厂卸载器；卸载后发现的残留会保留在结果中，不会被批量自动删除。</span></div>
        {!isTauriRuntime() && <div className="force-error"><TriangleAlert size={15} /><span>当前是浏览器预览，不能执行真实批量卸载。</span></div>}
        <div className="batch-preview-list"><h3>待处理程序 <small>{selectedPrograms.length} 项</small></h3>{selectedPrograms.map((program) => <div key={program.id}><span className="batch-preview-icon"><Package size={14} /></span><span><strong>{program.name}</strong><small>{program.publisher ?? "未知发布者"} · {program.install_location ?? "未记录安装路径"}</small></span></div>)}</div>
      </div> : <div className="batch-body">
        <div className="batch-summary"><div><strong>{items.length}</strong><small>总数</small></div><div><strong className="green">{completed}</strong><small>完成</small></div><div><strong className="orange">{failed}</strong><small>失败</small></div><div><strong>{queued + cancelled}</strong><small>待处理/取消</small></div></div>
        {error && <div className="batch-error"><TriangleAlert size={15} /><span>{error}</span></div>}
        <div className="batch-list">{items.map((item) => <div className="batch-row" key={item.program.id}><span className={`batch-status ${item.status}`}>{statusLabel[item.status]}</span><span><strong>{item.program.name}</strong><small>{item.message ?? "等待队列调度"}</small>{item.error && <em>{item.error}</em>}</span><b>{item.traces_found > 0 ? `保留 ${item.traces_found} 项残留` : item.status === "completed" ? "无残留" : ""}</b></div>)}</div>
      </div>}
      <footer>
        <span>{started ? (active ? (paused ? "队列已暂停，可继续或取消后续项" : "正在串行处理，当前项完成后才会开始下一项") : "队列已停止，所有结果均已保留") : "失败项会单独标记，不会阻止后续项继续"}</span>
        <button className="secondary-button" onClick={onClose}>{active ? "隐藏" : "关闭"}</button>
        {!started ? <button className="primary-button" disabled={!isTauriRuntime() || selectedPrograms.length === 0} onClick={onStart}><Play size={14} fill="currentColor" />开始队列</button> : active && !paused ? <><button className="secondary-button" onClick={onPause}><Pause size={14} />暂停</button><button className="danger-button" onClick={onCancel}>取消后续</button></> : active && paused ? <><button className="secondary-button" onClick={onCancel}>取消队列</button><button className="primary-button" onClick={onResume}><Play size={14} fill="currentColor" />继续队列</button></> : paused && queued > 0 ? <button className="primary-button" onClick={onResume}><Play size={14} fill="currentColor" />重试队列</button> : null}
      </footer>
    </section>
  </div>;
}

function ForceUninstallModal({
  initialPath,
  plan,
  result,
  loading,
  error,
  onPlan,
  onClean,
  onClose,
}: {
  initialPath: string;
  plan: import("./types").ForceUninstallPlan | null;
  result: import("./types").ForceUninstallResult | null;
  loading: boolean;
  error: string | null;
  onPlan: (path: string, name?: string) => Promise<import("./types").ForceUninstallPlan | null>;
  onClean: (traceIds: string[]) => Promise<import("./types").ForceUninstallResult | null>;
  onClose: () => void;
}) {
  const [path, setPath] = useState(initialPath === "—" ? "" : initialPath);
  const [name, setName] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const submitPlan = async () => {
    if (!isTauriRuntime()) return;
    const next = await onPlan(path, name);
    if (next) setSelectedIds(new Set(next.default_selected_ids));
  };

  const toggleTrace = (id: string) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const submitCleanup = () => {
    if (selectedIds.size === 0 || loading) return;
    void onClean([...selectedIds]);
  };

  return <div className="modal-backdrop" onMouseDown={onClose}>
    <section className="force-uninstall-modal" onMouseDown={(event) => event.stopPropagation()}>
      <header>
        <div><span className="force-modal-mark"><Wrench size={19} /></span><span><h2>强制 / 自定义卸载</h2><p>适用于卸载器损坏、程序不在列表中或残留无法清理的情况。</p></span></div>
        <button aria-label="关闭强制卸载" onClick={onClose}><X size={17} /></button>
      </header>
      {!plan && !result ? <>
        <div className="force-form">
          <div className="force-warning"><TriangleAlert size={17} /><span>强制模式不会运行原厂卸载器，而是删除你确认的目录和候选残留。请先尝试标准卸载。</span></div>
          <label>程序目录、EXE 或快捷方式<input value={path} onChange={(event) => setPath(event.target.value)} placeholder={"例如：C:\\Program Files\\Example App"} /></label>
          <label>程序名称（可选）<input value={name} onChange={(event) => setName(event.target.value)} placeholder="留空则从路径推断" /></label>
          {error && <div className="force-error" role="alert"><TriangleAlert size={15} />{error}</div>}
        </div>
        <footer><span>计划阶段只扫描，不会删除文件或注册表。</span><button className="primary-button" disabled={loading || path.trim().length === 0 || !isTauriRuntime()} onClick={() => void submitPlan()}>{loading ? <Loader2 size={14} className="spinning" /> : <Search size={14} />}分析目标</button></footer>
      </> : result ? <>
        <div className={`force-result ${result.success ? "success" : "partial"}`}><span>{result.success ? <Check size={25} /> : <TriangleAlert size={25} />}</span><h2>{result.success ? "强制卸载完成" : "强制卸载部分完成"}</h2><p>{result.message}</p><div><strong>{result.traces_cleaned}</strong><small>已处理</small><strong>{result.failed_count}</strong><small>失败</small><strong>{formatBytes(result.bytes_freed)}</strong><small>释放空间</small></div></div>
        {result.outcomes.some((outcome) => !outcome.success) && <div className="force-failed-list">{result.outcomes.filter((outcome) => !outcome.success).map((outcome) => <p key={outcome.trace_id}><TriangleAlert size={13} />{outcome.path}<small>{outcome.error ?? "删除失败"}</small></p>)}</div>}
        <footer><span>失败项已保留，请处理占用文件后重试。</span><button className="primary-button" onClick={onClose}>关闭</button></footer>
      </> : <>
        <div className="force-plan-body">
          <div className="force-target-summary"><strong>{plan?.target.name}</strong><span>{plan?.target.resolved_path}</span><em>{plan?.target.kind === "shortcut" ? "快捷方式解析" : plan?.target.kind === "executable" ? "EXE 所在目录" : "用户提供目录"}</em></div>
          {plan?.warnings.map((warning) => <p className="force-warning-line" key={warning}><Info size={13} />{warning}</p>)}
          <div className="force-trace-list"><h3>可审查目标 <small>{plan?.traces.length ?? 0} 项，默认不选</small></h3>{plan?.traces.map((trace) => <label className="force-trace-row" key={trace.id}><input type="checkbox" checked={selectedIds.has(trace.id)} onChange={() => toggleTrace(trace.id)} /><span className={`force-confidence ${trace.confidence}`}>{trace.confidence === "high" ? "高" : trace.confidence === "medium" ? "中" : "低"}</span><span><strong>{trace.description || formatTraceType(trace.trace_type)}</strong><small>{trace.path}</small></span><b>{formatBytes(trace.size)}</b></label>)}</div>
          {error && <div className="force-error" role="alert"><TriangleAlert size={15} />{error}</div>}
        </div>
        <footer><button className="secondary-button" onClick={onClose}>取消</button><span className="force-selection-count">已选择 {selectedIds.size} 项</span><button className="danger-button" disabled={loading || selectedIds.size === 0} onClick={submitCleanup}>{loading ? <Loader2 size={14} className="spinning" /> : <Trash2 size={14} />}确认删除所选</button></footer>
      </>}
    </section>
  </div>;
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

function CompleteStage({ program, results, traces: visibleTraces, job, logs, onDone }: { program: UiProgram; results: CleanResult[]; traces: Trace[]; job: UninstallJob | null; logs: string[]; onDone: () => void }) {
  const [reportOpen, setReportOpen] = useState(false);
  const traces = job?.snapshot.traces ?? visibleTraces;
  const selectedIds = new Set(job?.snapshot.selected_trace_ids ?? results.filter((result) => result.success).map((result) => result.trace_id));
  const outcome = job?.outcome;
  const removed = outcome?.traces_cleaned ?? results.filter((result) => result.success).length;
  const found = outcome?.traces_found ?? traces.length;
  const freed = outcome?.bytes_freed ?? results.reduce((sum, result) => sum + result.bytes_freed, 0);
  const skipped = Math.max(0, found - removed);
  const summaries = summarizeTraces(traces);
  const eventLogs = logs.length > 0 ? logs : job?.events.map((event) => formatUninstallEventLog(event)) ?? [];
  const reportLogs = eventLogs.length > 0 ? eventLogs : ["扫描日志暂不可用，请重新打开卸载报告。"];
  return (
    <div className="page complete-page">
      <SectionHeader title="卸载完成" subtitle="扫描结果已保存为本次卸载任务的审查快照" />
      <div className="success-heading"><span><Check size={36} /></span><div><h2>{program.name} 已成功卸载</h2><p>内置卸载器、移除验证、残留扫描和清理流程均已完成。</p></div></div>
      <div className="summary-grid card-surface"><div><span>扫描发现</span><strong className="blue">{found} 项</strong></div><div><span>已清理</span><strong className="green">{removed} 项</strong></div><div><span>保留 / 跳过</span><strong className="orange">{skipped} 项</strong></div><div><span>释放空间</span><strong className="green">{formatBytes(freed)}</strong></div></div>
      <div className="report-panel card-surface"><div className="panel-title"><span><FileText size={14} />扫描与清理报告</span><button onClick={() => setReportOpen(true)}>展开完整报告</button></div><div className="report-category-grid">{summaries.filter((summary) => summary.count > 0).map((summary) => <div key={summary.category}><span>{summary.category === "files" ? "文件系统" : summary.category === "user_data" ? "用户数据" : summary.category === "registry" ? "注册表" : "系统集成"}</span><strong>{summary.count} 项</strong><small>{formatBytes(summary.bytes)}</small></div>)}{summaries.every((summary) => summary.count === 0) && <div className="report-empty">未发现可疑残留项目</div>}</div><div className="log-content report-log">{reportLogs.slice(-8).map((log, index) => <div key={`${log}-${index}`}>{log}</div>)}</div></div>
      <div className="complete-actions"><button className="secondary-button" onClick={() => setReportOpen(true)}><FileText size={16} />查看完整报告</button><button className="primary-button" onClick={onDone}><Check size={16} />完成</button></div>
      {reportOpen && <UninstallReportModal program={program} traces={traces} selectedIds={selectedIds} summaries={summaries} logs={reportLogs} onClose={() => setReportOpen(false)} />}
    </div>
  );
}

function UninstallReportModal({ program, traces, selectedIds, summaries, logs, onClose }: { program: UiProgram; traces: Trace[]; selectedIds: Set<string>; summaries: ReturnType<typeof summarizeTraces>; logs: string[]; onClose: () => void }) {
  return <div className="modal-backdrop" onMouseDown={onClose}><section className="uninstall-report-modal" onMouseDown={(event) => event.stopPropagation()}><header><div><span className="report-modal-mark"><FileText size={20} /></span><span><h2>{program.name} · 卸载扫描报告</h2><p>以下内容来自本次卸载的真实扫描快照，不是估算值。</p></span></div><button aria-label="关闭卸载报告" onClick={onClose}><X size={17} /></button></header><div className="report-modal-summary">{summaries.filter((summary) => summary.count > 0).map((summary) => <div key={summary.category}><span>{summary.category === "files" ? "文件系统" : summary.category === "user_data" ? "用户数据" : summary.category === "registry" ? "注册表" : "系统集成"}</span><strong>{summary.count}</strong><small>{formatBytes(summary.bytes)}</small></div>)}</div><div className="report-modal-body"><section className="report-event-column"><h3>完整操作日志 <small>{logs.length} 条</small></h3><div className="report-event-list">{logs.map((log, index) => <p key={`${log}-${index}`}>{log}</p>)}</div></section><section className="report-trace-column"><h3>扫描项目明细 <small>{traces.length} 项</small></h3><div className="report-trace-list">{traces.length > 0 ? traces.map((trace) => <div className="report-trace-item" key={trace.id}><span className={`report-trace-state ${selectedIds.has(trace.id) ? "cleaned" : "kept"}`}>{selectedIds.has(trace.id) ? <Check size={12} /> : "—"}</span><span><strong>{trace.description || formatTraceType(trace.trace_type)}</strong><small>{trace.path}</small><em>{formatTraceType(trace.trace_type)} · {trace.confidence === "high" ? "高置信度" : trace.confidence === "medium" ? "中置信度" : "低置信度"} · {formatBytes(trace.size)}</em></span><b className={selectedIds.has(trace.id) ? "cleaned-status" : "kept-status"}>{selectedIds.has(trace.id) ? "已纳入清理" : "已保留"}</b></div>) : <p className="report-empty">扫描没有发现残留项目。</p>}</div></section></div><footer><span>低置信度项目默认保留，避免误删用户数据。</span><button className="primary-button" onClick={onClose}>关闭报告</button></footer></section></div>;
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
