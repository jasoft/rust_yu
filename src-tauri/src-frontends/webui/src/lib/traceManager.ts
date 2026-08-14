import type { CleanResult, Trace } from "../types";

export interface TraceGroups {
  files: Trace[];
  registry: Trace[];
  system: Trace[];
}

export function groupTraces(traces: readonly Trace[]): TraceGroups {
  return traces.reduce<TraceGroups>(
    (groups, trace) => {
      if (trace.trace_type === "registry_key" || trace.trace_type === "registry_value") {
        groups.registry.push(trace);
      } else if (
        trace.trace_type === "scheduled_task"
        || trace.trace_type === "service"
        || trace.trace_type === "driver"
      ) {
        groups.system.push(trace);
      } else {
        groups.files.push(trace);
      }
      return groups;
    },
    { files: [], registry: [], system: [] },
  );
}

export function selectedTraceBytes(
  traces: readonly Trace[],
  selectedIds: ReadonlySet<string>,
): number {
  return traces
    .filter((trace) => selectedIds.has(trace.id))
    .reduce((total, trace) => total + (trace.size ?? 0), 0);
}

export function summarizeCleanResults(results: readonly CleanResult[]): {
  succeeded: number;
  failed: number;
  bytesFreed: number;
} {
  return {
    succeeded: results.filter((result) => result.success).length,
    failed: results.filter((result) => !result.success).length,
    bytesFreed: results.reduce((total, result) => total + result.bytes_freed, 0),
  };
}
