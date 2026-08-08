import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { BrowserCleanupResult, BrowserScanResult, CommandError } from "../types";

interface BrowserCleanerState {
  scan: BrowserScanResult | null;
  result: BrowserCleanupResult | null;
  selectedIds: Set<string>;
  scanning: boolean;
  cleaning: boolean;
  error: string | null;
  scanData: () => Promise<void>;
  toggleItem: (id: string) => void;
  selectCaches: () => void;
  clearSelection: () => void;
  cleanSelected: () => Promise<void>;
}

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const useBrowserCleanerStore = create<BrowserCleanerState>((set, get) => ({
  scan: null,
  result: null,
  selectedIds: new Set(),
  scanning: false,
  cleaning: false,
  error: null,

  scanData: async () => {
    if (get().scanning) return;
    set({ scanning: true, error: null });
    try {
      const scan = await invoke<BrowserScanResult>("scan_browser_data");
      set({
        scan,
        selectedIds: new Set(scan.items.filter((item) => item.selected_by_default).map((item) => item.id)),
        scanning: false,
      });
    } catch (error) {
      set({ error: messageOf(error), scanning: false });
    }
  },

  toggleItem: (id) => {
    const selectedIds = new Set(get().selectedIds);
    if (selectedIds.has(id)) selectedIds.delete(id); else selectedIds.add(id);
    set({ selectedIds, result: null });
  },
  selectCaches: () => set({
    selectedIds: new Set((get().scan?.items ?? []).filter((item) => item.kind === "cache").map((item) => item.id)),
    result: null,
  }),
  clearSelection: () => set({ selectedIds: new Set(), result: null }),

  cleanSelected: async () => {
    const itemIds = [...get().selectedIds];
    if (itemIds.length === 0 || get().cleaning) return;
    set({ cleaning: true, error: null, result: null });
    try {
      const result = await invoke<BrowserCleanupResult>("clean_browser_data", {
        request: { item_ids: itemIds, dry_run: false, confirm: true },
      });
      const scan = await invoke<BrowserScanResult>("scan_browser_data");
      set({ result, scan, selectedIds: new Set(), cleaning: false });
    } catch (error) {
      set({ error: messageOf(error), cleaning: false });
    }
  },
}));
