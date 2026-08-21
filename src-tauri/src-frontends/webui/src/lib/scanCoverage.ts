import { t } from "../i18n/index.ts";
import type { ScanArea, ScanAreaStatus, ScanCoverage } from "../types";

export function scanAreaLabel(area: ScanArea): string {
  return t(`traces.coverage.area.${area}`);
}

export function scanStatusLabel(status: ScanAreaStatus): string {
  return t(`traces.coverage.status.${status}`);
}

export function activeCoverageCount(coverage: readonly ScanCoverage[]): number {
  return coverage.filter((entry) => entry.status !== "skipped").length;
}
