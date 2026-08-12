import { getLanguage, t } from "../i18n/index.ts";
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
import { SoftwareHibernation } from "./SoftwareHibernation";

type SourceFilter = "all" | StartupSource;
type StateFilter = "all" | StartupState;
type StartupView = "items" | "software";

const sourceMeta: Record<StartupSource, { label: string; shortLabel: string }> = {
  registry_run: { label: t("components.startupmanager.message_001"), shortLabel: "Run" },
  registry_run_once: { label: t("components.startupmanager.message_002"), shortLabel: "RunOnce" },
  registry_policy_run: { label: t("components.startupmanager.message_003"), shortLabel: t("components.startupmanager.message_004") },
  startup_folder: { label: t("components.startupmanager.message_005"), shortLabel: t("components.startupmanager.message_006") },
  scheduled_task: { label: t("components.startupmanager.message_007"), shortLabel: t("components.startupmanager.message_008") },
  service: { label: t("components.startupmanager.message_009"), shortLabel: t("components.startupmanager.message_010") },
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
  const [view, setView] = useState<StartupView>("items");

  useEffect(() => {
    void loadItems();
  }, [loadItems]);

  const filteredItems = useMemo(() => {
    const normalized = search.trim().toLocaleLowerCase(getLanguage());
    return items.filter((item) => {
      if (!showSystemItems && isProtectedSystemItem(item)) return false;
      if (source !== "all" && item.source !== source) return false;
      if (state !== "all" && item.state !== state) return false;
      if (!normalized) return true;
      return [item.name, item.command, item.locator.location, item.description]
        .filter(Boolean)
        .some((value) => value?.toLocaleLowerCase(getLanguage()).includes(normalized));
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
          <h1><Zap size={20} />{t("app.message_034")}</h1>
          <p>{t("components.startupmanager.message_012")}</p>
        </div>
        {view === "items" && (
          <button
            type="button"
            className="icon-button startup-refresh"
            title={t("app.message_229")}
            onClick={() => void loadItems()}
            disabled={loading || actionLoading}
          >
            <RefreshCw className={loading ? "spinning" : ""} size={17} />
          </button>
        )}
      </div>

      <div className="startup-view-tabs" role="tablist" aria-label={t("components.startupmanager.views.label")}>
        <button type="button" role="tab" aria-selected={view === "items"} className={view === "items" ? "active" : ""} onClick={() => setView("items")}>{t("components.startupmanager.views.items")}</button>
        <button type="button" role="tab" aria-selected={view === "software"} className={view === "software" ? "active" : ""} onClick={() => setView("software")}>{t("components.startupmanager.views.software")}</button>
      </div>

      {view === "items" ? (
        <>
          <div className="startup-summary">
            <SummaryCard label={t("components.startupmanager.message_014")} value={items.length} />
            <SummaryCard label={t("components.startupmanager.message_015")} value={enabledCount} tone="success" />
            <SummaryCard label={t("components.startupmanager.message_016")} value={disabledCount} />
            <SummaryCard label={t("components.startupmanager.message_017")} value={brokenCount} tone="warning" />
          </div>

          {lastResult?.change_id && (
            <div className="startup-notice success">
              <CheckCircle2 size={15} />
              <span>{t("components.startupmanager.message_018")}</span>
              <button type="button" onClick={() => void rollbackLastAction()} disabled={actionLoading}>
                <RotateCcw size={13} />{t("components.startupmanager.message_019")}
              </button>
              <button type="button" onClick={clearResult} aria-label={t("components.startupmanager.message_020")}><X size={14} /></button>
            </div>
          )}

          {(error || failedSources.length > 0) && (
            <div className="startup-notice warning">
              <AlertTriangle size={15} />
              <span>
                {error ?? t("components.startupmanager.message_021", { value0: failedSources.length })}
                {failedSources.length > 0 && <small>{failedSources.map(([key]) => sourceMeta[key].label).join("、")}</small>}
              </span>
            </div>
          )}

          <div className="startup-layout">
            <div className="startup-list card-surface">
              <div className="startup-toolbar">
                <label className="search-box startup-search">
                  <Search size={15} />
                  <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("components.startupmanager.message_022")} />
                </label>
                <FilterSelect value={source} onChange={(value) => setSource(value as SourceFilter)}>
                  <option value="all">{t("components.startupmanager.message_023")}</option>
                  {Object.entries(sourceMeta).map(([value, meta]) => <option key={value} value={value}>{meta.label}</option>)}
                </FilterSelect>
                <FilterSelect value={state} onChange={(value) => setState(value as StateFilter)}>
                  <option value="all">{t("components.startupmanager.message_024")}</option>
                  <option value="enabled">{t("components.startupmanager.message_015")}</option>
                  <option value="disabled">{t("components.startupmanager.message_016")}</option>
                  <option value="broken">{t("components.startupmanager.message_017")}</option>
                </FilterSelect>
                <label className="startup-system-toggle">
                  <input type="checkbox" checked={showSystemItems} onChange={(event) => setShowSystemItems(event.target.checked)} />
                  {t("components.startupmanager.message_028")}{protectedCount})
                </label>
              </div>

              <div className="startup-table-head">
                <span>{t("components.startupmanager.message_029")}</span><span>{t("components.evidencecenter.message_030")}</span><span>{t("components.startupmanager.message_031")}</span><span>{t("components.startupmanager.message_032")}</span><span />
              </div>
              <div className="startup-table-body">
                {loading && items.length === 0 ? (
                  <EmptyList icon={<Loader2 className="spinning" size={19} />} text={t("components.startupmanager.message_033")} />
                ) : filteredItems.length === 0 ? (
                  <EmptyList icon={<SlidersHorizontal size={22} />} text={t("components.startupmanager.message_034")} />
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
              <div className="startup-table-footer">{t("app.message_060")} {filteredItems.length} / {items.length}  {t("app.message_167")}</div>
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
        </>
      ) : (
        <SoftwareHibernation />
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
      <span>{item.scope === "user" ? t("components.startupmanager.message_037") : t("components.startupmanager.message_038")}</span>
      <StateBadge state={item.state} />
      <button
        type="button"
        role="switch"
        aria-checked={currentlyEnabled}
        aria-label={t("components.startupmanager.message_039", { value0: currentlyEnabled ? t("components.startupmanager.message_067") : t("components.startupmanager.message_068"), value1: item.name })}
        disabled={busy || !canToggle}
        onClick={() => onAction(action)}
        className={`startup-switch ${currentlyEnabled ? "on" : ""}`}
      ><span /></button>
    </div>
  );
}

function StateBadge({ state }: { state: StartupState }) {
  const labels: Record<StartupState, string> = { enabled: t("components.startupmanager.message_015"), disabled: t("components.startupmanager.message_016"), broken: t("components.startupmanager.message_042") };
  return <span className={`startup-state ${state}`}>{labels[state]}</span>;
}

function StartupDetail({ item, busy, onClose, onAction }: { item: StartupItem | null; busy: boolean; onClose: () => void; onAction: (action: StartupAction) => void }) {
  if (!item) {
    return <aside className="startup-detail card-surface empty"><FileInput size={30} /><p>{t("components.startupmanager.message_043")}</p></aside>;
  }
  return (
    <aside className="startup-detail card-surface">
      <header>
        <div><span>{sourceMeta[item.source].label}</span><h2>{item.name}</h2></div>
        <button type="button" onClick={onClose} aria-label={t("components.reportcenter.message_025")}><X size={15} /></button>
      </header>
      <div className="startup-detail-fields">
        <DetailField label={t("components.startupmanager.message_032")}><StateBadge state={item.state} /></DetailField>
        <DetailField label={t("components.startupmanager.message_046")}>{item.scope === "user" ? t("components.startupmanager.message_047") : t("components.startupmanager.message_048")}</DetailField>
        <DetailField label={t("components.startupmanager.message_049")}><code>{item.command ?? t("components.startupmanager.message_050")}</code></DetailField>
        <DetailField label={t("components.startupmanager.message_051")}><span className="startup-path">{item.locator.location}</span></DetailField>
        {item.description && <DetailField label={t("components.startupmanager.message_052")}>{item.description}</DetailField>}
        {item.warnings.map((warning) => <div key={warning} className="startup-inline-warning"><AlertTriangle size={14} />{warning}</div>)}
      </div>
      <div className="startup-detail-actions">
        {item.state === "disabled" ? (
          <ActionButton label={t("components.startupmanager.message_053")} disabled={busy || !item.capabilities.can_enable} onClick={() => onAction("enable")} />
        ) : (
          <ActionButton label={t("components.startupmanager.message_054")} disabled={busy || !item.capabilities.can_disable} onClick={() => onAction("disable")} />
        )}
        {item.capabilities.can_delete && <ActionButton label={t("components.startupmanager.message_055")} danger disabled={busy} onClick={() => onAction("delete")} />}
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
  const actionLabel = plan.action === "enable" ? t("components.startupmanager.message_056") : plan.action === "disable" ? t("components.startupmanager.message_057") : t("components.installmonitormanager.message_066");
  return (
    <div className="modal-backdrop startup-modal-backdrop">
      <section role="dialog" aria-modal="true" aria-labelledby="startup-confirm-title" className="startup-modal">
        <header>
          <span className={destructive ? "danger" : ""}>{destructive ? <Trash2 size={20} /> : <CirclePower size={20} />}</span>
          <div><h2 id="startup-confirm-title">{t("components.startupmanager.message_059")}{actionLabel}“{item?.name ?? t("components.startupmanager.message_060")}”</h2><p>{t("components.startupmanager.message_061")}</p></div>
        </header>
        <div className="startup-plan">
          <span>{t("components.startupmanager.message_062")}</span>
          <ul>{plan.operations.map((operation) => <li key={operation}><CheckCircle2 size={14} />{operation}</li>)}</ul>
        </div>
        <div className="startup-plan-tags">
          {plan.snapshot_available && <span className="success"><RotateCcw size={12} />{t("components.startupmanager.message_063")}</span>}
          {plan.requires_admin && <span className="warning"><ShieldAlert size={12} />{t("components.startupmanager.message_064")}</span>}
          <span>{item ? sourceMeta[item.source].label : t("components.startupmanager.message_065")}</span>
        </div>
        {destructive && <div className="startup-delete-warning"><AlertTriangle size={14} />{t("components.startupmanager.message_066")}</div>}
        {plan.warnings.map((warning) => <p key={warning} className="startup-plan-warning">{warning}</p>)}
        <footer>
          <button type="button" className="secondary-button" onClick={onCancel} disabled={busy}>{t("app.message_089")}</button>
          <button type="button" className={destructive ? "danger-button" : "primary-button"} onClick={onConfirm} disabled={busy}>
            {busy && <Loader2 className="spinning" size={14} />}{t("components.startupmanager.message_059")}{actionLabel}
          </button>
        </footer>
      </section>
    </div>
  );
}
