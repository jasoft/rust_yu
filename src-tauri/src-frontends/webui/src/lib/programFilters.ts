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

/**
 * 判断程序能否走标准卸载流程。
 *
 * 两类失效安装必须提前拦截，避免用户确认后才在“运行卸载器”阶段失败：
 * - is_broken_installation：安装目录、卸载程序、显示图标全部不存在，
 *   即使 UninstallString 还在，指向的卸载器也无法运行；
 * - Legacy 路由没有任何可用卸载命令（注册表只剩条目）。
 * MSI/Store 路由由后端按包标识构造 msiexec/powershell 命令，始终可用。
 */
export function hasUsableUninstallCommand(
  program: Pick<
    InstalledProgram,
    "uninstall_kind" | "uninstall_string" | "quiet_uninstall_string" | "is_broken_installation"
  >,
): boolean {
  if (program.is_broken_installation) return false;
  if (program.uninstall_kind !== "legacy") return true;
  return [program.quiet_uninstall_string, program.uninstall_string]
    .some((command) => Boolean(command?.trim()));
}
