import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  InstalledProgram,
  ProgramListResponse,
  Trace,
  CleanResult,
  UninstallResult,
  CommandError,
  CleanupSelection,
  UninstallJob,
  UninstallJobResponse,
  MetadataWarmupProgress,
} from "../types";
import type { ProgramSourceFilter } from "../lib/programFilters";
import { hasMissingProgramIcons } from "../lib/programFilters";

type ViewMode = "list" | "detail" | "uninstall" | "traces";

interface ProgramsState {
  programs: InstalledProgram[];
  loading: boolean;
  error: string | null;
  searchQuery: string;
  sourceFilter: ProgramSourceFilter;
  selectedProgram: InstalledProgram | null;
  viewMode: ViewMode;
  traces: Trace[];
  tracesLoading: boolean;
  metadataLoading: boolean;
  selectedTraces: Set<string>;
  cleanResults: CleanResult[];
  uninstallResult: UninstallResult | null;
  uninstalling: boolean;
  uninstallCancelling: boolean;
  uninstallJob: UninstallJob | null;

  loadPrograms: (options?: { refresh?: boolean }) => Promise<void>;
  reloadPrograms: (options?: { refresh?: boolean }) => Promise<void>;
  refreshProgramsInBackground: () => Promise<void>;
  applyMetadataProgress: (progress: MetadataWarmupProgress) => void;
  warmupIcons: () => Promise<boolean>;
  setSearchQuery: (query: string) => void;
  setSourceFilter: (source: ProgramSourceFilter) => void;
  selectProgram: (program: InstalledProgram | null) => void;
  setViewMode: (mode: ViewMode) => void;
  scanTraces: (programName: string, program?: InstalledProgram) => Promise<void>;
  toggleTrace: (traceId: string) => void;
  toggleAllTraces: () => void;
  cleanTraces: (confirm: boolean) => Promise<void>;
  planUninstall: (programId: string) => Promise<UninstallJob | null>;
  executeUninstall: (jobId: string, timeoutSecs?: number) => Promise<UninstallJob | null>;
  cleanUninstallResidues: (jobId: string, selection: CleanupSelection) => Promise<UninstallJob | null>;
  finishUninstall: (jobId: string) => Promise<UninstallJob | null>;
  cancelUninstall: (jobId: string) => Promise<boolean>;
  getUninstallJob: (jobId: string) => Promise<UninstallJob | null>;
  resetUninstall: () => void;
  resetTraces: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

let backgroundRefreshPromise: Promise<void> | null = null;

export const useProgramsStore = create<ProgramsState>((set, get) => ({
  programs: [],
  loading: false,
  error: null,
  searchQuery: "",
  sourceFilter: "all",
  selectedProgram: null,
  viewMode: "list",
  traces: [],
  tracesLoading: false,
  metadataLoading: false,
  selectedTraces: new Set(),
  cleanResults: [],
  uninstallResult: null,
  uninstalling: false,
  uninstallCancelling: false,
  uninstallJob: null,

  loadPrograms: async (options) => {
    set({ loading: true, error: null });
    try {
      const response = await invoke<ProgramListResponse>("list_programs", {
        options: {
          // 始终保留全量列表；来源标签和搜索在前端即时筛选，
          // 避免切换标签重新扫描后丢失已经预热的图标元数据。
          source: "all",
          search: undefined,
          refresh: options?.refresh ?? false,
        },
      });
      set({ programs: response.programs, loading: false });
    } catch (e) {
      set({ error: extractErrorMessage(e), loading: false });
    }
  },

  reloadPrograms: async (options) => {
    // 先显示 SQLite 中的上次结果；即使缓存过期也不能把列表清空。
    await get().loadPrograms({ refresh: false });
    if (get().error) return;

    const refreshPromise = get().refreshProgramsInBackground();
    // 手动刷新按钮需要等待清单刷新完成；启动路径则立刻返回，把所有 I/O
    // 放到后台，用户可以马上使用已有列表。
    if (options?.refresh) await refreshPromise;
    else void refreshPromise;
  },

  refreshProgramsInBackground: async () => {
    if (backgroundRefreshPromise) return backgroundRefreshPromise;

    backgroundRefreshPromise = (async () => {
      try {
        const response = await invoke<ProgramListResponse>("list_programs", {
          options: { source: "all", search: undefined, refresh: true },
        });
        // 增量扫描完成后才替换清单；扫描期间旧列表仍然可交互。
        set({ programs: response.programs, error: null });
        set({ metadataLoading: true });

        await invoke("warmup_program_metadata", {
          options: {
            source: "all",
            refresh: false,
            icons: true,
            sizes: true,
            progress_event: "installed-program-metadata-progress",
          },
        });

        // 预热按应用逐项写入 SQLite，最后只读取缓存，不再次扫描系统。
        const finalResponse = await invoke<ProgramListResponse>("list_programs", {
          options: { source: "all", search: undefined, refresh: false },
        });
        set({ programs: finalResponse.programs, error: null });
      } catch (e) {
        // 后台刷新失败不能覆盖已经可用的缓存列表；仅保留错误供状态栏显示。
        set({ error: extractErrorMessage(e) });
      } finally {
        set({ metadataLoading: false });
        backgroundRefreshPromise = null;
      }
    })();

    return backgroundRefreshPromise;
  },

  applyMetadataProgress: (progress) => {
    const updated = progress.program;
    if (!updated) return;
    set((state) => ({
      programs: state.programs.map((program) =>
        program.id === updated.id ? updated : program,
      ),
      selectedProgram:
        state.selectedProgram?.id === updated.id ? updated : state.selectedProgram,
    }));
  },

  warmupIcons: async () => {
    // 后端读取缓存时会清除已丢失的路径；只有确实缺少图标时才启动预热。
    if (!hasMissingProgramIcons(get().programs)) return false;
    set({ metadataLoading: true, error: null });
    try {
      await invoke("warmup_program_metadata", {
        options: {
          source: "all",
          refresh: false,
          icons: true,
          sizes: false,
          progress_event: "installed-program-metadata-progress",
        },
      });
      return true;
    } catch (e) {
      set({ error: extractErrorMessage(e) });
      return false;
    } finally {
      set({ metadataLoading: false });
    }
  },

  setSearchQuery: (query) => set({ searchQuery: query }),
  setSourceFilter: (source) => set({ sourceFilter: source }),

  selectProgram: (program) =>
    set({ selectedProgram: program, viewMode: program ? "detail" : "list" }),

  setViewMode: (mode) => set({ viewMode: mode }),

  scanTraces: async (programName, program) => {
    set({
      tracesLoading: true,
      traces: [],
      selectedTraces: new Set(),
      cleanResults: [],
      error: null,
    });
    try {
      const traces = await invoke<Trace[]>("scan_traces", {
        programName,
        traceTypes: null,
        program: program ?? null,
      });
      const existing = traces.filter((t) => t.exists);
      set({
        traces: existing,
        tracesLoading: false,
        // 残留项默认不勾选，避免用户在未逐项确认前误触发删除。
        selectedTraces: new Set(),
        viewMode: "traces",
      });
    } catch (e) {
      set({ tracesLoading: false, error: extractErrorMessage(e) });
    }
  },

  toggleTrace: (traceId) => {
    const trace = get().traces.find((candidate) => candidate.id === traceId);
    if (trace?.is_critical) return;
    const current = new Set(get().selectedTraces);
    if (current.has(traceId)) {
      current.delete(traceId);
    } else {
      current.add(traceId);
    }
    set({ selectedTraces: current });
  },

  toggleAllTraces: () => {
    const { traces, selectedTraces } = get();
    const selectableTraces = traces.filter((trace) => !trace.is_critical);
    if (selectedTraces.size === selectableTraces.length) {
      set({ selectedTraces: new Set() });
    } else {
      set({ selectedTraces: new Set(selectableTraces.map((trace) => trace.id)) });
    }
  },

  cleanTraces: async (confirm) => {
    const { traces, selectedTraces } = get();
    const toClean = traces.filter((t) => selectedTraces.has(t.id));
    set({ cleanResults: [], error: null });
    try {
      const results = await invoke<CleanResult[]>("clean_traces", {
        options: { traces: toClean, confirm, preview: false },
      });
      set({ cleanResults: results });
    } catch (e) {
      set({ error: extractErrorMessage(e) });
    }
  },

  planUninstall: async (programId) => {
    set({ uninstalling: true, uninstallResult: null, error: null });
    try {
      const response = await invoke<UninstallJobResponse>("plan_uninstall", {
        request: { program_id: programId },
      });
      set({ uninstallJob: response.job, uninstalling: false, uninstallCancelling: false });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false, uninstallCancelling: false });
      return null;
    }
  },

  executeUninstall: async (jobId, timeoutSecs = 120) => {
    set({ uninstalling: true, error: null });
    try {
      const response = await invoke<UninstallJobResponse>("execute_uninstall", {
        request: { job_id: jobId, timeout_secs: timeoutSecs },
      });
      set({
        uninstallJob: response.job,
        uninstallResult: response.job.outcome,
        cleanResults: response.job.cleanup_results,
        uninstalling: false,
        uninstallCancelling: false,
      });
      if (response.job.phase === "completed") void get().loadPrograms({ refresh: true });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false, uninstallCancelling: false });
      return null;
    }
  },

  cleanUninstallResidues: async (jobId, selection) => {
    set({ uninstalling: true, error: null });
    try {
      const response = await invoke<UninstallJobResponse>("clean_uninstall_residues", {
        request: { job_id: jobId, selection },
      });
      set({
        uninstallJob: response.job,
        uninstallResult: response.job.outcome,
        cleanResults: response.job.cleanup_results,
        uninstalling: false,
        uninstallCancelling: false,
      });
      if (response.job.phase === "completed") void get().loadPrograms({ refresh: true });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false, uninstallCancelling: false });
      return null;
    }
  },

  finishUninstall: async (jobId) =>
    get().cleanUninstallResidues(jobId, { trace_ids: [], confirm: true }),

  cancelUninstall: async (jobId) => {
    set({ uninstallCancelling: true, error: null });
    try {
      await invoke("cancel_uninstall", { request: { job_id: jobId } });
      return true;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstallCancelling: false });
      return false;
    }
  },

  getUninstallJob: async (jobId) => {
    try {
      const response = await invoke<UninstallJobResponse>("get_uninstall_job", { jobId });
      set({ uninstallJob: response.job, uninstallResult: response.job.outcome, cleanResults: response.job.cleanup_results });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e) });
      return null;
    }
  },

  resetUninstall: () => set({ uninstallResult: null, cleanResults: [], uninstalling: false, uninstallCancelling: false, uninstallJob: null }),
  resetTraces: () =>
    set({ traces: [], selectedTraces: new Set(), cleanResults: [], viewMode: "detail" }),
}));

