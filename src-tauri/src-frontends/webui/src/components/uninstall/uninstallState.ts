import type {
  CommandError,
  UninstallJob,
  UninstallJobEvent,
  UninstallPhase,
} from "../../types";
import { formatUninstallEventLog } from "./uninstallReport";

export interface UninstallUiState {
  job: UninstallJob | null;
  jobId: string | null;
  lastSequence: number;
  logs: string[];
  error: CommandError | null;
}

export const initialUninstallUiState: UninstallUiState = {
  job: null,
  jobId: null,
  lastSequence: 0,
  logs: [],
  error: null,
};

export const runningPhases: UninstallPhase[] = [
  "running_uninstaller",
  "verifying_removal",
  "scanning_residues",
  "cleaning_residues",
];

export function isModalLocked(phase: UninstallPhase | undefined): boolean {
  return phase !== undefined && runningPhases.includes(phase);
}

export function reduceUninstallEvent(
  state: UninstallUiState,
  event: UninstallJobEvent,
): UninstallUiState {
  if (state.jobId && state.jobId !== event.job_id) return state;
  if (event.sequence <= state.lastSequence) return state;

  const job = state.job
    ? { ...state.job, phase: event.phase, events: [...state.job.events, event] }
    : state.job;
  return {
    ...state,
    jobId: state.jobId ?? event.job_id,
    job,
    lastSequence: event.sequence,
    logs: [...state.logs, formatEventLog(event)],
  };
}

export function hydrateUninstallJob(
  state: UninstallUiState,
  job: UninstallJob,
): UninstallUiState {
  const lastSequence = job.events.reduce(
    (max, event) => Math.max(max, event.sequence),
    0,
  );
  return {
    ...state,
    job,
    jobId: job.snapshot.job_id,
    lastSequence,
    logs: job.events.map(formatEventLog),
    error: null,
  };
}

function formatEventLog(event: UninstallJobEvent): string {
  return formatUninstallEventLog(event);
}
