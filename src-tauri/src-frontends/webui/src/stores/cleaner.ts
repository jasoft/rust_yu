import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { CleanerCatalog, CleanerCleanResult, CleanerScanResult, CommandError } from "../types";

interface CleanerState {
  catalog: CleanerCatalog | null;
  scan: CleanerScanResult | null;
  result: CleanerCleanResult | null;
  selectedEntries: Set<string>;
  selectedTargets: Set<string>;
  loadingCatalog: boolean;
  scanning: boolean;
  cleaning: boolean;
  error: string | null;
  logs: string[];
  loadCatalog: () => Promise<void>;
  toggleEntry: (id: string) => void;
  selectRecommended: () => void;
  clearEntries: () => void;
  analyze: () => Promise<void>;
  toggleTarget: (id: string) => void;
  selectAllSafeTargets: () => void;
  clearTargets: () => void;
  clean: () => Promise<void>;
  appendLog: (message: string) => void;
}

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String((error as CommandError).message);
  return String(error);
}

export const useCleanerStore = create<CleanerState>((set, get) => ({
  catalog: null, scan: null, result: null,
  selectedEntries: new Set(), selectedTargets: new Set(),
  loadingCatalog: false, scanning: false, cleaning: false, error: null, logs: [],

  loadCatalog: async () => {
    if (get().loadingCatalog) return;
    set({ loadingCatalog: true, error: null });
    try {
      set({ catalog: await invoke<CleanerCatalog>("list_cleaner_entries"), loadingCatalog: false });
    } catch (error) { set({ error: messageOf(error), loadingCatalog: false }); }
  },
  toggleEntry: (id) => {
    const selectedEntries = new Set(get().selectedEntries);
    if (selectedEntries.has(id)) selectedEntries.delete(id); else selectedEntries.add(id);
    set({ selectedEntries, scan: null, selectedTargets: new Set(), result: null });
  },
  selectRecommended: () => set({
    selectedEntries: new Set((get().catalog?.entries ?? []).filter((item) => item.default_enabled).map((item) => item.id)),
    scan: null, selectedTargets: new Set(), result: null,
  }),
  clearEntries: () => set({ selectedEntries: new Set(), scan: null, selectedTargets: new Set(), result: null }),
  analyze: async () => {
    const entryIds = [...get().selectedEntries];
    if (entryIds.length === 0) return;
    set({ scanning: true, error: null, result: null, selectedTargets: new Set() });
    try {
      set({ scan: await invoke<CleanerScanResult>("scan_cleaner_entries", { entryIds }), scanning: false });
    } catch (error) { set({ error: messageOf(error), scanning: false }); }
  },
  toggleTarget: (id) => {
    const target = get().scan?.targets.find((item) => item.id === id);
    if (!target || target.blocked_reason) return;
    const selectedTargets = new Set(get().selectedTargets);
    if (selectedTargets.has(id)) selectedTargets.delete(id); else selectedTargets.add(id);
    set({ selectedTargets });
  },
  selectAllSafeTargets: () => set({ selectedTargets: new Set((get().scan?.targets ?? [])
    .filter((item) => !item.blocked_reason).map((item) => item.id)) }),
  clearTargets: () => set({ selectedTargets: new Set() }),
  clean: async () => {
    const targetIds = [...get().selectedTargets];
    if (targetIds.length === 0) return;
    set({ cleaning: true, error: null });
    try {
      const result = await invoke<CleanerCleanResult>("clean_cleaner_entries", { selection: {
        entry_ids: [...get().selectedEntries], target_ids: targetIds, confirm: true, dry_run: false,
      }});
      set({ result, cleaning: false, selectedTargets: new Set() });
    } catch (error) { set({ error: messageOf(error), cleaning: false }); }
  },
  appendLog: (message) => set((state) => ({ logs: [...state.logs.slice(-99), message] })),
}));
