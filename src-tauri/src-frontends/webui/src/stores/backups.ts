import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  BackupRestoreResult,
  BackupSessionInfo,
  CommandError,
} from "../types";

interface BackupsState {
  sessions: BackupSessionInfo[];
  loading: boolean;
  restoringId: string | null;
  error: string | null;
  notice: string | null;
  load: () => Promise<void>;
  restore: (sessionId: string) => Promise<BackupRestoreResult | null>;
  clearMessages: () => void;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const useBackupsStore = create<BackupsState>((set) => ({
  sessions: [],
  loading: false,
  restoringId: null,
  error: null,
  notice: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const sessions = await invoke<BackupSessionInfo[]>("list_backup_sessions");
      set({ sessions, loading: false });
    } catch (error) {
      set({ loading: false, error: errorMessage(error) });
    }
  },

  restore: async (sessionId) => {
    set({ restoringId: sessionId, error: null, notice: null });
    try {
      const result = await invoke<BackupRestoreResult>("restore_backup_session", { sessionId });
      set({
        restoringId: null,
        notice: result.success
          ? `已恢复 ${result.restored_count} 个项目`
          : `恢复完成，但有 ${result.failed_count} 个项目需要重试`,
      });
      await useBackupsStore.getState().load();
      return result;
    } catch (error) {
      set({ restoringId: null, error: errorMessage(error) });
      return null;
    }
  },

  clearMessages: () => set({ error: null, notice: null }),
}));
