import { t } from "../i18n/index.ts";
import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  CommandError,
  EvidenceBundleExport,
  ReportExport,
  ReportExportFormat,
  ReportInfo,
  UninstallerReport,
} from "../types";

interface ReportsState {
  reports: ReportInfo[];
  selected: UninstallerReport | null;
  loading: boolean;
  actionLoading: boolean;
  error: string | null;
  notice: string | null;
  load: () => Promise<void>;
  open: (reportId: string) => Promise<void>;
  exportReport: (reportId: string, format: ReportExportFormat) => Promise<ReportExport | null>;
  exportEvidenceBundle: (reportId: string) => Promise<EvidenceBundleExport | null>;
  deleteReport: (reportId: string) => Promise<boolean>;
  clearMessages: () => void;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String((error as CommandError).message);
  return String(error);
}

export const useReportsStore = create<ReportsState>((set) => ({
  reports: [],
  selected: null,
  loading: false,
  actionLoading: false,
  error: null,
  notice: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const reports = await invoke<ReportInfo[]>("get_reports");
      set({ reports, loading: false });
    } catch (error) {
      set({ loading: false, error: errorMessage(error) });
    }
  },

  open: async (reportId) => {
    set({ actionLoading: true, error: null });
    try {
      const selected = await invoke<UninstallerReport>("get_report", { reportId });
      set({ selected, actionLoading: false });
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
    }
  },

  exportReport: async (reportId, format) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const result = await invoke<ReportExport>("export_report", { reportId, format });
      set({ actionLoading: false, notice: t("stores.reports.message_001", { value0: result.path }) });
      return result;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  exportEvidenceBundle: async (reportId) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const result = await invoke<EvidenceBundleExport>("export_evidence_bundle", { reportId });
      set({ actionLoading: false, notice: t("stores.reports.message_002", { value0: result.file_count, value1: result.path }) });
      return result;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return null;
    }
  },

  deleteReport: async (reportId) => {
    set({ actionLoading: true, error: null, notice: null });
    try {
      const deleted = await invoke<boolean>("delete_report", { reportId });
      set((state) => ({
        reports: state.reports.filter((report) => report.id !== reportId),
        selected: state.selected?.id === reportId ? null : state.selected,
        actionLoading: false,
        notice: deleted ? t("stores.reports.message_003") : t("stores.reports.message_004"),
      }));
      return deleted;
    } catch (error) {
      set({ actionLoading: false, error: errorMessage(error) });
      return false;
    }
  },

  clearMessages: () => set({ error: null, notice: null }),
}));
