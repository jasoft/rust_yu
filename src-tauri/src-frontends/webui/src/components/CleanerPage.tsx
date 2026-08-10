import { t } from "../i18n/index.ts";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Database,
  FileCode2,
  Info,
  Loader2,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { useCleanerStore } from "../stores/cleaner";
import type { CleanerTarget } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function CleanerPage() {
  const store = useCleanerStore();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const catalog = store.catalog;
  const loadCatalog = store.loadCatalog;

  useEffect(() => {
    if (isTauri() && !catalog) void loadCatalog();
  }, [catalog, loadCatalog]);

  const categories = useMemo(
    () => [...new Set((catalog?.entries ?? []).map((entry) => entry.category))].sort(),
    [catalog],
  );
  const entries = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return (catalog?.entries ?? []).filter((entry) =>
      (category === "all" || entry.category === category)
      && (!normalized || entry.name.toLocaleLowerCase().includes(normalized)),
    );
  }, [catalog, category, query]);

  return (
    <div className="page cleaner-page">
      {isTauri() && <CleanerLogListener />}
      <div className="section-header cleaner-header">
        <div>
          <h1><Sparkles size={20} />{t("app.message_035")}</h1>
          <p>{t("components.cleanerpage.message_002")}</p>
        </div>
        {catalog && <div className="cleaner-database">{t("components.cleanerpage.message_003")} {catalog.database_version}<span>{t("components.cleanerpage.message_004")} {catalog.detected_rule_count} / {catalog.total_rule_count}  {t("components.cleanerpage.message_005")}</span></div>}
      </div>

      {!isTauri() ? (
        <div className="cleaner-runtime-note card-surface"><Info size={18} /><div><strong>{t("components.cleanerpage.message_006")}</strong><p>{t("components.cleanerpage.message_007")}</p></div></div>
      ) : store.error ? (
        <div className="cleaner-error"><AlertTriangle size={16} />{store.error}</div>
      ) : null}

      <div className="cleaner-layout">
        <aside className="cleaner-rules card-surface">
          <div className="cleaner-filters">
            <label className="search-box"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("components.cleanerpage.message_008")} /></label>
            <select value={category} onChange={(event) => setCategory(event.target.value)}>
              <option value="all">{t("components.cleanerpage.message_009")}</option>
              {categories.map((item) => <option key={item} value={item}>{item}</option>)}
            </select>
            <div><span>{t("components.browserpluginspage.message_019")} {store.selectedEntries.size}  {t("components.cleanerpage.message_005")}</span><button onClick={store.selectRecommended}>{t("components.cleanerpage.message_012")}</button><button onClick={store.clearEntries}>{t("app.message_110")}</button></div>
          </div>
          <div className="cleaner-rule-list">
            {store.loadingCatalog ? <CleanerLoading text={t("components.cleanerpage.message_014")} /> : entries.length === 0 ? (
              <div className="cleaner-empty">{isTauri() ? t("components.cleanerpage.message_015") : t("components.cleanerpage.message_016")}</div>
            ) : entries.map((entry) => (
              <button key={entry.id} className={`cleaner-rule ${store.selectedEntries.has(entry.id) ? "selected" : ""}`} onClick={() => store.toggleEntry(entry.id)}>
                <span className="cleaner-check">{store.selectedEntries.has(entry.id) && <Check size={11} />}</span>
                <span><strong>{entry.name}</strong><small>{entry.category} · {entry.file_rule_count}  {t("components.cleanerpage.message_017")} {entry.registry_rule_count}  {t("components.cleanerpage.message_018")}</small>{entry.warning && <em>{t("components.cleanerpage.message_019")}{entry.warning}</em>}</span>
              </button>
            ))}
          </div>
          <div className="cleaner-rule-action">
            <button className="primary-button" disabled={!isTauri() || store.selectedEntries.size === 0 || store.scanning} onClick={() => void store.analyze()}>
              {store.scanning ? <Loader2 className="spinning" size={15} /> : <Search size={15} />}{store.scanning ? t("components.cleanerpage.message_020") : t("components.cleanerpage.message_021")}
            </button>
          </div>
        </aside>

        <section className="cleaner-results card-surface">
          {store.scanning ? <CleanerLoading text={t("components.cleanerpage.message_022")} /> : store.scan ? (
            <CleanerResults confirmOpen={confirmOpen} onConfirmOpen={setConfirmOpen} />
          ) : (
            <div className="cleaner-welcome"><span><ShieldCheck size={34} /></span><h2>{t("components.cleanerpage.message_023")}</h2><p>{t("components.cleanerpage.message_024")}</p></div>
          )}
          {store.logs.length > 0 && <div className="cleaner-log"><strong>{t("app.message_109")}</strong>{store.logs.slice(-7).map((line, index) => <p key={`${index}-${line}`}>{line}</p>)}</div>}
        </section>
      </div>
    </div>
  );
}

function CleanerResults({ confirmOpen, onConfirmOpen }: { confirmOpen: boolean; onConfirmOpen: (open: boolean) => void }) {
  const store = useCleanerStore();
  const scan = store.scan;
  if (!scan) return null;
  const chosen = scan.targets.filter((target) => store.selectedTargets.has(target.id));
  const selectedBytes = chosen.reduce((sum, target) => sum + target.size, 0);
  return <div className="cleaner-result-content">
    <div className="cleaner-result-head">
      <div><strong>{t("components.cleanerpage.message_026")} {scan.targets.length}  {t("components.cleanerpage.message_027")}</strong><span>{t("components.cleanerpage.message_028")} {formatCleanerBytes(scan.total_bytes)}</span></div>
      <button onClick={store.selectAllSafeTargets}>{t("components.cleanerpage.message_029")}</button><button onClick={store.clearTargets}>{t("components.cleanerpage.message_030")}</button>
    </div>
    {scan.truncated && <div className="cleaner-warning"><AlertTriangle size={14} />{t("components.cleanerpage.message_031")}</div>}
    {store.result && <div className="cleaner-success"><CheckCircle2 size={15} />{t("components.browserpluginspage.message_008")} {store.result.items.filter((item) => item.success).length}  {t("components.browserpluginspage.message_009")} {formatCleanerBytes(store.result.bytes_freed)}</div>}
    <div className="cleaner-target-list">
      {scan.targets.length === 0 ? <div className="cleaner-empty">{t("components.cleanerpage.message_034")}</div> : scan.targets.map((target) => <CleanerTargetRow key={target.id} target={target} />)}
    </div>
    <div className="cleaner-result-foot"><span>{t("components.browserpluginspage.message_019")} {store.selectedTargets.size}  {t("components.cleanerpage.message_036")} {formatCleanerBytes(selectedBytes)}</span><button className="danger-button" disabled={store.selectedTargets.size === 0 || store.cleaning} onClick={() => onConfirmOpen(true)}><Trash2 size={15} />{t("app.message_240")}</button></div>
    {confirmOpen && <div className="modal-backdrop"><div className="safety-modal"><span className="modal-icon"><AlertTriangle size={24} /></span><h2>{t("app.message_245")} {store.selectedTargets.size}  {t("components.cleanerpage.message_039")}</h2><p>{t("components.browserpluginspage.message_021")} {formatCleanerBytes(selectedBytes)}{t("components.cleanerpage.message_041")}</p><div><button className="secondary-button" onClick={() => onConfirmOpen(false)}>{t("app.message_089")}</button><button className="danger-button" disabled={store.cleaning} onClick={() => { onConfirmOpen(false); void store.clean(); }}>{store.cleaning ? <Loader2 className="spinning" size={15} /> : <Trash2 size={15} />}{t("app.message_245")}</button></div></div></div>}
  </div>;
}

function CleanerTargetRow({ target }: { target: CleanerTarget }) {
  const selected = useCleanerStore((state) => state.selectedTargets.has(target.id));
  const toggle = useCleanerStore((state) => state.toggleTarget);
  const Icon = target.kind === "file" ? FileCode2 : Database;
  return <button className={`cleaner-target ${selected ? "selected" : ""} ${target.blocked_reason ? "blocked" : ""}`} disabled={Boolean(target.blocked_reason)} onClick={() => toggle(target.id)}>
    <span className="cleaner-check">{selected && <Check size={11} />}</span><Icon size={15} />
    <span><strong>{target.entry_name}</strong><small>{target.value_name ? `${target.path} → ${target.value_name}` : target.path}</small>{target.blocked_reason && <em>{t("components.cleanerpage.message_044")}{target.blocked_reason}</em>}</span>
    <span className="cleaner-target-meta">{target.requires_admin && <b>{t("components.cleanerpage.message_045")}</b>}{target.size > 0 && formatCleanerBytes(target.size)}</span>
  </button>;
}

function CleanerLoading({ text }: { text: string }) {
  return <div className="cleaner-loading"><Loader2 className="spinning" size={20} />{text}</div>;
}

function CleanerLogListener() {
  const appendLog = useCleanerStore((state) => state.appendLog);
  useTauriEvent<string>("fluent-cleaner-log", appendLog);
  return null;
}

function formatCleanerBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`;
}
