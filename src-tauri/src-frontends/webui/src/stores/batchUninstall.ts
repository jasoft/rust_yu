import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  BatchUninstallItem,
  CommandError,
  InstalledProgram,
  UninstallJob,
  UninstallJobResponse,
} from "../types";

interface BatchUninstallState {
  items: BatchUninstallItem[];
  active: boolean;
  paused: boolean;
  cancelRequested: boolean;
  error: string | null;
  startQueue: (programs: InstalledProgram[]) => Promise<void>;
  pauseQueue: () => void;
  resumeQueue: () => void;
  cancelQueue: () => void;
  reset: () => void;
}

interface QueueError {
  code: string | null;
  message: string;
}

const wait = (milliseconds: number) => new Promise<void>((resolve) => setTimeout(resolve, milliseconds));

function describeError(error: unknown): QueueError {
  if (typeof error === "string") return { code: null, message: error };
  if (error && typeof error === "object") {
    const candidate = error as Partial<CommandError>;
    return {
      code: candidate.code ? String(candidate.code) : null,
      message: candidate.message ? String(candidate.message) : String(error),
    };
  }
  return { code: null, message: String(error) };
}

function createItem(program: InstalledProgram): BatchUninstallItem {
  return {
    program,
    status: "queued",
    job_id: null,
    job: null,
    message: null,
    error: null,
    traces_found: 0,
    traces: [],
    bytes_freed: 0,
  };
}

function jobMessage(job: UninstallJob): string {
  return job.outcome?.message ?? (job.phase === "failed" ? "卸载任务失败" : "卸载任务已完成");
}

export const useBatchUninstallStore = create<BatchUninstallState>((set, get) => {
  const patchItem = (programId: string, patch: Partial<BatchUninstallItem>) => {
    set((state) => ({
      items: state.items.map((item) =>
        item.program.id === programId ? { ...item, ...patch } : item,
      ),
    }));
  };

  const cancelQueuedItems = (message: string) => {
    set((state) => ({
      items: state.items.map((item) =>
        item.status === "queued"
          ? { ...item, status: "cancelled", message, error: null }
          : item,
      ),
    }));
  };

  const pauseForConflict = (programId: string, error: QueueError) => {
    patchItem(programId, { status: "queued", message: null, error: null });
    set({ active: false, paused: true, error: `队列已暂停：${error.message}` });
  };

  const stopForAdministratorError = (programId: string, error: QueueError) => {
    patchItem(programId, { status: "failed", message: error.message, error: error.message });
    cancelQueuedItems("未执行：需要管理员权限");
    set({ active: false, paused: false, error: error.message });
  };

  const finishItemFromJob = (programId: string, job: UninstallJob, message?: string) => {
    const traces = job.residue_review.traces;
    const outcome = job.outcome;
    patchItem(programId, {
      status: job.phase === "completed" ? "completed" : "failed",
      job_id: job.snapshot.job_id,
      job,
      message: message ?? jobMessage(job),
      error: job.phase === "completed" ? null : jobMessage(job),
      traces_found: outcome?.traces_found ?? traces.length,
      traces,
      bytes_freed: outcome?.bytes_freed ?? 0,
    });
  };

  const runQueue = async () => {
    // 每个程序重新走现有的 plan -> execute -> finish 命令，因此每项拥有独立
    // job_id、指纹核验和报告；队列本身只负责串行调度，不把多个删除操作并发化。
    for (const queuedItem of get().items) {
      if (get().cancelRequested) {
        cancelQueuedItems("用户取消了后续队列项");
        break;
      }

      while (get().paused && !get().cancelRequested) await wait(120);
      if (get().cancelRequested) {
        cancelQueuedItems("用户取消了后续队列项");
        break;
      }

      const current = get().items.find((item) => item.program.id === queuedItem.program.id);
      if (!current || current.status !== "queued") continue;

      patchItem(current.program.id, { status: "planning", error: null, message: null });
      let planned: UninstallJobResponse;
      try {
        planned = await invoke<UninstallJobResponse>("plan_uninstall", {
          request: { program_id: current.program.id },
        });
      } catch (error) {
        const details = describeError(error);
        if (details.code === "job_conflict") {
          pauseForConflict(current.program.id, details);
          return;
        }
        if (details.code === "admin_required") {
          stopForAdministratorError(current.program.id, details);
          return;
        }
        patchItem(current.program.id, { status: "failed", message: details.message, error: details.message });
        continue;
      }

      patchItem(current.program.id, {
        status: "running",
        job_id: planned.job.snapshot.job_id,
        job: planned.job,
        message: "正在运行原厂卸载器",
      });

      let executed: UninstallJobResponse;
      try {
        executed = await invoke<UninstallJobResponse>("execute_uninstall", {
          request: { job_id: planned.job.snapshot.job_id, timeout_secs: 120 },
        });
      } catch (error) {
        const details = describeError(error);
        if (details.code === "job_conflict") {
          pauseForConflict(current.program.id, details);
          return;
        }
        patchItem(current.program.id, { status: "failed", message: details.message, error: details.message });
        continue;
      }

      let job = executed.job;
      if (job.phase === "failed" || job.phase === "cancelled") {
        finishItemFromJob(current.program.id, job);
        continue;
      }

      if (job.phase === "awaiting_cleanup_confirmation") {
        // 批量模式默认只执行原厂卸载器；残留不自动删除，明确记录后交给用户
        // 逐项审核，避免批量操作放大低置信度误删。
        patchItem(current.program.id, {
          traces_found: job.residue_review.traces.length,
          traces: job.residue_review.traces,
          job,
          message: `已完成主体卸载，保留 ${job.residue_review.traces.length} 项残留待审核`,
        });
        try {
          const finished = await invoke<UninstallJobResponse>("finish_uninstall", {
            request: { job_id: job.snapshot.job_id },
          });
          job = finished.job;
        } catch (error) {
          const details = describeError(error);
          if (details.code === "job_conflict") {
            pauseForConflict(current.program.id, details);
            return;
          }
          patchItem(current.program.id, { status: "failed", message: details.message, error: details.message });
          continue;
        }
      }

      finishItemFromJob(current.program.id, job);
    }

    if (get().cancelRequested) cancelQueuedItems("用户取消了后续队列项");
    set({ active: false, paused: false });
  };

  return {
    items: [],
    active: false,
    paused: false,
    cancelRequested: false,
    error: null,

    startQueue: async (programs) => {
      if (programs.length === 0 || get().active) return;
      set({
        items: programs.map(createItem),
        active: true,
        paused: false,
        cancelRequested: false,
        error: null,
      });
      await runQueue();
    },

    pauseQueue: () => {
      if (get().active) set({ paused: true });
    },

    resumeQueue: () => {
      if (get().active) {
        set({ paused: false, error: null });
        return;
      }
      if (get().items.some((item) => item.status === "queued")) {
        set({ active: true, paused: false, cancelRequested: false, error: null });
        void runQueue();
      }
    },

    cancelQueue: () => {
      if (get().active) {
        set({ cancelRequested: true, paused: false });
      } else {
        cancelQueuedItems("用户取消了后续队列项");
        set({ paused: false, cancelRequested: true });
      }
    },

    reset: () => {
      if (get().active) return;
      set({ items: [], active: false, paused: false, cancelRequested: false, error: null });
    },
  };
});
