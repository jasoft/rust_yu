import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { CommandError, ShredMethod, ShredPlan, ShredProgress, ShredResult } from "../types";

interface FileShredderState {
  paths: string[];
  method: ShredMethod;
  plan: ShredPlan | null;
  result: ShredResult | null;
  progress: ShredProgress | null;
  planning: boolean;
  shredding: boolean;
  error: string | null;
  addPaths: (paths: string[]) => void;
  removePath: (path: string) => void;
  clear: () => void;
  setMethod: (method: ShredMethod) => void;
  analyze: () => Promise<void>;
  execute: (confirmationText: string) => Promise<void>;
  setProgress: (progress: ShredProgress) => void;
}

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String((error as CommandError).message);
  return String(error);
}

export const useFileShredderStore = create<FileShredderState>((set, get) => ({
  paths: [], method: "standard", plan: null, result: null, progress: null,
  planning: false, shredding: false, error: null,
  addPaths: (incoming) => set((state) => ({
    paths: [...new Set([...state.paths, ...incoming])],
    plan: null, result: null, error: null,
  })),
  removePath: (path) => set((state) => ({
    paths: state.paths.filter((item) => item !== path),
    plan: null, result: null, error: null,
  })),
  clear: () => set({ paths: [], plan: null, result: null, progress: null, error: null }),
  setMethod: (method) => set({ method, plan: null, result: null, progress: null, error: null }),
  analyze: async () => {
    const { paths, method } = get();
    if (paths.length === 0 || get().planning) return;
    set({ planning: true, plan: null, result: null, progress: null, error: null });
    try {
      const plan = await invoke<ShredPlan>("plan_file_shred", { paths, method });
      set({ plan, planning: false });
    } catch (error) {
      set({ error: messageOf(error), planning: false });
    }
  },
  execute: async (confirmationText) => {
    const { paths, method, plan } = get();
    if (!plan || get().shredding) return;
    set({ shredding: true, result: null, progress: null, error: null });
    try {
      const result = await invoke<ShredResult>("execute_file_shred", { request: {
        paths,
        method,
        confirmation_token: plan.confirmation_token,
        confirmation_text: confirmationText,
        confirm: true,
        dry_run: false,
      }});
      set({ result, shredding: false, plan: null, paths: result.failures.map((item) => item.path) });
    } catch (error) {
      set({ error: messageOf(error), shredding: false });
    }
  },
  setProgress: (progress) => set({ progress }),
}));
