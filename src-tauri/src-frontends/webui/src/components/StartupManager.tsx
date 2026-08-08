import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  CirclePower,
  Clock3,
  FileInput,
  FolderOpen,
  Loader2,
  RefreshCw,
  RotateCcw,
  Search,
  ServerCog,
  ShieldAlert,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import { cn } from "../lib/utils";
import { useStartupStore } from "../stores/startup";
import type { StartupAction, StartupItem, StartupSource, StartupState } from "../types";

type SourceFilter = "all" | StartupSource;
type StateFilter = "all" | StartupState;

const sourceMeta: Record<StartupSource, { label: string; shortLabel: string }> = {
  registry_run: { label: "注册表 Run", shortLabel: "Run" },
  registry_run_once: { label: "注册表 RunOnce", shortLabel: "RunOnce" },
  registry_policy_run: { label: "策略启动项", shortLabel: "策略" },
  startup_folder: { label: "启动文件夹", shortLabel: "文件夹" },
  scheduled_task: { label: "计划任务", shortLabel: "任务" },
  service: { label: "Windows 服务", shortLabel: "服务" },
};

export function StartupManager() {
  const {
    items,
    loading,
    actionLoading,
    error,
    sourceErrors,
    selectedId,
    pendingPlan,
    lastResult,
    loadItems,
    selectItem,
    planAction,
    cancelPlan,
    applyPendingAction,
    rollbackLastAction,
    clearResult,
  } = useStartupStore();
  const [search, setSearch] = useState("");
  const [source, setSource] = useState<SourceFilter>("all");
  const [state, setState] = useState<StateFilter>("all");
  const [showSystemItems, setShowSystemItems] = useState(false);

  useEffect(() => {
    void loadItems();
  }, [loadItems]);

  const filteredItems = useMemo(() => {
    const normalized = search.trim().toLocaleLowerCase("zh-CN");
    return items.filter((item) => {
      if (!showSystemItems && isProtectedSystemItem(item)) return false;
      if (source !== "all" && item.source !== source) return false;
      if (state !== "all" && item.state !== state) return false;
      if (!normalized) return true;
      return [item.name, item.command, item.locator.location, item.description]
        .filter(Boolean)
        .some((value) => value?.toLocaleLowerCase("zh-CN").includes(normalized));
    });
  }, [items, search, showSystemItems, source, state]);

  const selectedItem = items.find((item) => item.id === selectedId) ?? null;
  const enabledCount = items.filter((item) => item.state === "enabled").length;
  const disabledCount = items.filter((item) => item.state === "disabled").length;
  const brokenCount = items.filter((item) => item.state === "broken").length;
  const protectedCount = items.filter(isProtectedSystemItem).length;
  const failedSources = Object.entries(sourceErrors) as [StartupSource, string][];

  return (
    <section className="flex h-full min-w-0 flex-col bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-5">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="mb-1 flex items-center gap-2 text-xs font-medium text-blue-400">
              <CirclePower className="h-4 w-4" /> 系统启动管理
            </div>
            <h1 className="m-0 text-2xl font-semibold tracking-tight text-white">自启动程序</h1>
            <p className="mt-1 text-sm text-slate-400">
              汇总登录项、启动文件夹、计划任务与自动服务。所有变更先预演，并保留可回滚快照。
            </p>
          </div>
          <button
            type="button"
            onClick={() => void loadItems()}
            disabled={loading || actionLoading}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-sm text-slate-200 transition hover:border-slate-600 hover:bg-slate-700 disabled:opacity-50"
          >
            <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} />
            重新扫描
          </button>
        </div>

        <div className="mt-5 grid grid-cols-4 gap-3">
          <SummaryCard label="全部项目" value={items.length} tone="slate" />
          <SummaryCard label="已启用" value={enabledCount} tone="green" />
          <SummaryCard label="已禁用" value={disabledCount} tone="slate" />
          <SummaryCard label="目标异常" value={brokenCount} tone="amber" />
        </div>
      </header>

      {lastResult?.change_id && (
        <div className="mx-6 mt-4 flex items-center gap-3 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-sm">
          <CheckCircle2 className="h-4 w-4 text-emerald-400" />
          <span className="flex-1 text-emerald-100">变更已完成，并已保存回滚快照。</span>
          <button
            type="button"
            onClick={() => void rollbackLastAction()}
            disabled={actionLoading}
            className="inline-flex items-center gap-1.5 font-medium text-emerald-300 hover:text-emerald-200 disabled:opacity-50"
          >
            <RotateCcw className="h-3.5 w-3.5" /> 撤销
          </button>
          <button type="button" onClick={clearResult} aria-label="关闭提示">
            <X className="h-4 w-4 text-slate-400" />
          </button>
        </div>
      )}

      {(error || failedSources.length > 0) && (
        <div className="mx-6 mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
          <div className="flex items-center gap-2 font-medium">
            <AlertTriangle className="h-4 w-4 text-amber-400" />
            {error ?? `${failedSources.length} 个来源读取失败，其余结果仍可使用`}
          </div>
          {failedSources.length > 0 && (
            <div className="mt-1 pl-6 text-xs text-amber-200/70">
              {failedSources.map(([key]) => sourceMeta[key].label).join("、")}
            </div>
          )}
        </div>
      )}

      <div className="flex min-h-0 flex-1 gap-0 px-6 pb-6 pt-4">
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-l-xl border border-slate-800 bg-slate-900">
          <div className="flex flex-wrap items-center gap-3 border-b border-slate-800 p-3">
            <label className="relative min-w-56 flex-1">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="搜索名称、命令或位置"
                className="h-9 w-full rounded-lg border border-slate-700 bg-slate-950 pl-9 pr-3 text-sm text-slate-100 outline-none transition placeholder:text-slate-600 focus:border-blue-500"
              />
            </label>
            <FilterSelect value={source} onChange={(value) => setSource(value as SourceFilter)}>
              <option value="all">全部来源</option>
              {Object.entries(sourceMeta).map(([value, meta]) => (
                <option key={value} value={value}>{meta.label}</option>
              ))}
            </FilterSelect>
            <FilterSelect value={state} onChange={(value) => setState(value as StateFilter)}>
              <option value="all">全部状态</option>
              <option value="enabled">已启用</option>
              <option value="disabled">已禁用</option>
              <option value="broken">目标异常</option>
            </FilterSelect>
            <label className="ml-auto inline-flex cursor-pointer items-center gap-2 whitespace-nowrap text-xs text-slate-400">
              <input
                type="checkbox"
                checked={showSystemItems}
                onChange={(event) => setShowSystemItems(event.target.checked)}
                className="h-4 w-4 accent-blue-600"
              />
              显示受保护系统项 ({protectedCount})
            </label>
          </div>

          <div className="grid grid-cols-[minmax(210px,1.2fr)_110px_90px_90px_64px] gap-3 border-b border-slate-800 bg-slate-950/50 px-4 py-2 text-xs font-medium text-slate-500">
            <span>名称与命令</span><span>来源</span><span>范围</span><span>状态</span><span />
          </div>

          <div className="min-h-0 flex-1 overflow-auto">
            {loading && items.length === 0 ? (
              <div className="flex h-48 items-center justify-center text-sm text-slate-500">
                <Loader2 className="mr-2 h-5 w-5 animate-spin" /> 正在并行扫描 6 类启动来源…
              </div>
            ) : filteredItems.length === 0 ? (
              <div className="flex h-48 flex-col items-center justify-center text-slate-500">
                <SlidersHorizontal className="mb-2 h-7 w-7" />
                <span className="text-sm">没有符合筛选条件的项目</span>
              </div>
            ) : (
              filteredItems.map((item) => (
                <StartupRow
                  key={item.id}
                  item={item}
                  selected={item.id === selectedId}
                  busy={actionLoading}
                  onSelect={() => selectItem(item.id)}
                  onAction={(action) => void planAction(item, action)}
                />
              ))
            )}
          </div>
          <div className="border-t border-slate-800 px-4 py-2 text-xs text-slate-500">
            显示 {filteredItems.length} / {items.length} 项
          </div>
        </div>

        <StartupDetail
          item={selectedItem}
          busy={actionLoading}
          onClose={() => selectItem(null)}
          onAction={(action) => selectedItem && void planAction(selectedItem, action)}
        />
      </div>

      {pendingPlan && (
        <ActionConfirmation
          item={items.find((item) => item.id === pendingPlan.item_id) ?? null}
          plan={pendingPlan}
          busy={actionLoading}
          onCancel={cancelPlan}
          onConfirm={() => void applyPendingAction()}
        />
      )}
    </section>
  );
}

function isProtectedSystemItem(item: StartupItem): boolean {
  return (
    !item.capabilities.can_enable &&
    !item.capabilities.can_disable &&
    !item.capabilities.can_delete
  );
}

function SummaryCard({ label, value, tone }: { label: string; value: number; tone: "slate" | "green" | "amber" }) {
  return (
    <div className="rounded-lg border border-slate-800 bg-slate-950/60 px-4 py-3">
      <div className="text-xs text-slate-500">{label}</div>
      <div className={cn("mt-1 text-xl font-semibold", tone === "green" && "text-emerald-400", tone === "amber" && "text-amber-400", tone === "slate" && "text-slate-100")}>{value}</div>
    </div>
  );
}

function FilterSelect({ value, onChange, children }: { value: string; onChange: (value: string) => void; children: React.ReactNode }) {
  return (
    <select value={value} onChange={(event) => onChange(event.target.value)} className="h-9 rounded-lg border border-slate-700 bg-slate-950 px-3 text-sm text-slate-300 outline-none focus:border-blue-500">
      {children}
    </select>
  );
}

function StartupRow({ item, selected, busy, onSelect, onAction }: { item: StartupItem; selected: boolean; busy: boolean; onSelect: () => void; onAction: (action: StartupAction) => void }) {
  const currentlyEnabled = item.state !== "disabled";
  const action: StartupAction = currentlyEnabled ? "disable" : "enable";
  const canToggle = currentlyEnabled ? item.capabilities.can_disable : item.capabilities.can_enable;
  return (
    <div className={cn("grid grid-cols-[minmax(210px,1.2fr)_110px_90px_90px_64px] items-center gap-3 border-b border-slate-800/70 px-4 py-3 text-left transition hover:bg-slate-800/45", selected && "bg-blue-500/10")}>
      <button type="button" onClick={onSelect} className="min-w-0 text-left">
        <div className="truncate text-sm font-medium text-slate-100">{item.name}</div>
        <div className="mt-0.5 truncate text-xs text-slate-500">{item.command ?? item.locator.location}</div>
      </button>
      <span className="text-xs text-slate-400">{sourceMeta[item.source].shortLabel}</span>
      <span className="text-xs text-slate-400">{item.scope === "user" ? "当前用户" : "所有用户"}</span>
      <StateBadge state={item.state} />
      <button
        type="button"
        role="switch"
        aria-checked={currentlyEnabled}
        aria-label={`${currentlyEnabled ? "禁用" : "启用"} ${item.name}`}
        disabled={busy || !canToggle}
        onClick={() => onAction(action)}
        className={cn("relative h-6 w-11 rounded-full transition disabled:cursor-not-allowed disabled:opacity-35", currentlyEnabled ? "bg-blue-600" : "bg-slate-700")}
      >
        <span className={cn("absolute top-1 h-4 w-4 rounded-full bg-white shadow transition-all", currentlyEnabled ? "left-6" : "left-1")} />
      </button>
    </div>
  );
}

function StateBadge({ state }: { state: StartupState }) {
  const labels: Record<StartupState, string> = { enabled: "已启用", disabled: "已禁用", broken: "异常" };
  return <span className={cn("w-fit rounded-full px-2 py-0.5 text-xs", state === "enabled" && "bg-emerald-500/10 text-emerald-400", state === "disabled" && "bg-slate-700 text-slate-400", state === "broken" && "bg-amber-500/10 text-amber-400")}>{labels[state]}</span>;
}

function StartupDetail({ item, busy, onClose, onAction }: { item: StartupItem | null; busy: boolean; onClose: () => void; onAction: (action: StartupAction) => void }) {
  if (!item) {
    return <aside className="flex w-80 shrink-0 flex-col items-center justify-center rounded-r-xl border border-l-0 border-slate-800 bg-slate-900 text-center text-slate-500"><FileInput className="mb-3 h-8 w-8" /><p className="text-sm">选择一项查看启动命令与来源</p></aside>;
  }
  return (
    <aside className="w-80 shrink-0 overflow-auto rounded-r-xl border border-l-0 border-slate-800 bg-slate-900 p-5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0"><div className="text-xs text-blue-400">{sourceMeta[item.source].label}</div><h2 className="mt-1 break-words text-lg font-semibold text-white">{item.name}</h2></div>
        <button type="button" onClick={onClose} aria-label="关闭详情"><X className="h-4 w-4 text-slate-500" /></button>
      </div>
      <div className="mt-5 space-y-4 text-sm">
        <DetailField label="状态"><StateBadge state={item.state} /></DetailField>
        <DetailField label="作用范围">{item.scope === "user" ? "仅当前用户" : "所有用户（需要管理员权限）"}</DetailField>
        <DetailField label="启动命令"><code className="block whitespace-pre-wrap break-all bg-transparent p-0 text-xs text-slate-300">{item.command ?? "未提供"}</code></DetailField>
        <DetailField label="来源位置"><span className="break-all text-xs">{item.locator.location}</span></DetailField>
        {item.description && <DetailField label="说明">{item.description}</DetailField>}
        {item.warnings.map((warning) => <div key={warning} className="flex gap-2 rounded-lg bg-amber-500/10 p-3 text-xs text-amber-200"><AlertTriangle className="h-4 w-4 shrink-0" />{warning}</div>)}
      </div>
      <div className="mt-6 space-y-2">
        {item.state === "disabled" ? (
          <ActionButton icon={CirclePower} label="启用此启动项" disabled={busy || !item.capabilities.can_enable} onClick={() => onAction("enable")} />
        ) : (
          <ActionButton icon={CirclePower} label="禁用此启动项" disabled={busy || !item.capabilities.can_disable} onClick={() => onAction("disable")} />
        )}
        {item.capabilities.can_delete && <ActionButton icon={Trash2} label="删除启动项…" danger disabled={busy} onClick={() => onAction("delete")} />}
      </div>
    </aside>
  );
}

function DetailField({ label, children }: { label: string; children: React.ReactNode }) {
  return <div><div className="mb-1 text-xs text-slate-500">{label}</div><div className="text-slate-300">{children}</div></div>;
}

function ActionButton({ icon: Icon, label, disabled, danger = false, onClick }: { icon: typeof CirclePower; label: string; disabled: boolean; danger?: boolean; onClick: () => void }) {
  return <button type="button" onClick={onClick} disabled={disabled} className={cn("flex w-full items-center gap-2 rounded-lg border px-3 py-2 text-sm transition disabled:opacity-40", danger ? "border-red-500/30 text-red-300 hover:bg-red-500/10" : "border-slate-700 text-slate-200 hover:bg-slate-800")}><Icon className="h-4 w-4" />{label}<ChevronRight className="ml-auto h-4 w-4" /></button>;
}

function ActionConfirmation({ item, plan, busy, onCancel, onConfirm }: { item: StartupItem | null; plan: { action: StartupAction; requires_admin: boolean; operations: string[]; warnings: string[]; snapshot_available: boolean }; busy: boolean; onCancel: () => void; onConfirm: () => void }) {
  const destructive = plan.action === "delete";
  const actionLabel = plan.action === "enable" ? "启用" : plan.action === "disable" ? "禁用" : "删除";
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-6 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" aria-labelledby="startup-confirm-title" className="w-full max-w-lg rounded-xl border border-slate-700 bg-slate-900 p-5 shadow-2xl">
        <div className="flex items-start gap-3">
          <div className={cn("rounded-lg p-2", destructive ? "bg-red-500/10 text-red-400" : "bg-blue-500/10 text-blue-400")}>{destructive ? <Trash2 className="h-5 w-5" /> : <CirclePower className="h-5 w-5" />}</div>
          <div><h2 id="startup-confirm-title" className="text-lg font-semibold text-white">确认{actionLabel}“{item?.name ?? "该启动项"}”</h2><p className="mt-1 text-sm text-slate-400">以下是后端生成的模拟执行计划，确认后才会修改系统。</p></div>
        </div>
        <div className="mt-4 rounded-lg border border-slate-800 bg-slate-950 p-4">
          <div className="mb-2 text-xs font-medium text-slate-500">将执行</div>
          <ul className="space-y-2 text-sm text-slate-300">{plan.operations.map((operation) => <li key={operation} className="flex gap-2"><CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-blue-400" />{operation}</li>)}</ul>
        </div>
        <div className="mt-3 flex flex-wrap gap-2 text-xs">
          {plan.snapshot_available && <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2.5 py-1 text-emerald-300"><RotateCcw className="h-3 w-3" />支持回滚</span>}
          {plan.requires_admin && <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2.5 py-1 text-amber-300"><ShieldAlert className="h-3 w-3" />需要管理员权限</span>}
          {item?.source === "scheduled_task" && <span className="inline-flex items-center gap-1 rounded-full bg-slate-800 px-2.5 py-1 text-slate-300"><Clock3 className="h-3 w-3" />计划任务</span>}
          {item?.source === "startup_folder" && <span className="inline-flex items-center gap-1 rounded-full bg-slate-800 px-2.5 py-1 text-slate-300"><FolderOpen className="h-3 w-3" />启动文件</span>}
          {item?.source === "service" && <span className="inline-flex items-center gap-1 rounded-full bg-slate-800 px-2.5 py-1 text-slate-300"><ServerCog className="h-3 w-3" />系统服务</span>}
        </div>
        {destructive && <div className="mt-3 flex gap-2 rounded-lg border border-red-500/25 bg-red-500/10 p-3 text-xs text-red-200"><AlertTriangle className="h-4 w-4 shrink-0" />删除会移除原始启动配置。虽然保存了快照，仍建议优先使用“禁用”。</div>}
        {plan.warnings.map((warning) => <div key={warning} className="mt-2 text-xs text-amber-300">{warning}</div>)}
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onCancel} disabled={busy} className="rounded-lg px-4 py-2 text-sm text-slate-300 hover:bg-slate-800 disabled:opacity-50">取消</button>
          <button type="button" onClick={onConfirm} disabled={busy} className={cn("inline-flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50", destructive ? "bg-red-600 hover:bg-red-500" : "bg-blue-600 hover:bg-blue-500")}>{busy && <Loader2 className="h-4 w-4 animate-spin" />}确认{actionLabel}</button>
        </div>
      </div>
    </div>
  );
}
