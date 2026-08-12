import { getLanguage, setLanguage, supportedLanguages, t, type Language } from "./i18n/index.ts";
import { useEffect, useRef, useState } from "react";
import {
  Activity,
  AppWindow,
  Archive,
  Box,
  Check,
  ChevronDown,
  CircleHelp,
  Database,
  FileCode2,
  FileText,
  FileWarning,
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
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getProgramIconSrc } from "./lib/icon";
import { completedSuccessfully, selectAvailableItem } from "./lib/appWorkflow";
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
import { FileShredderPage } from "./components/FileShredderPage";
import { BackupCenter } from "./components/BackupCenter";
import { InstallMonitorManager } from "./components/InstallMonitorManager";
import { ReportCenter } from "./components/ReportCenter";
import { HealthCenter } from "./components/HealthCenter";
import { BrowserPluginsPage } from "./components/BrowserPluginsPage";
import { ToolboxPage } from "./components/ToolboxPage";
import { CleanupSafetyRules } from "./components/CleanupSafetyRules";
import { SoftwareInventoryComparison } from "./components/SoftwareInventoryComparison";
import { SoftwareRecordDialog } from "./components/SoftwareRecordDialog";
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
type NavKey = "apps" | "health" | "startup" | "cleaner" | "shredder" | "backups" | "traces" | "monitor" | "inventory" | "reports" | "plugins" | "tools" | "settings" | "about";

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
  { id: "t1", program_name: "7-Zip 24.09", trace_type: "file", path: "C:\\Program Files\\7-Zip\\", exists: true, size: 5_570_560, confidence: "high", description: t("app.message_001") },
  { id: "t2", program_name: "7-Zip 24.09", trace_type: "appdata", path: "C:\\Users\\Admin\\AppData\\Roaming\\7-Zip\\", exists: true, size: 3_276_800, confidence: "high", description: t("app.message_002") },
  { id: "t3", program_name: "7-Zip 24.09", trace_type: "appdata", path: "C:\\Users\\Admin\\AppData\\Local\\7-Zip\\", exists: true, size: 1_310_720, confidence: "medium", description: t("app.message_003") },
  { id: "t4", program_name: "7-Zip 24.09", trace_type: "file", path: "C:\\Users\\Admin\\AppData\\Local\\Temp\\7zFM\\", exists: true, size: 430_080, confidence: "low", description: t("app.message_004") },
  { id: "t5", program_name: "7-Zip 24.09", trace_type: "shortcut", path: "C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\7-Zip.lnk", exists: true, size: 379_000, confidence: "medium", description: t("app.message_005") },
  { id: "t6", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\7zFM.exe", exists: true, size: 3_200, confidence: "high", description: t("app.message_006") },
  { id: "t7", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKCU\\Software\\Classes\\7z.*", exists: true, size: 196_000, confidence: "medium", description: t("app.message_007") },
  { id: "t8", program_name: "7-Zip 24.09", trace_type: "registry_key", path: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.7z", exists: true, size: 56_000, confidence: "low", description: t("app.message_008") },
];

const initialLogs = [
  t("app.message_009"),
  t("app.message_010"),
  t("app.message_011"),
  t("app.message_012"),
  t("app.message_013"),
  t("app.message_014"),
  t("app.message_015"),
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
    publisher: program.publisher ?? t("app.message_016"),
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
  return `[${new Date().toLocaleTimeString(getLanguage(), { hour12: false })}] ${message}`;
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
    () => new Set(demoTraces.filter((trace) => trace.confidence === "high" && !trace.is_critical).map((trace) => trace.id)),
  );
  const [cleanConfirm, setCleanConfirm] = useState(false);
  const [cleanResults, setCleanResults] = useState<CleanResult[]>([]);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [recordsOpen, setRecordsOpen] = useState(false);
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
  const contextMenuEnabled = useForceUninstallStore((state) => state.contextMenuEnabled);
  const contextMenuLoading = useForceUninstallStore((state) => state.contextMenuLoading);
  const hunterLoading = useForceUninstallStore((state) => state.hunterLoading);
  const planForceTarget = useForceUninstallStore((state) => state.planTarget);
  const cleanForceSelected = useForceUninstallStore((state) => state.cleanSelected);
  const loadContextMenu = useForceUninstallStore((state) => state.loadContextMenu);
  const setContextMenuEnabled = useForceUninstallStore((state) => state.setContextMenuEnabled);
  const captureHunterTarget = useForceUninstallStore((state) => state.captureHunterTarget);
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
  const [startupForceTarget, setStartupForceTarget] = useState<string | null>(null);

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
    if (!isTauriRuntime()) return;
    let active = true;
    void loadContextMenu();
    void invoke<string | null>("get_force_uninstall_startup_target")
      .then((target) => {
        if (!active || !target) return;
        setStartupForceTarget(target);
        resetForce();
        setForceOpen(true);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [loadContextMenu, resetForce]);

  useEffect(() => {
    if (!manualScanPending || tracesLoading) return;
    setTraces(scannedTraces);
    setSelectedTraceIds(new Set(scannedTraces.filter((trace) => trace.confidence === "high" && !trace.is_critical).map((trace) => trace.id)));
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
            formatLocalLog(t("app.message_017")),
            formatLocalLog(t("app.message_018")),
            formatLocalLog(t("app.message_019")),
            formatLocalLog(t("app.message_020")),
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
  const selectedProgram = selectAvailableItem(
    filteredPrograms,
    allPrograms,
    selectedId,
    isTauriRuntime() ? null : mockPrograms[1],
  );
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
    setStartupForceTarget(null);
    resetForce();
  };

  const handleSearch = (value: string) => {
    setQuery(value);
    if (isTauriRuntime()) {
      setSearchQuery(value);
    }
  };

  const handleManualScan = () => {
    if (!selectedProgram) return;
    setStage("scan");
    setTraces([]);
    setScanStatus("scanning");
    scanStartedAtRef.current = Date.now();
    if (isTauriRuntime() && selectedProgram.source) {
      setManualScanPending(true);
      void scanTraces(selectedProgram.source.name, selectedProgram.source);
    } else {
      setTraces(demoTraces);
      setSelectedTraceIds(new Set(demoTraces.filter((trace) => trace.confidence === "high" && !trace.is_critical).map((trace) => trace.id)));
      window.setTimeout(() => {
        setScanStatus("complete");
        setStage("review");
      }, 900);
    }
  };

  const startUninstall = () => {
    if (!selectedProgram) return;
    setStage("progress");
    setProgress(8);
    setTraces([]);
    setLogs([t("app.message_021", { value0: new Date().toLocaleTimeString(getLanguage(), { hour12: false }) })]);
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
        const categoryLabels = { files: t("app.message_022"), user_data: t("app.message_023"), registry: t("app.message_024"), system: t("app.message_025") };
        const scanSummaryLogs = summarizeTraces(residueItems)
          .filter((summary) => summary.count > 0)
          .map((summary) => formatLocalLog(t("app.message_026", { value0: categoryLabels[summary.category], value1: summary.count, value2: formatBytes(summary.bytes) })));
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
          ) : activeNav === "shredder" ? (
            <FileShredderPage />
          ) : activeNav === "backups" ? (
            <BackupCenter />
          ) : activeNav === "monitor" ? (
            <InstallMonitorManager />
          ) : activeNav === "inventory" ? (
            <SoftwareInventoryComparison />
          ) : activeNav === "reports" ? (
            <ReportCenter />
          ) : activeNav === "health" ? (
            <HealthCenter />
          ) : activeNav === "plugins" ? (
            <BrowserPluginsPage />
          ) : activeNav === "tools" ? (
            <ToolboxPage onNavigate={(next) => setActiveNav(next)} />
          ) : activeNav === "settings" ? (
            <SettingsPage />
          ) : activeNav !== "apps" ? (
            <PlaceholderPage active={activeNav} />
          ) : stage === "apps" || !selectedProgram ? (
            <AppsStage
              programs={filteredPrograms}
              selected={selectedProgram}
              selectedId={selectedProgram?.id ?? ""}
              loading={loading}
              metadataLoading={metadataLoading}
              error={error}
              query={query}
              sourceFilter={sourceFilter}
              sourceCounts={sourceCounts}
              onQuery={handleSearch}
              onSourceFilter={setSourceFilter}
              onSelect={setSelectedId}
              onUninstall={() => { if (selectedProgram) setStage("confirm"); }}
              onScan={handleManualScan}
              onDetails={() => setDetailsOpen(true)}
              onRecords={() => setRecordsOpen(true)}
              canInspectRecords={isTauriRuntime() && Boolean(selectedProgram?.source)}
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
                if (jobId) void finishUninstall(jobId).then((result) => {
                  if (completedSuccessfully(result)) setStage("complete");
                });
              }}
              onRescan={handleManualScan}
              onClean={() => setCleanConfirm(true)}
              confirmOpen={cleanConfirm}
              onConfirmClose={() => setCleanConfirm(false)}
              onConfirm={() => void cleanSelected()}
            />
          ) : (
            <CompleteStage
              program={completedProgram ?? selectedProgram}
              results={cleanResults}
              traces={traces}
              job={uninstallJob}
              logs={logs}
              onDone={() => { setStage("apps"); setCleanResults([]); }}
            />
          )}
        </main>
      </div>
      {detailsOpen && selectedProgram && <ProgramInfoModal program={selectedProgram} onClose={() => setDetailsOpen(false)} />}
      {recordsOpen && selectedProgram?.source && <SoftwareRecordDialog program={selectedProgram.source} onClose={() => setRecordsOpen(false)} />}
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
        initialPath={startupForceTarget ?? selectedProgram?.source?.install_location ?? ""}
        plan={forcePlan}
        result={forceResult}
        loading={forceLoading}
        error={forceError}
        contextMenuEnabled={contextMenuEnabled}
        contextMenuLoading={contextMenuLoading}
        hunterLoading={hunterLoading}
        onPlan={planForceTarget}
        onClean={cleanForceSelected}
        onReset={resetForce}
        onContextMenu={setContextMenuEnabled}
        onHunter={captureHunterTarget}
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
      console.error(t("app.message_027", { value0: name }), error);
    }
  };

  const startDragging = async (event: React.MouseEvent<HTMLElement>) => {
    if (!isTauriRuntime() || event.button !== 0) return;
    if ((event.target as HTMLElement).closest(".window-actions")) return;
    try {
      await getCurrentWindow().startDragging();
    } catch (error) {
      console.error(t("app.message_028"), error);
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
        <span>{t("common.brand.compact")}</span>
      </div>
      <div className="window-actions">
        <button aria-label={t("app.message_029")} onClick={() => void act("minimize")}><Minus size={15} /></button>
        <button aria-label={t("app.message_030")} onClick={() => void act("maximize")}><Maximize2 size={12} /></button>
        <button aria-label={t("app.message_031")} className="close" onClick={() => void act("close")}><X size={15} /></button>
      </div>
    </header>
  );
}

const navItems: { id: NavKey; label: string; icon: typeof Package }[] = [
  { id: "apps", label: t("app.message_032"), icon: Grid2X2 },
  { id: "health", label: t("app.message_033"), icon: Activity },
  { id: "startup", label: t("app.message_034"), icon: Zap },
  { id: "cleaner", label: t("app.message_035"), icon: Sparkles },
  { id: "shredder", label: t("shredder.nav"), icon: FileWarning },
  { id: "backups", label: t("app.message_036"), icon: ShieldCheck },
  { id: "traces", label: t("app.message_037"), icon: Archive },
  { id: "monitor", label: t("app.message_038"), icon: SquareActivity },
  { id: "inventory", label: t("components.softwareinventory.nav"), icon: Database },
  { id: "reports", label: t("app.message_040"), icon: FileText },
  { id: "plugins", label: t("app.message_041"), icon: AppWindow },
  { id: "tools", label: t("app.message_042"), icon: Wrench },
];

function Sidebar({ active, onNavigate }: { active: NavKey; onNavigate: (key: NavKey) => void }) {
  return (
    <aside className="sidebar">
      <nav>
        {navItems.map((item) => <NavButton key={item.id} {...item} active={active === item.id} onClick={() => onNavigate(item.id)} />)}
      </nav>
      <nav className="sidebar-bottom">
        <NavButton id="settings" label={t("app.message_043")} icon={Settings} active={active === "settings"} onClick={() => onNavigate("settings")} />
        <NavButton id="about" label={t("app.message_044")} icon={CircleHelp} active={active === "about"} onClick={() => onNavigate("about")} />
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
  programs: UiProgram[]; selected: UiProgram | null; selectedId: string; loading: boolean; metadataLoading: boolean;
  error: string | null; query: string; sourceFilter: ProgramSourceFilter;
  sourceCounts: Record<ProgramSourceFilter, number>;
  onQuery: (value: string) => void; onSourceFilter: (value: ProgramSourceFilter) => void; onSelect: (id: string) => void;
  onUninstall: () => void; onScan: () => void; onDetails: () => void; onRecords: () => void; onRefresh: () => void;
  canInspectRecords: boolean;
  onForceUninstall: () => void;
  batchSelectedIds: Set<string>; batchActive: boolean; onToggleBatch: (id: string) => void; onOpenBatch: () => void;
}) {
  return (
    <div className="page apps-page">
      <SectionHeader title={t("app.message_032")} action={<div className="header-actions"><button className="secondary-button compact-button" disabled={!props.batchActive && props.batchSelectedIds.size === 0} onClick={props.onOpenBatch}><ListChecks size={14} />{props.batchActive ? t("app.message_046") : t("app.message_047", { value0: props.batchSelectedIds.size ? ` (${props.batchSelectedIds.size})` : "" })}</button><button className="secondary-button compact-button" onClick={props.onForceUninstall}><Wrench size={14} />{t("app.message_048")}</button><button className="icon-button" disabled={props.loading || props.metadataLoading} title={props.metadataLoading ? t("app.message_049") : t("app.message_050")} onClick={props.onRefresh}><RefreshCw className={props.loading || props.metadataLoading ? "spinning" : ""} size={17} /></button></div>} />
      <div className="toolbar">
        <label className="search-box"><Search size={16} /><input value={props.query} onChange={(event) => props.onQuery(event.target.value)} placeholder={t("app.message_051")} /></label>
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
          <div className="table-head"><span>{t("app.message_052")}</span><span>{t("app.message_053")} <ChevronDown size={12} /></span><span>{t("app.message_054")}</span><span>{t("app.message_055")}</span><span>{t("app.message_056")} <ChevronDown size={12} /></span></div>
          <div className="table-body">
            {props.error ? <div className="table-message error">{props.error}</div> : props.loading && props.programs.length === 0 ? <div className="table-message">{t("app.message_057")}</div> : props.programs.length === 0 ? <div className="table-message">{t("app.message_058")}</div> : props.programs.map((program) => (
              <div key={program.id} className={`program-row ${props.selectedId === program.id ? "selected" : ""}`} role="button" tabIndex={0} aria-selected={props.selectedId === program.id} onClick={() => props.onSelect(program.id)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); props.onSelect(program.id); } }}>
                <span className="program-select"><input type="checkbox" aria-label={t("app.message_059", { value0: program.name })} checked={props.batchSelectedIds.has(program.id)} disabled={props.batchActive} onClick={(event) => event.stopPropagation()} onChange={() => props.onToggleBatch(program.id)} /></span>
                <span className="program-name"><AppIcon program={program} /><strong>{program.name}</strong></span>
                <span>{program.publisher}</span><span>{program.size}</span><span>{program.installed}</span>
              </div>
            ))}
          </div>
          <div className="table-footer">{t("app.message_060")} {props.programs.length}  {t("app.message_061")}{props.batchSelectedIds.size > 0 ? t("app.message_062", { value0: props.batchSelectedIds.size }) : ""}</div>
        </div>
        {props.selected && <aside className="program-detail card-surface">
          <div className="detail-app"><AppIcon program={props.selected} large /><h2>{props.selected.name}</h2><p>{props.selected.publisher}</p><span>{props.selected.size}</span></div>
          <dl><div><dt>{t("app.message_063")}</dt><dd>{props.selected.version}</dd></div><div><dt>{t("app.message_056")}</dt><dd>{props.selected.installed}</dd></div></dl>
          <p className="install-path">{props.selected.location}</p>
          <div className="detail-actions">
            <button onClick={props.onDetails}><Info size={14} />{t("app.message_068")}</button>
            <button onClick={props.onScan}><Search size={14} />{t("app.message_069")}</button>
            <button disabled={!props.canInspectRecords} onClick={props.onRecords}><FileCode2 size={14} />{t("components.softwarerecords.action")}</button>
            <button onClick={props.onForceUninstall}><Wrench size={14} />{t("app.message_048")}</button>
          </div>
          <button className="primary-button uninstall-button" onClick={props.onUninstall}><Trash2 size={16} />{t("app.message_071")}</button>
        </aside>}
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
      <SectionHeader title={t("app.message_072")} subtitle={t("app.message_073")} />
      <div className="confirm-card card-surface">
        <div className="confirm-program"><AppIcon program={props.program} large /><div><h2>{props.program.name}</h2><p>{props.program.publisher}<span>·</span>{props.program.size}</p></div></div>
        <div className="warning-banner"><TriangleAlert size={21} /><div><strong>{t("app.message_074")}</strong><p>{t("app.message_075")}</p></div></div>
        <div className="risk-box"><h3>{t("app.message_076")}</h3><div><span>{t("app.message_077")}</span><b className="risk-medium">{t("app.message_078")}</b></div><div><span>{t("app.message_079")}</span><b className="risk-medium">{t("app.message_078")}</b></div><div><span>{t("app.message_081")}</span><b className="risk-low">{t("app.message_082")}</b></div><div><span>{t("app.message_083")}</span><b>{t("app.message_067")}</b></div></div>
        <div className="option-list"><CheckOption checked={props.restorePoint} onChange={props.onRestorePoint} title={t("app.message_085")} hint={t("app.message_086")} /><CheckOption checked={props.scanAfter} onChange={props.onScanAfter} title={t("app.message_087")} hint={t("app.message_086")} /></div>
        <div className="dialog-actions"><button className="secondary-button" onClick={props.onCancel}>{t("app.message_089")}</button><button className="primary-button" onClick={props.onStart}><Play size={15} fill="currentColor" />{t("app.message_090")}</button></div>
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
    [t("app.message_085"), "done"], [t("app.message_092"), "done"], [t("app.message_093"), failed ? "failed" : "active"], [t("app.message_094"), "todo"], [t("app.message_095"), "todo"], [t("app.message_096"), "todo"],
  ];
  return (
    <div className="page progress-page">
      <SectionHeader title={failed ? t("app.message_097", { value0: program.name }) : t("app.message_098", { value0: program.name })} subtitle={failed ? t("app.message_099") : t("app.message_100")} />
      <div className="progress-top">
        <div className={`progress-ring ${failed ? "failed" : nativeWorkflow ? "indeterminate" : "preview-progress"}`}><div><strong>{failed ? "!" : nativeWorkflow ? "…" : `${progress}%`}</strong><span>{failed ? t("app.message_101") : nativeWorkflow ? t("app.message_102") : t("app.message_103")}</span><small>{failed ? t("app.message_104") : nativeWorkflow ? t("app.message_105") : "00:00:28"}</small></div></div>
        <div className="steps-list">{steps.map(([label, state]) => <div key={label} className={`step ${state}`}><span>{state === "done" ? <Check size={12} /> : state === "failed" ? <TriangleAlert size={11} /> : ""}</span><strong>{label}</strong><em>{state === "done" ? t("app.message_106") : state === "active" ? t("app.message_107") : state === "failed" ? t("app.message_108") : ""}</em></div>)}</div>
      </div>
      <div className={`log-panel card-surface${failed ? " error-state" : ""}`}>
        <div className="panel-title"><span>{t("app.message_109")}</span><button>{t("app.message_110")}</button></div>
        {error && <div className="uninstall-error" role="alert"><TriangleAlert size={16} /><div><strong>{t("app.message_101")}</strong><span>{error}</span></div></div>}
        <div className="log-content">{error && <div className="error-log">{t("app.message_112")} {error}</div>}{logs.map((log, index) => <div key={`${log}-${index}`}>{log}</div>)}</div>
      </div>
      <div className="page-footnote"><Info size={15} /><span>{t("app.message_113")}</span>{!isTauriRuntime() && <button onClick={onContinue}>{t("app.message_114")}</button>}</div>
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
    { category: "files" as const, icon: Folder, title: t("app.message_115"), path: program.location || t("app.message_116"), hint: t("app.message_117") },
    { category: "user_data" as const, icon: Box, title: t("app.message_023"), path: "%APPDATA% · %LOCALAPPDATA%", hint: t("app.message_119") },
    { category: "registry" as const, icon: Database, title: t("app.message_024"), path: "HKCU · HKLM\\Software", hint: t("app.message_121") },
    { category: "system" as const, icon: ShieldCheck, title: t("app.message_025"), path: t("app.message_123"), hint: t("app.message_124") },
  ];
  const isComplete = status === "complete";
  const latestLogs = logs.slice(-4);
  return (
    <div className={`page scan-page ${isComplete ? "scan-complete" : "scan-running"}`}>
      <SectionHeader title={isComplete ? t("app.message_125") : t("app.message_126")} subtitle={t("app.message_127", { value0: program.name, value1: isComplete ? t("app.message_325") : t("app.message_326") })} />
      <div className="scan-main">
        <div className={`radar ${isComplete ? "done" : ""}`}><div className="radar-sweep" /><span className="radar-dot one" /><span className="radar-dot two" /><span className="radar-dot three" /><div className="radar-center"><strong>{isComplete ? traces.length : "…"}</strong><span>{isComplete ? t("app.message_128") : t("app.message_129")}</span></div></div>
        <div className="scan-locations"><h3>{t("app.message_130")} <span>{isComplete ? t("app.message_131") : t("app.message_132")}</span></h3>{locationMeta.map(({ category, icon: Icon, title, path, hint }) => {
          const summary = summaries.find((item) => item.category === category);
          return <div className={`scan-location ${isComplete ? "done" : "active"}`} key={category}><Icon size={20} /><div><strong>{title}</strong><span>{path}</span><small>{hint}</small></div><em>{isComplete ? t("app.message_133") : t("app.message_134")}</em><b>{isComplete ? t("app.message_135", { value0: summary?.count ?? 0 }) : t("app.message_136")}</b></div>;
        })}</div>
      </div>
      <div className="scan-stats card-surface"><h3>{t("app.message_137")} <span>{isComplete ? t("app.message_138") : t("app.message_139")}</span></h3><div><span>{t("app.message_140")}<strong>4 / 4</strong></span><span>{t("app.message_128")}<strong className="blue">{isComplete ? traces.length : t("app.message_129")}</strong></span><span>{t("app.message_143")}<strong className="blue">{isComplete ? formatBytes(traces.reduce((sum, trace) => sum + (trace.size ?? 0), 0)) : t("app.message_144")}</strong></span><span>{t("app.message_145")}<strong>{isComplete ? t("app.message_146") : t("app.message_147")}</strong></span></div></div>
      <div className="scan-live-log card-surface"><div className="panel-title"><span><Loader2 size={13} className={isComplete ? "" : "spinning"} />{t("app.message_148")}</span><strong>{isComplete ? t("app.message_149") : t("app.message_150")}</strong></div><div>{latestLogs.length > 0 ? latestLogs.map((log, index) => <p key={`${log}-${index}`}>{log}</p>) : <p>{t("app.message_151")}</p>}</div></div>
      <div className="page-footnote"><Info size={15} /><span>{t("app.message_152")}</span>{isComplete && <button onClick={onContinue}>{t("app.message_153")} <ChevronDown size={13} /></button>}</div>
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
    queued: t("app.message_154"),
    planning: t("app.message_155"),
    running: t("app.message_156"),
    completed: t("app.message_133"),
    failed: t("app.message_108"),
    cancelled: t("app.message_159"),
  } as const;

  return <div className="modal-backdrop" onMouseDown={onClose}>
    <section className="batch-uninstall-modal" onMouseDown={(event) => event.stopPropagation()}>
      <header>
        <div><span className="batch-modal-mark"><ListChecks size={20} /></span><span><h2>{t("app.message_160")}</h2><p>{started ? t("app.message_161") : t("app.message_162")}</p></span></div>
        <button aria-label={t("app.message_163")} onClick={onClose}><X size={17} /></button>
      </header>
      {!started ? <div className="batch-preview">
        <div className="batch-warning"><TriangleAlert size={17} /><span>{t("app.message_164")}</span></div>
        {!isTauriRuntime() && <div className="force-error"><TriangleAlert size={15} /><span>{t("app.message_165")}</span></div>}
        <div className="batch-preview-list"><h3>{t("app.message_166")} <small>{selectedPrograms.length}  {t("app.message_167")}</small></h3>{selectedPrograms.map((program) => <div key={program.id}><span className="batch-preview-icon"><Package size={14} /></span><span><strong>{program.name}</strong><small>{program.publisher ?? t("app.message_016")} · {program.install_location ?? t("app.message_169")}</small></span></div>)}</div>
      </div> : <div className="batch-body">
        <div className="batch-summary"><div><strong>{items.length}</strong><small>{t("app.message_170")}</small></div><div><strong className="green">{completed}</strong><small>{t("app.message_106")}</small></div><div><strong className="orange">{failed}</strong><small>{t("app.message_108")}</small></div><div><strong>{queued + cancelled}</strong><small>{t("app.message_173")}</small></div></div>
        {error && <div className="batch-error"><TriangleAlert size={15} /><span>{error}</span></div>}
        <div className="batch-list">{items.map((item) => <div className="batch-row" key={item.program.id}><span className={`batch-status ${item.status}`}>{statusLabel[item.status]}</span><span><strong>{item.program.name}</strong><small>{item.message ?? t("app.message_174")}</small>{item.error && <em>{item.error}</em>}</span><b>{item.traces_found > 0 ? t("app.message_175", { value0: item.traces_found }) : item.status === "completed" ? t("app.message_176") : ""}</b></div>)}</div>
      </div>}
      <footer>
        <span>{started ? (active ? (paused ? t("app.message_177") : t("app.message_178")) : t("app.message_179")) : t("app.message_180")}</span>
        <button className="secondary-button" onClick={onClose}>{active ? t("app.message_181") : t("app.message_031")}</button>
        {!started ? <button className="primary-button" disabled={!isTauriRuntime() || selectedPrograms.length === 0} onClick={onStart}><Play size={14} fill="currentColor" />{t("app.message_183")}</button> : active && !paused ? <><button className="secondary-button" onClick={onPause}><Pause size={14} />{t("app.message_184")}</button><button className="danger-button" onClick={onCancel}>{t("app.message_185")}</button></> : active && paused ? <><button className="secondary-button" onClick={onCancel}>{t("app.message_186")}</button><button className="primary-button" onClick={onResume}><Play size={14} fill="currentColor" />{t("app.message_187")}</button></> : paused && queued > 0 ? <button className="primary-button" onClick={onResume}><Play size={14} fill="currentColor" />{t("app.message_188")}</button> : null}
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
  contextMenuEnabled,
  contextMenuLoading,
  hunterLoading,
  onPlan,
  onClean,
  onReset,
  onContextMenu,
  onHunter,
  onClose,
}: {
  initialPath: string;
  plan: import("./types").ForceUninstallPlan | null;
  result: import("./types").ForceUninstallResult | null;
  loading: boolean;
  error: string | null;
  contextMenuEnabled: boolean;
  contextMenuLoading: boolean;
  hunterLoading: boolean;
  onPlan: (path: string, name?: string) => Promise<import("./types").ForceUninstallPlan | null>;
  onClean: (traceIds: string[]) => Promise<import("./types").ForceUninstallResult | null>;
  onReset: () => void;
  onContextMenu: (enabled: boolean) => Promise<void>;
  onHunter: () => Promise<string | null>;
  onClose: () => void;
}) {
  const [path, setPath] = useState(initialPath === "—" ? "" : initialPath);
  const [name, setName] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [dragActive, setDragActive] = useState(false);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") {
        setDragActive(true);
      } else if (payload.type === "leave") {
        setDragActive(false);
      } else if (payload.type === "drop") {
        setDragActive(false);
        const droppedPath = payload.paths[0];
        if (!droppedPath) return;
        setPath(droppedPath);
        setName("");
        setSelectedIds(new Set());
        onReset();
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [onReset]);

  const updatePath = (value: string) => {
    setPath(value);
    setSelectedIds(new Set());
    onReset();
  };

  const updateName = (value: string) => {
    setName(value);
    setSelectedIds(new Set());
    onReset();
  };

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

  const submitHunter = async () => {
    const target = await onHunter();
    if (!target) return;
    updatePath(target);
  };

  return <div className="modal-backdrop" onMouseDown={onClose}>
    <section className="force-uninstall-modal" onMouseDown={(event) => event.stopPropagation()}>
      <header>
        <div><span className="force-modal-mark"><Wrench size={19} /></span><span><h2>{t("app.message_189")}</h2><p>{t("app.message_190")}</p></span></div>
        <button aria-label={t("app.message_191")} onClick={onClose}><X size={17} /></button>
      </header>
      {!plan && !result ? <>
        <div className="force-form">
          <div className="force-warning"><TriangleAlert size={17} /><span>{t("app.message_192")}</span></div>
          <div className={`force-drop-zone ${dragActive ? "active" : ""}`}><FolderOpen size={19} /><span><strong>{dragActive ? t("app.message_193") : t("app.message_194")}</strong><small>{t("app.message_195")}</small></span></div>
          <label>{t("app.message_196")}<input value={path} onChange={(event) => updatePath(event.target.value)} placeholder={t("app.message_197")} /></label>
          <label>{t("app.message_198")}<input value={name} onChange={(event) => updateName(event.target.value)} placeholder={t("app.message_199")} /></label>
          <div className="force-entry-actions"><button type="button" className="secondary-button compact-button" disabled={loading || hunterLoading} onClick={() => void submitHunter}>{hunterLoading ? <Loader2 size={13} className="spinning" /> : <Search size={13} />}{hunterLoading ? t("app.message_200") : t("app.message_201")}</button><button type="button" className={`secondary-button compact-button ${contextMenuEnabled ? "enabled" : ""}`} disabled={contextMenuLoading} onClick={() => void onContextMenu(!contextMenuEnabled)}>{contextMenuLoading ? <Loader2 size={13} className="spinning" /> : <ShieldCheck size={13} />}{contextMenuEnabled ? t("app.message_202") : t("app.message_203")}</button></div>
          <p className="force-entry-help">{t("app.message_204")}</p>
          {error && <div className="force-error" role="alert"><TriangleAlert size={15} />{error}</div>}
        </div>
        <footer><span>{t("app.message_205")}</span><button className="primary-button" disabled={loading || path.trim().length === 0 || !isTauriRuntime()} onClick={() => void submitPlan()}>{loading ? <Loader2 size={14} className="spinning" /> : <Search size={14} />}{t("app.message_206")}</button></footer>
      </> : result ? <>
        <div className={`force-result ${result.success ? "success" : "partial"}`}><span>{result.success ? <Check size={25} /> : <TriangleAlert size={25} />}</span><h2>{result.success ? t("app.message_207") : t("app.message_208")}</h2><p>{result.message}</p><div><strong>{result.traces_cleaned}</strong><small>{t("app.message_209")}</small><strong>{result.failed_count}</strong><small>{t("app.message_108")}</small><strong>{formatBytes(result.bytes_freed)}</strong><small>{t("app.message_211")}</small></div></div>
        {result.outcomes.some((outcome) => !outcome.success) && <div className="force-failed-list">{result.outcomes.filter((outcome) => !outcome.success).map((outcome) => <p key={outcome.trace_id}><TriangleAlert size={13} />{outcome.path}<small>{outcome.error ?? t("app.message_212")}</small></p>)}</div>}
        <footer><span>{t("app.message_213")}</span><button className="primary-button" onClick={onClose}>{t("app.message_031")}</button></footer>
      </> : <>
        <div className="force-plan-body">
          <div className="force-target-summary"><strong>{plan?.target.name}</strong><span>{plan?.target.resolved_path}</span><em>{plan?.target.kind === "shortcut" ? t("app.message_215") : plan?.target.kind === "executable" ? t("app.message_216") : t("app.message_217")}</em></div>
          {plan?.warnings.map((warning) => <p className="force-warning-line" key={warning}><Info size={13} />{warning}</p>)}
          <div className="force-trace-list"><h3>{t("app.message_218")} <small>{plan?.traces.length ?? 0}  {t("app.message_219")}</small></h3>{plan?.traces.map((trace) => <label className="force-trace-row" key={trace.id}><input type="checkbox" checked={selectedIds.has(trace.id)} onChange={() => toggleTrace(trace.id)} /><span className={`force-confidence ${trace.confidence}`}>{trace.confidence === "high" ? t("app.message_220") : trace.confidence === "medium" ? t("app.message_221") : t("app.message_082")}</span><span><strong>{trace.description || formatTraceType(trace.trace_type)}</strong><small>{trace.path}</small></span><b>{formatBytes(trace.size)}</b></label>)}</div>
          {error && <div className="force-error" role="alert"><TriangleAlert size={15} />{error}</div>}
        </div>
        <footer><button className="secondary-button" onClick={onClose}>{t("app.message_089")}</button><span className="force-selection-count">{t("app.message_224")} {selectedIds.size}  {t("app.message_167")}</span><button className="danger-button" disabled={loading || selectedIds.size === 0} onClick={submitCleanup}>{loading ? <Loader2 size={14} className="spinning" /> : <Trash2 size={14} />}{t("app.message_226")}</button></footer>
      </>}
    </section>
  </div>;
}

function ReviewStage(props: {
  traces: Trace[]; selectedIds: Set<string>; onToggle: (id: string) => void; onBack: () => void; onClean: () => void;
  allowBack: boolean; onSkip: () => void; onRescan: () => void; confirmOpen: boolean; onConfirmClose: () => void; onConfirm: () => void;
}) {
  const systemTraces = props.traces.filter((trace) => trace.trace_type === "scheduled_task" || trace.trace_type === "service" || trace.trace_type === "driver");
  const fileTraces = props.traces.filter((trace) => trace.trace_type !== "registry_key" && trace.trace_type !== "registry_value" && trace.trace_type !== "scheduled_task" && trace.trace_type !== "service" && trace.trace_type !== "driver");
  const registryTraces = props.traces.filter((trace) => trace.trace_type === "registry_key" || trace.trace_type === "registry_value");
  const selectedSize = props.traces.filter((trace) => props.selectedIds.has(trace.id)).reduce((sum, trace) => sum + (trace.size ?? 0), 0);
  return (
    <div className="page review-page">
      <SectionHeader title={t("app.message_227")} subtitle={t("app.message_228", { value0: props.traces.length, value1: formatBytes(props.traces.reduce((sum, trace) => sum + (trace.size ?? 0), 0)), value2: props.selectedIds.size })} action={<button className="link-button" onClick={props.onRescan}><RotateCcw size={14} />{t("app.message_229")}</button>} />
      <div className="review-tabs"><button className="active">{t("app.message_230")}{props.traces.length})</button><button>{t("app.message_231")}{fileTraces.length})</button><button>{t("app.message_232")}{registryTraces.length})</button><button>{t("app.message_233")}{systemTraces.length})</button></div>
      <div className="trace-table card-surface">
        <TraceGroup title={t("app.message_234", { value0: fileTraces.length })} traces={fileTraces} selectedIds={props.selectedIds} onToggle={props.onToggle} />
        <TraceGroup title={t("app.message_235", { value0: registryTraces.length })} traces={registryTraces} selectedIds={props.selectedIds} onToggle={props.onToggle} />
        <TraceGroup title={t("app.message_236", { value0: systemTraces.length })} traces={systemTraces} selectedIds={props.selectedIds} onToggle={props.onToggle} />
      </div>
      <div className="review-footer"><span>{t("app.message_237")}<strong>{formatBytes(selectedSize)}</strong></span><div><button className="secondary-button" onClick={props.onSkip}>{t("app.message_238")}</button>{props.allowBack && <button className="secondary-button" onClick={props.onBack}>{t("app.message_239")}</button>}<button className="primary-button" disabled={props.selectedIds.size === 0} onClick={props.onClean}><Trash2 size={16} />{t("app.message_240")}</button></div></div>
      {props.confirmOpen && <div className="modal-backdrop"><div className="safety-modal"><span className="modal-icon"><TriangleAlert size={24} /></span><h2>{t("app.message_241")}</h2><p>{t("app.message_242")} {props.selectedIds.size}  {t("app.message_243")}</p><div><button className="secondary-button" onClick={props.onConfirmClose}>{t("app.message_089")}</button><button className="danger-button" onClick={props.onConfirm}>{t("app.message_245")}</button></div></div></div>}
    </div>
  );
}

function TraceGroup({ title, traces, selectedIds, onToggle }: { title: string; traces: Trace[]; selectedIds: Set<string>; onToggle: (id: string) => void }) {
  return <section className="trace-group"><h3>{title}</h3><div className="trace-head"><span /><span>{t("app.message_246")}</span><span>{t("app.message_055")}</span><span>{t("app.message_248")}</span></div>{traces.map((trace) => <label className={`trace-row${trace.is_critical ? " protected" : ""}`} key={trace.id}><input type="checkbox" checked={selectedIds.has(trace.id)} disabled={trace.is_critical} onChange={() => onToggle(trace.id)} /><span className="fake-check"><Check size={12} /></span><TraceIcon type={trace.trace_type} /><span className="trace-name"><strong>{trace.description ?? t("app.message_249")}</strong><small>{trace.path}</small></span><span>{formatBytes(trace.size)}</span>{trace.is_critical ? <span className="confidence low">{t("app.message_250")}</span> : <ConfidenceBadge value={trace.confidence} />}</label>)}</section>;
}

function TraceIcon({ type }: { type: Trace["trace_type"] }) {
  const Icon = type === "registry_key" || type === "registry_value" ? Database : type === "shortcut" ? FileCode2 : type === "scheduled_task" || type === "service" || type === "driver" ? ShieldCheck : Folder;
  return <span className="trace-icon"><Icon size={15} /></span>;
}

function ConfidenceBadge({ value }: { value: Trace["confidence"] }) {
  const label = value === "high" ? t("app.message_251") : value === "medium" ? t("app.message_252") : t("app.message_253");
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
  const reportLogs = eventLogs.length > 0 ? eventLogs : [t("app.message_254")];
  return (
    <div className="page complete-page">
      <SectionHeader title={t("app.message_096")} subtitle={t("app.message_256")} />
      <div className="success-heading"><span><Check size={36} /></span><div><h2>{program.name}  {t("app.message_257")}</h2><p>{t("app.message_258")}</p></div></div>
      <div className="summary-grid card-surface"><div><span>{t("app.message_259")}</span><strong className="blue">{found}  {t("app.message_167")}</strong></div><div><span>{t("app.message_261")}</span><strong className="green">{removed}  {t("app.message_167")}</strong></div><div><span>{t("app.message_263")}</span><strong className="orange">{skipped}  {t("app.message_167")}</strong></div><div><span>{t("app.message_211")}</span><strong className="green">{formatBytes(freed)}</strong></div></div>
      <div className="report-panel card-surface"><div className="panel-title"><span><FileText size={14} />{t("app.message_266")}</span><button onClick={() => setReportOpen(true)}>{t("app.message_267")}</button></div><div className="report-category-grid">{summaries.filter((summary) => summary.count > 0).map((summary) => <div key={summary.category}><span>{summary.category === "files" ? t("app.message_022") : summary.category === "user_data" ? t("app.message_023") : summary.category === "registry" ? t("app.message_024") : t("app.message_025")}</span><strong>{summary.count}  {t("app.message_167")}</strong><small>{formatBytes(summary.bytes)}</small></div>)}{summaries.every((summary) => summary.count === 0) && <div className="report-empty">{t("app.message_273")}</div>}</div><div className="log-content report-log">{reportLogs.slice(-8).map((log, index) => <div key={`${log}-${index}`}>{log}</div>)}</div></div>
      <div className="complete-actions"><button className="secondary-button" onClick={() => setReportOpen(true)}><FileText size={16} />{t("app.message_274")}</button><button className="primary-button" onClick={onDone}><Check size={16} />{t("app.message_106")}</button></div>
      {reportOpen && <UninstallReportModal program={program} traces={traces} selectedIds={selectedIds} summaries={summaries} logs={reportLogs} onClose={() => setReportOpen(false)} />}
    </div>
  );
}

function UninstallReportModal({ program, traces, selectedIds, summaries, logs, onClose }: { program: UiProgram; traces: Trace[]; selectedIds: Set<string>; summaries: ReturnType<typeof summarizeTraces>; logs: string[]; onClose: () => void }) {
  return <div className="modal-backdrop" onMouseDown={onClose}><section className="uninstall-report-modal" onMouseDown={(event) => event.stopPropagation()}><header><div><span className="report-modal-mark"><FileText size={20} /></span><span><h2>{program.name}  {t("app.message_276")}</h2><p>{t("app.message_277")}</p></span></div><button aria-label={t("app.message_278")} onClick={onClose}><X size={17} /></button></header><div className="report-modal-summary">{summaries.filter((summary) => summary.count > 0).map((summary) => <div key={summary.category}><span>{summary.category === "files" ? t("app.message_022") : summary.category === "user_data" ? t("app.message_023") : summary.category === "registry" ? t("app.message_024") : t("app.message_025")}</span><strong>{summary.count}</strong><small>{formatBytes(summary.bytes)}</small></div>)}</div><div className="report-modal-body"><section className="report-event-column"><h3>{t("app.message_283")} <small>{logs.length}  {t("app.message_284")}</small></h3><div className="report-event-list">{logs.map((log, index) => <p key={`${log}-${index}`}>{log}</p>)}</div></section><section className="report-trace-column"><h3>{t("app.message_285")} <small>{traces.length}  {t("app.message_167")}</small></h3><div className="report-trace-list">{traces.length > 0 ? traces.map((trace) => <div className="report-trace-item" key={trace.id}><span className={`report-trace-state ${selectedIds.has(trace.id) ? "cleaned" : "kept"}`}>{selectedIds.has(trace.id) ? <Check size={12} /> : "—"}</span><span><strong>{trace.description || formatTraceType(trace.trace_type)}</strong><small>{trace.path}</small><em>{formatTraceType(trace.trace_type)} · {trace.confidence === "high" ? t("app.message_251") : trace.confidence === "medium" ? t("app.message_252") : t("app.message_253")} · {formatBytes(trace.size)}</em></span><b className={selectedIds.has(trace.id) ? "cleaned-status" : "kept-status"}>{selectedIds.has(trace.id) ? t("app.message_290") : t("app.message_291")}</b></div>) : <p className="report-empty">{t("app.message_292")}</p>}</div></section></div><footer><span>{t("app.message_293")}</span><button className="primary-button" onClick={onClose}>{t("app.message_294")}</button></footer></section></div>;
}

function ProgramInfoModal({ program, onClose }: { program: UiProgram; onClose: () => void }) {
  const source = program.source;
  const sourceLabel = source?.install_source === "msi" ? t("common.format.msi") : source?.install_source === "store" ? t("common.source.microsoft_store") : source?.install_source === "registry" ? t("app.message_024") : t("app.message_296");
  const rows: Array<[string, string | null | undefined, boolean?]> = [
    [t("app.message_297"), source?.id ?? program.id, true],
    [t("app.message_054"), source?.publisher ?? program.publisher],
    [t("app.message_063"), source?.display_version ?? source?.version ?? program.version],
    [t("app.message_300"), sourceLabel],
    [t("app.message_301"), source?.uninstall_kind],
    [t("app.message_056"), source?.install_date ?? program.installed],
    [t("app.message_303"), program.size],
    [t("app.message_304"), source?.install_location ?? program.location, true],
    [t("app.message_305"), source?.uninstall_string, true],
    [t("app.message_306"), source?.quiet_uninstall_string, true],
    [t("app.message_307"), source?.uninstall_registry_key_path, true],
  ];
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section className="program-info-modal" onMouseDown={(event) => event.stopPropagation()}>
        <header>
          <div><AppIcon program={program} large /><span><h2>{program.name}</h2><p>{program.publisher}</p></span></div>
          <button aria-label={t("app.message_308")} onClick={onClose}><X size={17} /></button>
        </header>
        <div className="program-info-rows">
          {rows.filter(([, value]) => Boolean(value)).map(([label, value, mono]) => (
            <div key={label}><dt>{label}</dt><dd className={mono ? "mono" : ""}>{value}</dd></div>
          ))}
        </div>
        <footer>
          {source?.install_location && <a href={`file:///${source.install_location}`} target="_blank" rel="noreferrer"><FolderOpen size={15} />{t("app.message_309")}</a>}
          <button className="secondary-button" onClick={onClose}>{t("app.message_031")}</button>
        </footer>
      </section>
    </div>
  );
}

function PlaceholderPage({ active }: { active: NavKey }) {
  const names: Record<NavKey, string> = { apps: t("app.message_032"), health: t("app.message_033"), startup: t("app.message_034"), cleaner: t("app.message_035"), shredder: t("shredder.nav"), backups: t("app.message_036"), traces: t("app.message_037"), monitor: t("app.message_038"), inventory: t("components.softwareinventory.nav"), reports: t("app.message_040"), plugins: t("app.message_041"), tools: t("app.message_042"), settings: t("app.message_043"), about: t("app.message_044") };
  return <div className="placeholder-page"><span><Sparkles size={30} /></span><h1>{names[active]}</h1><p>{t("app.message_324")}</p></div>;
}

function SettingsPage() {
  const currentLanguage = getLanguage();
  const languageNames: Record<Language, string> = {
    "zh-CN": t("settings.language.zh_cn"),
    "en-US": t("settings.language.en_us"),
  };

  return (
    <div className="page settings-page">
      <SectionHeader title={t("settings.title")} subtitle={t("settings.subtitle")} />
      <section className="settings-card card-surface">
        <div>
          <h2>{t("settings.language.title")}</h2>
          <p>{t("settings.language.description")}</p>
        </div>
        <div className="language-options" role="radiogroup" aria-label={t("settings.language.title")}>
          {supportedLanguages.map((language) => (
            <button
              type="button"
              role="radio"
              aria-checked={currentLanguage === language}
              className={currentLanguage === language ? "selected" : ""}
              key={language}
              onClick={() => setLanguage(language)}
            >
              <span>{languageNames[language]}</span>
              <small>{language}</small>
              {currentLanguage === language && <Check size={16} />}
            </button>
          ))}
        </div>
        <p className="settings-note"><Info size={14} />{t("settings.language.reload_note")}</p>
      </section>
      <CleanupSafetyRules />
    </div>
  );
}
