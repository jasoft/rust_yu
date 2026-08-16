import { describe, expect, it } from "vitest";
import type { UninstallJobEvent } from "../../types";
import {
  hydrateUninstallJob,
  initialUninstallUiState,
  isModalLocked,
  reduceUninstallEvent,
} from "./uninstallState";

const event = (sequence: number, phase: UninstallJobEvent["phase"], jobId = "job-1"): UninstallJobEvent => ({
  job_id: jobId,
  sequence,
  phase,
  payload: { kind: "residues_scanned", count: 2 },
});

describe("uninstall state reducer", () => {
  it("ignores other jobs and out-of-order events", () => {
    const active = reduceUninstallEvent(initialUninstallUiState, event(2, "running_uninstaller"));
    const hydrated = reduceUninstallEvent(active, event(1, "planned"));
    expect(hydrated.lastSequence).toBe(2);
    expect(hydrated.logs).toHaveLength(1);

    const otherJob = reduceUninstallEvent(hydrated, event(3, "completed", "other-job"));
    expect(otherJob.lastSequence).toBe(2);
  });

  it("locks modal during destructive phases", () => {
    expect(isModalLocked("running_uninstaller")).toBe(true);
    expect(isModalLocked("verifying_removal")).toBe(true);
    expect(isModalLocked("scanning_residues")).toBe(true);
    expect(isModalLocked("cleaning_residues")).toBe(true);
    expect(isModalLocked("awaiting_cleanup_confirmation")).toBe(false);
  });

  it("hydrates from backend snapshot without selecting residues", () => {
    const job = {
      snapshot: { job_id: "job-1", program: {} as never, fingerprint: "f", route: "legacy", traces: [], selected_trace_ids: [] },
      phase: "awaiting_cleanup_confirmation" as const,
      next_sequence: 3,
      events: [event(1, "planned"), event(2, "awaiting_cleanup_confirmation")],
      residue_review: { traces: [], default_selected_ids: [] },
      cleanup_results: [],
      outcome: null,
    };
    const state = hydrateUninstallJob(initialUninstallUiState, job);
    expect(state.lastSequence).toBe(2);
    expect(state.job?.residue_review.default_selected_ids).toEqual([]);
  });
});
