import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  CommandError,
  ForceUninstallPlan,
  ForceUninstallResult,
} from "../types";

interface ContextMenuStatus {
  enabled: boolean;
  command: string | null;
}

interface ForceUninstallState {
  plan: ForceUninstallPlan | null;
  result: ForceUninstallResult | null;
  loading: boolean;
  error: string | null;
  contextMenuEnabled: boolean;
  contextMenuLoading: boolean;
  hunterLoading: boolean;
  planTarget: (path: string, name?: string) => Promise<ForceUninstallPlan | null>;
  cleanSelected: (traceIds: string[]) => Promise<ForceUninstallResult | null>;
  loadContextMenu: () => Promise<void>;
  setContextMenuEnabled: (enabled: boolean) => Promise<void>;
  captureHunterTarget: () => Promise<string | null>;
  reset: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const useForceUninstallStore = create<ForceUninstallState>((set, get) => ({
  plan: null,
  result: null,
  loading: false,
  error: null,
  contextMenuEnabled: false,
  contextMenuLoading: false,
  hunterLoading: false,

  planTarget: async (path, name) => {
    set({ loading: true, error: null, result: null, plan: null });
    try {
      const plan = await invoke<ForceUninstallPlan>("plan_force_uninstall", {
        request: { path, name: name?.trim() || null },
      });
      set({ plan, loading: false });
      return plan;
    } catch (error) {
      set({ loading: false, error: extractErrorMessage(error) });
      return null;
    }
  },

  cleanSelected: async (traceIds) => {
    const plan = get().plan;
    if (!plan) {
      set({ error: "请先生成强制卸载计划" });
      return null;
    }
    set({ loading: true, error: null });
    try {
      const result = await invoke<ForceUninstallResult>("clean_force_uninstall", {
        request: {
          plan,
          selection: {
            plan_id: plan.plan_id,
            trace_ids: traceIds,
            confirm: true,
          },
        },
      });
      set({ result, loading: false });
      return result;
    } catch (error) {
      set({ loading: false, error: extractErrorMessage(error) });
      return null;
    }
  },

  loadContextMenu: async () => {
    try {
      const status = await invoke<ContextMenuStatus>("get_force_uninstall_context_menu");
      set({ contextMenuEnabled: status.enabled });
    } catch (error) {
      set({ error: extractErrorMessage(error) });
    }
  },

  setContextMenuEnabled: async (enabled) => {
    set({ contextMenuLoading: true, error: null });
    try {
      const status = await invoke<ContextMenuStatus>("set_force_uninstall_context_menu", { enabled });
      set({ contextMenuEnabled: status.enabled, contextMenuLoading: false });
    } catch (error) {
      set({ contextMenuLoading: false, error: extractErrorMessage(error) });
    }
  },

  captureHunterTarget: async () => {
    set({ hunterLoading: true, error: null });
    try {
      const path = await invoke<string>("capture_hunter_target", { timeoutSecs: 15 });
      set({ hunterLoading: false });
      return path;
    } catch (error) {
      set({ hunterLoading: false, error: extractErrorMessage(error) });
      return null;
    }
  },

  reset: () => set({ plan: null, result: null, loading: false, error: null }),
}));
