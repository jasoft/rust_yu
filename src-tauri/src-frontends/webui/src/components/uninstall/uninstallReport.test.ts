import { describe, expect, it } from "vitest";
import type { Trace, UninstallJobEvent } from "../../types";
import {
  formatTraceType,
  formatUninstallEventLog,
  summarizeTraces,
  traceCategory,
} from "./uninstallReport";

const trace = (traceType: Trace["trace_type"], size: number): Trace => ({
  id: `${traceType}-${size}`,
  program_name: "Demo",
  trace_type: traceType,
  path: `C:\\Demo\\${traceType}`,
  exists: true,
  size,
  confidence: "high",
  description: "残留项目",
});

describe("uninstall report helpers", () => {
  it("groups scan results into user-facing categories and totals bytes", () => {
    const summaries = summarizeTraces([
      trace("file", 1024),
      trace("shortcut", 256),
      trace("appdata", 2048),
      trace("registry_key", 512),
    ]);

    expect(summaries.find((item) => item.category === "files")).toMatchObject({ count: 2, bytes: 1280 });
    expect(summaries.find((item) => item.category === "user_data")).toMatchObject({ count: 1, bytes: 2048 });
    expect(summaries.find((item) => item.category === "registry")).toMatchObject({ count: 1, bytes: 512 });
    expect(traceCategory({ ...trace("file", 1), trace_type: "AppData" as Trace["trace_type"] })).toBe("user_data");
  });

  it("formats detailed event logs instead of only exposing a phase name", () => {
    const event: UninstallJobEvent = {
      job_id: "job-1",
      sequence: 4,
      phase: "scanning_residues",
      payload: { kind: "removal_verified", removed: true },
    };

    expect(formatUninstallEventLog(event, new Date("2026-08-10T10:24:50"))).toContain("程序移除验证通过");
    expect(traceCategory(trace("appdata", 1))).toBe("user_data");
    expect(formatTraceType("registry_key")).toBe("注册表键");
    expect(formatTraceType("RegistryKey")).toBe("注册表键");
  });
});
