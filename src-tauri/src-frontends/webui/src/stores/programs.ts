import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  InstalledProgram,
  ProgramListResponse,
  Trace,
  CleanResult,
  UninstallResult,
  CommandError,
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
  selectedTraces: Set<string>;
  cleanResults: CleanResult[];
  uninstallResult: UninstallResult | null;
  uninstalling: boolean;

  loadPrograms: (options?: { source?: string; search?: string; refresh?: boolean }) => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSourceFilter: (source: string) => void;
  selectProgram: (program: InstalledProgram | null) => void;
  setViewMode: (mode: ViewMode) => void;
  scanTraces: (programName: string) => Promise<void>;
  toggleTrace: (traceId: string) => void;
  toggleAllTraces: () => void;
  cleanTraces: (confirm: boolean) => Promise<void>;
  uninstallProgram: (options: {
    program_name: string;
    scan_only?: boolean;
    clean_after?: boolean;
    timeout_secs?: number;
    confirm?: boolean;
  }) => Promise<void>;
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
  selectedTraces: new Set(),
  cleanResults: [],
  uninstallResult: null,
  uninstalling: false,

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
        selectedTraces: new Set(existing.map((t) => t.id)),
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

  uninstallProgram: async (options) => {
    set({ uninstalling: true, uninstallResult: null });
    try {
      const result = await invoke<UninstallResult>("uninstall_program", {
        options,
      });
      set({ uninstallResult: result, uninstalling: false });
      get().loadPrograms({ refresh: true });
    } catch (e) {
      set({ error: extractErrorMessage(e), uninstalling: false });
    }
  },

  resetUninstall: () => set({ uninstallResult: null, uninstalling: false }),
  resetTraces: () =>
    set({ traces: [], selectedTraces: new Set(), cleanResults: [], viewMode: "detail" }),
}));

