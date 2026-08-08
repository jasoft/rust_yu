import { useEffect, useCallback } from "react";
import { Search, RefreshCw, Package, Loader2 } from "lucide-react";
import { useProgramsStore } from "../stores/programs";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";
import { cn, formatBytes, formatSource } from "../lib/utils";
import { getProgramIconSrc } from "../lib/icon";
import type { InstalledProgram } from "../types";
import { countProgramsBySource, filterPrograms, programSourceOptions } from "../lib/programFilters";

export function ProgramList() {
  const {
    programs,
    loading,
    metadataLoading,
    error,
    searchQuery,
    sourceFilter,
    selectedProgram,
    reloadPrograms,
    setSearchQuery,
    setSourceFilter,
    selectProgram,
  } = useProgramsStore();
  const visiblePrograms = filterPrograms(programs, sourceFilter, searchQuery);
  const sourceCounts = countProgramsBySource(programs);

  useEffect(() => {
    let cancelled = false;
    const initialize = async () => {
      await reloadPrograms();
      if (cancelled) return;
    };

    void initialize();
    return () => {
      cancelled = true;
    };
  }, [reloadPrograms]);

  const handleSearch = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = e.target.value;
      setSearchQuery(value);
    },
    [setSearchQuery],
  );

  const handleRefresh = () => void reloadPrograms({ refresh: true });

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-500" />
          <Input
            placeholder="搜索已安装程序..."
            value={searchQuery}
            onChange={handleSearch}
            className="pl-9"
          />
        </div>
        <button
          onClick={handleRefresh}
          disabled={loading || metadataLoading}
          className="inline-flex items-center gap-1.5 rounded-md px-3 py-2 text-sm text-slate-300 hover:bg-slate-700 transition-colors"
        >
          <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} />
          {metadataLoading ? "图标缓存中..." : "刷新"}
        </button>
      </div>

      <div className="flex gap-2 px-4 py-2 border-b border-slate-700">
        {programSourceOptions.map((opt) => (
          <button
            key={opt.id}
            onClick={() => setSourceFilter(opt.id)}
            className={cn(
              "rounded-md px-3 py-1 text-xs font-medium transition-colors",
              sourceFilter === opt.id
                ? "bg-blue-600 text-white"
                : "text-slate-400 hover:bg-slate-700 hover:text-white",
            )}
          >
            {opt.label} ({sourceCounts[opt.id]})
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-auto">
        {loading && visiblePrograms.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-slate-500">
            <Loader2 className="h-5 w-5 animate-spin mr-2" />
            加载中...
          </div>
        ) : error ? (
          <div className="p-4 text-sm text-red-400">{error}</div>
        ) : visiblePrograms.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-slate-500">
            <Package className="h-8 w-8 mb-2" />
            <span>未找到已安装程序</span>
          </div>
        ) : (
          <div className="divide-y divide-slate-700/50">
            {visiblePrograms.map((program) => (
              <ProgramItem
                key={program.id}
                program={program}
                selected={selectedProgram?.id === program.id}
                onClick={() => selectProgram(program)}
              />
            ))}
          </div>
        )}
      </div>

      <div className="border-t border-slate-700 px-4 py-2 text-xs text-slate-500">
        共 {visiblePrograms.length} 个程序
      </div>
    </div>
  );
}

function ProgramItem({
  program,
  selected,
  onClick,
}: {
  program: InstalledProgram;
  selected: boolean;
  onClick: () => void;
}) {
  const iconSrc = getProgramIconSrc(program);

  return (
    <button
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-slate-700/50",
        selected && "bg-blue-600/10 border-l-2 border-blue-500",
      )}
    >
      {iconSrc ? (
        <img src={iconSrc} alt="" className="h-8 w-8 rounded" />
      ) : (
        <div className="flex h-8 w-8 items-center justify-center rounded bg-slate-700 text-slate-400">
          <Package className="h-4 w-4" />
        </div>
      )}

      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-white truncate">{program.name}</div>
        <div className="text-xs text-slate-400 truncate">
          {program.publisher ?? "未知发布者"}{" "}
          {program.version && `· ${program.version}`}
        </div>
      </div>

      <div className="flex flex-col items-end gap-1">
        <Badge
          variant={
            program.install_source === "store"
              ? "success"
              : program.install_source === "msi"
                ? "warning"
                : "default"
          }
        >
          {formatSource(program.install_source)}
        </Badge>
        {program.size && (
          <span className="text-xs text-slate-500">{formatBytes(program.size)}</span>
        )}
      </div>
    </button>
  );
}
