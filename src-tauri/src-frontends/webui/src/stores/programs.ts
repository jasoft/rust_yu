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
} from "../types";

type ViewMode = "list" | "detail" | "uninstall" | "traces";

interface ProgramsState {
  programs: InstalledProgram[];
  loading: boolean;
  error: string | null;
  searchQuery: string;
  sourceFilter: string;
  selectedProgram: InstalledProgram | null;
  viewMode: ViewMode;
  traces: Trace[];
  tracesLoading: boolean;
  metadataLoading: boolean;
  selectedTraces: Set<string>;
  cleanResults: CleanResult[];
  uninstallResult: UninstallResult | null;
  uninstalling: boolean;
  uninstallJob: UninstallJob | null;

  loadPrograms: (options?: { source?: string; search?: string; refresh?: boolean }) => Promise<void>;
  warmupIcons: () => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSourceFilter: (source: string) => void;
  selectProgram: (program: InstalledProgram | null) => void;
  setViewMode: (mode: ViewMode) => void;
  scanTraces: (programName: string) => Promise<void>;
  toggleTrace: (traceId: string) => void;
  toggleAllTraces: () => void;
  cleanTraces: (confirm: boolean) => Promise<void>;
  planUninstall: (programId: string) => Promise<UninstallJob | null>;
  executeUninstall: (jobId: string, timeoutSecs?: number) => Promise<UninstallJob | null>;
  cleanUninstallResidues: (jobId: string, selection: CleanupSelection) => Promise<UninstallJob | null>;
  finishUninstall: (jobId: string) => Promise<UninstallJob | null>;
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
  uninstallJob: null,

  loadPrograms: async (options) => {
    set({ loading: true, error: null });
    try {
      const response = await invoke<ProgramListResponse>("list_programs", {
        options: {
          source: options?.source ?? get().sourceFilter,
          search: options?.search ?? (get().searchQuery || undefined),
          refresh: options?.refresh ?? false,
        },
      });
      set({ programs: response.programs, loading: false });
    } catch (e) {
      set({ error: extractErrorMessage(e), loading: false });
    }
  },

  warmupIcons: async () => {
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
    } catch (e) {
      set({ error: extractErrorMessage(e) });
    } finally {
      set({ metadataLoading: false });
    }
  },

  setSearchQuery: (query) => set({ searchQuery: query }),
  setSourceFilter: (source) => {
    set({ sourceFilter: source });
    get().loadPrograms({ source });
  },

  selectProgram: (program) =>
    set({ selectedProgram: program, viewMode: program ? "detail" : "list" }),

  setViewMode: (mode) => set({ viewMode: mode }),

  scanTraces: async (programName) => {
    set({ tracesLoading: true, traces: [], selectedTraces: new Set() });
    try {
      const traces = await invoke<Trace[]>("scan_traces", {
        programName,
        traceTypes: null,
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
    if (selectedTraces.size === traces.length) {
      set({ selectedTraces: new Set() });
    } else {
      set({ selectedTraces: new Set(traces.map((t) => t.id)) });
    }
  },

  cleanTraces: async (confirm) => {
    const { traces, selectedTraces } = get();
    const toClean = traces.filter((t) => selectedTraces.has(t.id));
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
      set({ uninstallJob: response.job, uninstalling: false });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false });
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
        uninstalling: false,
      });
      if (response.job.phase === "completed") void get().loadPrograms({ refresh: true });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false });
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
        uninstalling: false,
      });
      if (response.job.phase === "completed") void get().loadPrograms({ refresh: true });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false });
      return null;
    }
  },

  finishUninstall: async (jobId) =>
    get().cleanUninstallResidues(jobId, { trace_ids: [], confirm: true }),

  getUninstallJob: async (jobId) => {
    try {
      const response = await invoke<UninstallJobResponse>("get_uninstall_job", { jobId });
      set({ uninstallJob: response.job, uninstallResult: response.job.outcome });
      return response.job;
    } catch (e) {
      set({ error: extractErrorMessage(e) });
      return null;
    }
  },

  resetUninstall: () => set({ uninstallResult: null, uninstalling: false, uninstallJob: null }),
  resetTraces: () =>
    set({ traces: [], selectedTraces: new Set(), cleanResults: [], viewMode: "detail" }),
}));

