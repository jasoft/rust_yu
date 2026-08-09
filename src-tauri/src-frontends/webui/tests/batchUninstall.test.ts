import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useBatchUninstallStore } from "../src/stores/batchUninstall";
import type {
  InstalledProgram,
  Trace,
  UninstallJob,
  UninstallJobResponse,
} from "../src/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

function program(id: string): InstalledProgram {
  return {
    id,
    name: `Demo ${id}`,
    publisher: "Rust Yu Test",
    version: "1.0.0",
    install_date: null,
    install_location: `C:\\Program Files\\Demo ${id}`,
    uninstall_string: `C:\\Program Files\\Demo ${id}\\uninstall.exe`,
    quiet_uninstall_string: null,
    uninstall_registry_key_path: `HKLM\\Software\\Demo ${id}`,
    install_source: "registry",
    uninstall_kind: "legacy",
    size: 1024,
    icon_path: null,
    icon_cache_path_32: null,
    icon_cache_path_48: null,
    size_last_updated_at: null,
    icon_data_url: null,
    icon_data_url_32: null,
    icon_data_url_48: null,
    estimated_size: 1024,
    display_version: "1.0.0",
    url_info_about: null,
    help_link: null,
    install_date_source: "test",
    install_date_confidence: "high",
    icon_source: "none",
    icon_confidence: "low",
    size_source: "test",
    size_confidence: "high",
    metadata_confidence: "high",
  };
}

function makeTrace(programName: string, id: string): Trace {
  return {
    id,
    program_name: programName,
    trace_type: "file",
    path: `C:\\Program Files\\${programName}\\leftover.log`,
    exists: true,
    size: 2048,
    confidence: "high",
    description: "测试残留",
  };
}

function makeJob(
  installed: InstalledProgram,
  jobId: string,
  phase: UninstallJob["phase"],
  traces: Trace[] = [],
): UninstallJobResponse {
  const completed = phase === "completed";
  return {
    job: {
      snapshot: {
        job_id: jobId,
        program: installed,
        fingerprint: "test-fingerprint",
        route: "legacy",
        traces,
        selected_trace_ids: [],
      },
      phase,
      next_sequence: 1,
      events: [],
      residue_review: { traces, default_selected_ids: [] },
      outcome: completed
        ? {
            success: true,
            message: "卸载完成",
            exit_code: 0,
            reboot_required: false,
            traces_found: traces.length,
            traces_cleaned: 0,
            bytes_freed: 0,
          }
        : null,
    },
  };
}

describe("batch uninstall queue", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    useBatchUninstallStore.getState().reset();
  });

  it("runs each program through an independent job in serial order and skips residues", async () => {
    const first = program("first");
    const second = program("second");
    const firstTrace = makeTrace(first.name, "trace-first");
    const jobs = new Map<string, number>();

    mockedInvoke.mockImplementation(async (command, args) => {
      const payload = args as { request?: { program_id?: string; job_id?: string } } | undefined;
      if (command === "plan_uninstall") {
        const id = payload?.request?.program_id ?? "";
        const jobId = `job-${id}`;
        jobs.set(jobId, (jobs.get(jobId) ?? 0) + 1);
        return makeJob(id === first.id ? first : second, jobId, "planned");
      }
      if (command === "execute_uninstall") {
        const jobId = payload?.request?.job_id ?? "";
        return jobId === "job-first"
          ? makeJob(first, jobId, "awaiting_cleanup_confirmation", [firstTrace])
          : makeJob(second, jobId, "completed");
      }
      if (command === "finish_uninstall") {
        return makeJob(first, payload?.request?.job_id ?? "job-first", "completed", [firstTrace]);
      }
      throw new Error(`unexpected command: ${command}`);
    });

    await useBatchUninstallStore.getState().startQueue([first, second]);

    expect(mockedInvoke.mock.calls.map(([command]) => command)).toEqual([
      "plan_uninstall",
      "execute_uninstall",
      "finish_uninstall",
      "plan_uninstall",
      "execute_uninstall",
    ]);
    expect(jobs.size).toBe(2);
    expect(useBatchUninstallStore.getState().items.map((item) => item.status)).toEqual(["completed", "completed"]);
    expect(useBatchUninstallStore.getState().items[0].traces_found).toBe(1);
    expect(useBatchUninstallStore.getState().active).toBe(false);
  });

  it("isolates a failed item and continues with the next item", async () => {
    const first = program("broken");
    const second = program("healthy");
    mockedInvoke.mockImplementation(async (command, args) => {
      const payload = args as { request?: { program_id?: string; job_id?: string } } | undefined;
      if (command === "plan_uninstall") {
        if (payload?.request?.program_id === first.id) {
          throw { code: "uninstaller_failed", message: "测试卸载器失败" };
        }
        return makeJob(second, "job-healthy", "planned");
      }
      if (command === "execute_uninstall") return makeJob(second, "job-healthy", "completed");
      throw new Error(`unexpected command: ${command}`);
    });

    await useBatchUninstallStore.getState().startQueue([first, second]);

    const items = useBatchUninstallStore.getState().items;
    expect(items[0].status).toBe("failed");
    expect(items[0].error).toBe("测试卸载器失败");
    expect(items[1].status).toBe("completed");
  });

  it("pauses on a coordinator conflict and can resume the queued item", async () => {
    const target = program("conflict");
    let conflicted = true;
    mockedInvoke.mockImplementation(async (command, args) => {
      const payload = args as { request?: { job_id?: string } } | undefined;
      if (command === "plan_uninstall" && conflicted) {
        conflicted = false;
        throw { code: "job_conflict", message: "已有卸载任务正在执行" };
      }
      if (command === "plan_uninstall") return makeJob(target, "job-conflict", "planned");
      if (command === "execute_uninstall") return makeJob(target, payload?.request?.job_id ?? "job-conflict", "completed");
      throw new Error(`unexpected command: ${command}`);
    });

    await useBatchUninstallStore.getState().startQueue([target]);
    expect(useBatchUninstallStore.getState().paused).toBe(true);
    expect(useBatchUninstallStore.getState().items[0].status).toBe("queued");

    useBatchUninstallStore.getState().resumeQueue();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(useBatchUninstallStore.getState().items[0].status).toBe("completed");
    expect(useBatchUninstallStore.getState().active).toBe(false);
  });
});
