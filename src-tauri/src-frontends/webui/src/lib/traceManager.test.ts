import { describe, expect, it } from "vitest";
import type { CleanResult, Trace } from "../types";
import { groupTraces, selectedTraceBytes, summarizeCleanResults } from "./traceManager";

const traces: Trace[] = [
  { id: "file", program_name: "Fixture", trace_type: "file", path: "C:\\Fixture", exists: true, size: 20, confidence: "high", description: null },
  { id: "registry", program_name: "Fixture", trace_type: "registry_key", path: "HKCU\\Software\\Fixture", exists: true, size: null, confidence: "medium", description: null },
  { id: "service", program_name: "Fixture", trace_type: "service", path: "FixtureService", exists: true, size: 5, confidence: "low", description: null, is_critical: true },
];

describe("trace manager helpers", () => {
  it("groups file, registry, and system integration traces", () => {
    const groups = groupTraces(traces);
    expect(groups.files.map((trace) => trace.id)).toEqual(["file"]);
    expect(groups.registry.map((trace) => trace.id)).toEqual(["registry"]);
    expect(groups.system.map((trace) => trace.id)).toEqual(["service"]);
  });

  it("counts only selected trace bytes", () => {
    expect(selectedTraceBytes(traces, new Set(["file", "registry"]))).toBe(20);
  });

  it("keeps successful and failed cleanup evidence separate", () => {
    const results: CleanResult[] = [
      { trace_id: "file", path: "C:\\Fixture", success: true, error: null, bytes_freed: 20 },
      { trace_id: "service", path: "FixtureService", success: false, error: "protected", bytes_freed: 0 },
    ];
    expect(summarizeCleanResults(results)).toEqual({ succeeded: 1, failed: 1, bytesFreed: 20 });
  });
});
