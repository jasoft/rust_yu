import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  CommandError,
  ForceUninstallPlan,
  ForceUninstallResult,
} from "../types";

interface ForceUninstallState {
  plan: ForceUninstallPlan | null;
  result: ForceUninstallResult | null;
  loading: boolean;
  error: string | null;
  planTarget: (path: string, name?: string) => Promise<ForceUninstallPlan | null>;
  cleanSelected: (traceIds: string[]) => Promise<ForceUninstallResult | null>;
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

  reset: () => set({ plan: null, result: null, loading: false, error: null }),
}));
