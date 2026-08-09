import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { CommandError, HealthReport } from "../types";

interface HealthState {
  report: HealthReport | null;
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const useHealthStore = create<HealthState>((set) => ({
  report: null,
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const report = await invoke<HealthReport>("get_program_health");
      set({ report, loading: false });
    } catch (error) {
      set({ error: errorMessage(error), loading: false });
    }
  },
}));
