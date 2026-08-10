import { t } from "../i18n/index.ts";
import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  CommandError,
  InstallMonitorPlan,
  InstallMonitorSession,
  InstallMonitorSessionInfo,
  InstallMonitorStartRequest,
  MonitorExport,
  Trace,
} from "../types";

interface InstallMonitorState {
  plan: InstallMonitorPlan | null;
  sessions: InstallMonitorSessionInfo[];
  selectedSession: InstallMonitorSession | null;
  activeSessionId: string | null;
  loading: boolean;
  actionLoading: boolean;
  error: string | null;
  notice: string | null;
  load: () => Promise<void>;
  planFor: (request: InstallMonitorStartRequest) => Promise<InstallMonitorPlan | null>;
  start: (request: InstallMonitorStartRequest) => Promise<InstallMonitorSessionInfo | null>;
  complete: (sessionId: string) => Promise<InstallMonitorSession | null>;
  cancel: (sessionId: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  select: (sessionId: string | null) => Promise<void>;
  exportSession: (sessionId: string, format: "json" | "csv") => Promise<MonitorExport | null>;
  getTraces: (sessionId: string) => Promise<Trace[]>;
  clearMessages: () => void;
  setPlan: (plan: InstallMonitorPlan | null) => void;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const useInstallMonitorStore = create<InstallMonitorState>((set, get) => ({
  plan: null,
  sessions: [],
  selectedSession: null,
  activeSessionId: null,
  loading: false,
  actionLoading: false,
  error: null,
  notice: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const sessions = await invoke<InstallMonitorSessionInfo[]>("list_install_monitor_sessions");
      set({ sessions, loading: false });
    } catch (error) {
      set({ loading: false, error: errorMessage(error) });
    }
  },

  planFor: async (request) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const plan = await invoke<InstallMonitorPlan>("plan_install_monitor", { request });
      set({ plan, actionLoading: false });
      return plan;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  start: async (request) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const session = await invoke<InstallMonitorSessionInfo>("start_install_monitor", { request });
      set({ actionLoading: false, activeSessionId: session.id, plan: null, notice: t("stores.installmonitor.message_001") });
      await get().load();
      return session;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  complete: async (sessionId) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const session = await invoke<InstallMonitorSession>("complete_install_monitor", { sessionId });
      set({ actionLoading: false, activeSessionId: null, selectedSession: session, notice: t("stores.installmonitor.message_002", { value0: session.changes.length }) });
      await get().load();
      return session;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  cancel: async (sessionId) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const session = await invoke<InstallMonitorSession>("cancel_install_monitor", { sessionId });
      set({ actionLoading: false, activeSessionId: null, selectedSession: session, notice: t("stores.installmonitor.message_003") });
      await get().load();
    } catch (error) { set({ actionLoading: false, error: errorMessage(error) }); }
  },

  deleteSession: async (sessionId) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      await invoke<boolean>("delete_install_monitor", { sessionId });
      set({ actionLoading: false, selectedSession: null, notice: t("stores.installmonitor.message_004") });
      await get().load();
    } catch (error) { set({ actionLoading: false, error: errorMessage(error) }); }
  },

  select: async (sessionId) => {
    if (!sessionId) {
      set({ selectedSession: null });
      return;
    }
    set({ actionLoading: true, error: null });
    try {
      const session = await invoke<InstallMonitorSession>("get_install_monitor_session", { sessionId });
      set({ selectedSession: session, actionLoading: false });
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
    }
  },

  exportSession: async (sessionId, format) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const result = await invoke<MonitorExport>("export_install_monitor", { sessionId, format });
      set({ actionLoading: false, notice: t("stores.installmonitor.message_005", { value0: result.changes_count, value1: result.path }) });
      return result;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  getTraces: async (sessionId) => {
    try {
      return await invoke<Trace[]>("get_install_monitor_traces", { sessionId });
    } catch (error) {
      set({ error: errorMessage(error) });
      return [];
    }
  },

  clearMessages: () => set({ error: null, notice: null }),
  setPlan: (plan) => set({ plan }),
}));
