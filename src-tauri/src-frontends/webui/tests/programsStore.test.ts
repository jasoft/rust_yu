import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useProgramsStore } from "../src/stores/programs";
import type { CleanResult, Trace } from "../src/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

const trace: Trace = {
  id: "trace-1",
  program_name: "Demo App",
  trace_type: "file",
  path: "C:\\Program Files\\Demo App\\leftover.log",
  exists: true,
  size: 128,
  confidence: "medium",
  confidence_score: 60,
  confidence_reason: "exact_name_standard_location",
  description: "用户已复核的残留",
};

const result: CleanResult = {
  trace_id: trace.id,
  path: trace.path,
  success: true,
  error: null,
  bytes_freed: trace.size,
};

const protectedTrace: Trace = {
  ...trace,
  id: "trace-protected",
  path: "SYSTEM\\Protected",
};

describe("programs store reviewed cleanup", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    useProgramsStore.setState({
      traces: [trace],
      selectedTraces: new Set([trace.id]),
      cleanResults: [],
      error: null,
    });
  });

  it("explicitly requests user-reviewed authorization for analysis cleanup", async () => {
    mockedInvoke.mockResolvedValue([result]);

    const results = await useProgramsStore.getState().cleanTraces(true);

    expect(mockedInvoke).toHaveBeenCalledWith("clean_traces", {
      options: {
        traces: [trace],
        confirm: true,
        preview: false,
        reviewed: true,
      },
    });
    expect(results).toEqual([result]);
    expect(useProgramsStore.getState().cleanResults).toEqual([result]);
    expect(useProgramsStore.getState().error).toBeNull();
  });

  it("returns an empty result and keeps the backend error visible", async () => {
    mockedInvoke.mockRejectedValue({ message: "admin required" });

    const results = await useProgramsStore.getState().cleanTraces(true);

    expect(results).toEqual([]);
    expect(useProgramsStore.getState().error).toBe("admin required");
  });

  it("lets deep analysis submit its local review selection", async () => {
    useProgramsStore.setState({
      traces: [trace, protectedTrace],
      selectedTraces: new Set(),
    });
    mockedInvoke.mockResolvedValue([result]);

    await useProgramsStore.getState().cleanTraces(true, new Set([trace.id]));

    expect(mockedInvoke).toHaveBeenCalledWith("clean_traces", {
      options: {
        traces: [trace],
        confirm: true,
        preview: false,
        reviewed: true,
      },
    });
  });
});
