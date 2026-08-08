import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  CirclePower,
  FileInput,
  Loader2,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldAlert,
  SlidersHorizontal,
  Trash2,
  X,
  Zap,
} from "lucide-react";
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
    <section className="page startup-page">
      <div className="section-header startup-header">
        <div>
          <h1><Zap size={20} />自启动管理</h1>
          <p>管理登录项、启动文件夹、计划任务与自动服务。所有变更均先预演并保留回滚快照。</p>
        </div>
        <button
          type="button"
          className="icon-button startup-refresh"
          title="重新扫描"
          onClick={() => void loadItems()}
          disabled={loading || actionLoading}
        >
          <RefreshCw className={loading ? "spinning" : ""} size={17} />
        </button>
      </div>

      <div className="startup-summary">
        <SummaryCard label="全部项目" value={items.length} />
        <SummaryCard label="已启用" value={enabledCount} tone="success" />
        <SummaryCard label="已禁用" value={disabledCount} />
        <SummaryCard label="目标异常" value={brokenCount} tone="warning" />
      </div>

      {lastResult?.change_id && (
        <div className="startup-notice success">
          <CheckCircle2 size={15} />
          <span>变更已完成，并已保存回滚快照。</span>
          <button type="button" onClick={() => void rollbackLastAction()} disabled={actionLoading}>
            <RotateCcw size={13} />撤销
          </button>
          <button type="button" onClick={clearResult} aria-label="关闭提示"><X size={14} /></button>
        </div>
      )}

      {(error || failedSources.length > 0) && (
        <div className="startup-notice warning">
          <AlertTriangle size={15} />
          <span>
            {error ?? `${failedSources.length} 个来源读取失败，其余结果仍可使用`}
            {failedSources.length > 0 && <small>{failedSources.map(([key]) => sourceMeta[key].label).join("、")}</small>}
          </span>
        </div>
      )}

      <div className="startup-layout">
        <div className="startup-list card-surface">
          <div className="startup-toolbar">
            <label className="search-box startup-search">
              <Search size={15} />
              <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索名称、命令或位置" />
            </label>
            <FilterSelect value={source} onChange={(value) => setSource(value as SourceFilter)}>
              <option value="all">全部来源</option>
              {Object.entries(sourceMeta).map(([value, meta]) => <option key={value} value={value}>{meta.label}</option>)}
            </FilterSelect>
            <FilterSelect value={state} onChange={(value) => setState(value as StateFilter)}>
              <option value="all">全部状态</option>
              <option value="enabled">已启用</option>
              <option value="disabled">已禁用</option>
              <option value="broken">目标异常</option>
            </FilterSelect>
            <label className="startup-system-toggle">
              <input type="checkbox" checked={showSystemItems} onChange={(event) => setShowSystemItems(event.target.checked)} />
              显示系统项 ({protectedCount})
            </label>
          </div>

          <div className="startup-table-head">
            <span>名称与命令</span><span>来源</span><span>范围</span><span>状态</span><span />
          </div>
          <div className="startup-table-body">
            {loading && items.length === 0 ? (
              <EmptyList icon={<Loader2 className="spinning" size={19} />} text="正在并行扫描 6 类启动来源…" />
            ) : filteredItems.length === 0 ? (
              <EmptyList icon={<SlidersHorizontal size={22} />} text="没有符合筛选条件的项目" />
            ) : filteredItems.map((item) => (
              <StartupRow
                key={item.id}
                item={item}
                selected={item.id === selectedId}
                busy={actionLoading}
                onSelect={() => selectItem(item.id)}
                onAction={(action) => void planAction(item, action)}
              />
            ))}
          </div>
          <div className="startup-table-footer">显示 {filteredItems.length} / {items.length} 项</div>
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

function isProtectedSystemItem(item: StartupItem) {
  return !item.capabilities.can_enable && !item.capabilities.can_disable && !item.capabilities.can_delete;
}

function SummaryCard({ label, value, tone = "default" }: { label: string; value: number; tone?: "default" | "success" | "warning" }) {
  return <div className="card-surface startup-stat"><span>{label}</span><strong className={tone}>{value}</strong></div>;
}

function FilterSelect({ value, onChange, children }: { value: string; onChange: (value: string) => void; children: ReactNode }) {
  return <select value={value} onChange={(event) => onChange(event.target.value)}>{children}</select>;
}

function EmptyList({ icon, text }: { icon: ReactNode; text: string }) {
  return <div className="startup-empty">{icon}<span>{text}</span></div>;
}

function StartupRow({ item, selected, busy, onSelect, onAction }: { item: StartupItem; selected: boolean; busy: boolean; onSelect: () => void; onAction: (action: StartupAction) => void }) {
  const currentlyEnabled = item.state !== "disabled";
  const action: StartupAction = currentlyEnabled ? "disable" : "enable";
  const canToggle = currentlyEnabled ? item.capabilities.can_disable : item.capabilities.can_enable;
  return (
    <div className={`startup-row ${selected ? "selected" : ""}`}>
      <button type="button" className="startup-row-main" onClick={onSelect}>
        <strong>{item.name}</strong><small>{item.command ?? item.locator.location}</small>
      </button>
      <span>{sourceMeta[item.source].shortLabel}</span>
      <span>{item.scope === "user" ? "当前用户" : "所有用户"}</span>
      <StateBadge state={item.state} />
      <button
        type="button"
        role="switch"
        aria-checked={currentlyEnabled}
        aria-label={`${currentlyEnabled ? "禁用" : "启用"} ${item.name}`}
        disabled={busy || !canToggle}
        onClick={() => onAction(action)}
        className={`startup-switch ${currentlyEnabled ? "on" : ""}`}
      ><span /></button>
    </div>
  );
}

function StateBadge({ state }: { state: StartupState }) {
  const labels: Record<StartupState, string> = { enabled: "已启用", disabled: "已禁用", broken: "异常" };
  return <span className={`startup-state ${state}`}>{labels[state]}</span>;
}

function StartupDetail({ item, busy, onClose, onAction }: { item: StartupItem | null; busy: boolean; onClose: () => void; onAction: (action: StartupAction) => void }) {
  if (!item) {
    return <aside className="startup-detail card-surface empty"><FileInput size={30} /><p>选择一项查看启动命令与来源</p></aside>;
  }
  return (
    <aside className="startup-detail card-surface">
      <header>
        <div><span>{sourceMeta[item.source].label}</span><h2>{item.name}</h2></div>
        <button type="button" onClick={onClose} aria-label="关闭详情"><X size={15} /></button>
      </header>
      <div className="startup-detail-fields">
        <DetailField label="状态"><StateBadge state={item.state} /></DetailField>
        <DetailField label="作用范围">{item.scope === "user" ? "仅当前用户" : "所有用户（需要管理员权限）"}</DetailField>
        <DetailField label="启动命令"><code>{item.command ?? "未提供"}</code></DetailField>
        <DetailField label="来源位置"><span className="startup-path">{item.locator.location}</span></DetailField>
        {item.description && <DetailField label="说明">{item.description}</DetailField>}
        {item.warnings.map((warning) => <div key={warning} className="startup-inline-warning"><AlertTriangle size={14} />{warning}</div>)}
      </div>
      <div className="startup-detail-actions">
        {item.state === "disabled" ? (
          <ActionButton label="启用此启动项" disabled={busy || !item.capabilities.can_enable} onClick={() => onAction("enable")} />
        ) : (
          <ActionButton label="禁用此启动项" disabled={busy || !item.capabilities.can_disable} onClick={() => onAction("disable")} />
        )}
        {item.capabilities.can_delete && <ActionButton label="删除启动项…" danger disabled={busy} onClick={() => onAction("delete")} />}
      </div>
    </aside>
  );
}

function DetailField({ label, children }: { label: string; children: ReactNode }) {
  return <div className="startup-field"><span>{label}</span><div>{children}</div></div>;
}

function ActionButton({ label, disabled, danger = false, onClick }: { label: string; disabled: boolean; danger?: boolean; onClick: () => void }) {
  return <button type="button" className={danger ? "startup-action danger" : "startup-action"} onClick={onClick} disabled={disabled}><CirclePower size={14} />{label}<ChevronRight size={14} /></button>;
}

function ActionConfirmation({ item, plan, busy, onCancel, onConfirm }: { item: StartupItem | null; plan: { action: StartupAction; requires_admin: boolean; operations: string[]; warnings: string[]; snapshot_available: boolean }; busy: boolean; onCancel: () => void; onConfirm: () => void }) {
  const destructive = plan.action === "delete";
  const actionLabel = plan.action === "enable" ? "启用" : plan.action === "disable" ? "禁用" : "删除";
  return (
    <div className="modal-backdrop startup-modal-backdrop">
      <section role="dialog" aria-modal="true" aria-labelledby="startup-confirm-title" className="startup-modal">
        <header>
          <span className={destructive ? "danger" : ""}>{destructive ? <Trash2 size={20} /> : <CirclePower size={20} />}</span>
          <div><h2 id="startup-confirm-title">确认{actionLabel}“{item?.name ?? "该启动项"}”</h2><p>以下是后端生成的模拟执行计划，确认后才会修改系统。</p></div>
        </header>
        <div className="startup-plan">
          <span>将执行</span>
          <ul>{plan.operations.map((operation) => <li key={operation}><CheckCircle2 size={14} />{operation}</li>)}</ul>
        </div>
        <div className="startup-plan-tags">
          {plan.snapshot_available && <span className="success"><RotateCcw size={12} />支持回滚</span>}
          {plan.requires_admin && <span className="warning"><ShieldAlert size={12} />需要管理员权限</span>}
          <span>{item ? sourceMeta[item.source].label : "自启动项"}</span>
        </div>
        {destructive && <div className="startup-delete-warning"><AlertTriangle size={14} />删除会移除原始启动配置，建议优先使用“禁用”。</div>}
        {plan.warnings.map((warning) => <p key={warning} className="startup-plan-warning">{warning}</p>)}
        <footer>
          <button type="button" className="secondary-button" onClick={onCancel} disabled={busy}>取消</button>
          <button type="button" className={destructive ? "danger-button" : "primary-button"} onClick={onConfirm} disabled={busy}>
            {busy && <Loader2 className="spinning" size={14} />}确认{actionLabel}
          </button>
        </footer>
      </section>
    </div>
  );
}
