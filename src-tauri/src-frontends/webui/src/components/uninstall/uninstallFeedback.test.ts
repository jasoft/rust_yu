import { describe, expect, it } from "vitest";
import { getUninstallFailureMessage } from "./uninstallFeedback";

const failedResult = {
  success: false,
  message: "卸载器返回失败状态",
  exit_code: 1603,
  reboot_required: false,
  traces_found: 0,
  traces_cleaned: 0,
  bytes_freed: 0,
};

describe("getUninstallFailureMessage", () => {
  it("keeps an invoke error visible even without leaving the progress page", () => {
    expect(getUninstallFailureMessage("管理员权限不足", null)).toBe("管理员权限不足");
  });

  it("falls back to a failed backend result", () => {
    expect(getUninstallFailureMessage(null, failedResult)).toBe("卸载器返回失败状态");
  });

  it("does not report successful results as failures", () => {
    expect(getUninstallFailureMessage(null, { ...failedResult, success: true })).toBeNull();
  });
});
