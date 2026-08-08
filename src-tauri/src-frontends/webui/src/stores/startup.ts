import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  StartupAction,
  StartupActionPlan,
  StartupActionResult,
  StartupEnvelope,
  StartupItem,
  StartupListResponse,
  StartupSource,
} from "../types";

export const startupSources: StartupSource[] = [
  "registry_run",
  "registry_run_once",
  "registry_policy_run",
  "startup_folder",
  "scheduled_task",
  "service",
];

interface StartupStateStore {
  items: StartupItem[];
  loading: boolean;
  actionLoading: boolean;
  error: string | null;
  sourceErrors: Partial<Record<StartupSource, string>>;
  selectedId: string | null;
  pendingPlan: StartupActionPlan | null;
  lastResult: StartupActionResult | null;
  loadItems: () => Promise<void>;
  selectItem: (id: string | null) => void;
  planAction: (item: StartupItem, action: StartupAction) => Promise<void>;
  cancelPlan: () => void;
  applyPendingAction: () => Promise<void>;
  rollbackLastAction: () => Promise<void>;
  clearResult: () => void;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function unwrapEnvelope<T>(envelope: StartupEnvelope<T>): T {
  if (!envelope.ok || envelope.data === null) {
    throw new Error(envelope.error?.message ?? "自启动操作失败");
  }
  return envelope.data;
}

async function loadSource(source: StartupSource): Promise<StartupItem[]> {
  const envelope = await invoke<StartupEnvelope<StartupListResponse>>("list_startup_items", {
    options: { source, sort_by: "name", include_raw: false },
  });
  return unwrapEnvelope(envelope).items;
}

export const useStartupStore = create<StartupStateStore>((set, get) => ({
  items: [],
  loading: false,
  actionLoading: false,
  error: null,
  sourceErrors: {},
  selectedId: null,
  pendingPlan: null,
  lastResult: null,

  loadItems: async () => {
    set({ loading: true, error: null, sourceErrors: {} });
    const results = await Promise.allSettled(startupSources.map(loadSource));
    const items: StartupItem[] = [];
    const sourceErrors: Partial<Record<StartupSource, string>> = {};

    results.forEach((result, index) => {
      const source = startupSources[index];
      if (result.status === "fulfilled") {
        items.push(...result.value);
      } else {
        sourceErrors[source] = errorMessage(result.reason);
      }
    });

    items.sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
    const allFailed = Object.keys(sourceErrors).length === startupSources.length;
    const selectedId = get().selectedId;
    set({
      items,
      loading: false,
      sourceErrors,
      error: allFailed ? "无法读取任何自启动来源，请检查系统权限后重试。" : null,
      selectedId: selectedId && items.some((item) => item.id === selectedId) ? selectedId : null,
    });
  },

  selectItem: (selectedId) => set({ selectedId }),

  planAction: async (item, action) => {
    set({ actionLoading: true, error: null, pendingPlan: null });
    try {
      const envelope = await invoke<StartupEnvelope<StartupActionPlan>>("plan_startup_action", {
        action,
        options: { id: item.id, reason: "用户在自启动管理页面确认操作" },
      });
      set({ pendingPlan: unwrapEnvelope(envelope), actionLoading: false });
    } catch (error) {
      set({ error: errorMessage(error), actionLoading: false });
    }
  },

  cancelPlan: () => set({ pendingPlan: null }),

  applyPendingAction: async () => {
    const plan = get().pendingPlan;
    if (!plan) return;

    set({ actionLoading: true, error: null });
    try {
      const envelope = await invoke<StartupEnvelope<StartupActionResult>>("apply_startup_action", {
        action: plan.action,
        options: { id: plan.item_id, reason: "用户在自启动管理页面确认操作" },
      });
      const result = unwrapEnvelope(envelope);
      set({ pendingPlan: null, lastResult: result, actionLoading: false });
      await get().loadItems();
    } catch (error) {
      set({ error: errorMessage(error), actionLoading: false });
    }
  },

  rollbackLastAction: async () => {
    const changeId = get().lastResult?.change_id;
    if (!changeId) return;

    set({ actionLoading: true, error: null });
    try {
      const envelope = await invoke<StartupEnvelope<StartupActionResult>>(
        "rollback_startup_action",
        { options: { change_id: changeId, reason: "用户撤销最近一次自启动变更" } },
      );
      unwrapEnvelope(envelope);
      set({ lastResult: null, actionLoading: false });
      await get().loadItems();
    } catch (error) {
      set({ error: errorMessage(error), actionLoading: false });
    }
  },

  clearResult: () => set({ lastResult: null }),
}));
