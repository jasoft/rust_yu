import { t } from "../i18n/index.ts";
import type { InstalledProgram } from "../types";

export type ProgramSourceFilter = "all" | "registry" | "msi" | "store";

export const programSourceOptions: ReadonlyArray<{
  id: ProgramSourceFilter;
  label: string;
}> = [
  { id: "all", label: t("components.installmonitormanager.message_074") },
  { id: "registry", label: t("app.message_024") },
  { id: "msi", label: t("common.format.msi") },
  { id: "store", label: t("lib.programfilters.message_003") },
];

export function matchesProgramSource(
  installSource: InstalledProgram["install_source"],
  filter: ProgramSourceFilter,
): boolean {
  return filter === "all" || installSource === filter;
}

export function filterPrograms(
  programs: readonly InstalledProgram[],
  source: ProgramSourceFilter,
  search: string,
): InstalledProgram[] {
  const normalizedSearch = search.trim().toLocaleLowerCase();
  return programs.filter((program) => {
    if (!matchesProgramSource(program.install_source, source)) return false;
    if (!normalizedSearch) return true;
    return `${program.name} ${program.publisher ?? ""}`
      .toLocaleLowerCase()
      .includes(normalizedSearch);
  });
}

export function countProgramsBySource(
  programs: readonly InstalledProgram[],
): Record<ProgramSourceFilter, number> {
  const counts: Record<ProgramSourceFilter, number> = {
    all: programs.length,
    registry: 0,
    msi: 0,
    store: 0,
  };
  for (const program of programs) {
    if (program.install_source !== "unknown") counts[program.install_source] += 1;
  }
  return counts;
}

export function hasMissingProgramIcons(programs: readonly InstalledProgram[]): boolean {
  return programs.some(
    (program) =>
      Boolean(program.icon_path) &&
      (!program.icon_cache_path_32 || !program.icon_cache_path_48),
  );
}
