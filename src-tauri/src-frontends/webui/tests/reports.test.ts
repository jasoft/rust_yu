import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useReportsStore } from "../src/stores/reports";
import type { BackupRestoreResult, ReportInfo } from "../src/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

const sessionId = "11111111-1111-4111-8111-111111111111";

function report(restorableCount: number): ReportInfo {
  return {
    id: "report-1",
    name: "Rust Yu Legacy Test App",
    created_at: "2026-08-10T10:00:00Z",
    path: "C:\\RustYu\\reports\\report-1.json",
    success: true,
    traces_count: 1,
    cleaned_count: 1,
    failed_count: 0,
    warning_count: 0,
    formats: ["json"],
    backup_sessions: [{
      id: sessionId,
      created_at: "2026-08-10T09:59:00Z",
      reason: "Rust Yu Legacy Test App · 卸载残留清理",
      status: restorableCount > 0 ? "partially_cleaned" : "restored",
      item_count: 1,
      restorable_count: restorableCount,
      failed_count: 0,
      bytes: 2048,
    }],
  };
}

describe("卸载记录恢复", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    useReportsStore.setState({
      reports: [report(1)],
      selected: null,
      loading: false,
      actionLoading: false,
      error: null,
      notice: null,
      restoringId: null,
    });
  });

  it("恢复真实备份会话后刷新历史中的可恢复状态", async () => {
    const restored = {
      session_id: sessionId,
      success: true,
      restored_count: 1,
      failed_count: 0,
      session: {},
    } as BackupRestoreResult;
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "restore_backup_session") return restored;
      if (command === "get_reports") return [report(0)];
      throw new Error(`unexpected command: ${command}`);
    });

    const result = await useReportsStore.getState().restoreBackup(sessionId);

    expect(result).toBe(restored);
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, "restore_backup_session", { sessionId });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, "get_reports");
    expect(useReportsStore.getState().reports[0].backup_sessions[0].status).toBe("restored");
    expect(useReportsStore.getState().notice).toContain("已恢复 1 个清理项目");
  });
});
