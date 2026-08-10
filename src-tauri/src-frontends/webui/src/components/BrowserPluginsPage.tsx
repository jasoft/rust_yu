import { t } from "../i18n/index.ts";
import {
  AlertTriangle,
  AppWindow,
  Check,
  CheckCircle2,
  HardDrive,
  Info,
  Loader2,
  Puzzle,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useBrowserCleanerStore } from "../stores/browserCleaner";
import type { BrowserCleanupItem } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function BrowserPluginsPage() {
  const store = useBrowserCleanerStore();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const scanData = store.scanData;

  useEffect(() => {
    if (isTauri() && !store.scan) void scanData();
  }, [scanData, store.scan]);

  const selectedItems = useMemo(
    () => (store.scan?.items ?? []).filter((item) => store.selectedIds.has(item.id)),
    [store.scan, store.selectedIds],
  );
  const selectedBytes = selectedItems.reduce((sum, item) => sum + item.size, 0);
  const runningSelected = store.scan?.browsers.some(
    (browser) => browser.running && selectedItems.some((item) => item.browser_id === browser.id),
  ) ?? false;

  return (
    <div className="page browser-cleaner-page">
      <div className="section-header browser-cleaner-header">
        <div>
          <h1><AppWindow size={20} />{t("app.message_041")}</h1>
          <p>{t("components.browserpluginspage.message_002")}</p>
        </div>
        <button className="icon-button" title={t("app.message_229")} disabled={store.scanning || store.cleaning} onClick={() => void store.scanData()}>
          <RefreshCw className={store.scanning ? "spinning" : ""} size={17} />
        </button>
      </div>

      {!isTauri() ? (
        <div className="browser-runtime-note card-surface"><Info size={18} /><div><strong>{t("components.browserpluginspage.message_004")}</strong><p>{t("components.browserpluginspage.message_005")}</p></div></div>
      ) : (
        <div className="browser-safety-note"><ShieldCheck size={17} /><span><strong>{t("components.browserpluginspage.message_006")}</strong>{t("components.browserpluginspage.message_007")}</span></div>
      )}

      {store.error && <div className="browser-cleaner-error"><AlertTriangle size={15} />{store.error}</div>}
      {store.result && <div className="browser-cleaner-success"><CheckCircle2 size={15} />{t("components.browserpluginspage.message_008")} {store.result.outcomes.filter((item) => item.success).length}  {t("components.browserpluginspage.message_009")} {formatBytes(store.result.bytes_freed)}</div>}

      <div className="browser-cleaner-body card-surface">
        {store.scanning ? (
          <div className="browser-cleaner-empty"><Loader2 className="spinning" size={20} />{t("components.browserpluginspage.message_010")}</div>
        ) : !store.scan?.browsers.length ? (
          <div className="browser-cleaner-empty"><AppWindow size={30} /><strong>{isTauri() ? t("components.browserpluginspage.message_011") : t("components.browserpluginspage.message_012")}</strong></div>
        ) : (
          <div className="browser-list">
            {store.scan.browsers.map((browser) => {
              const items = store.scan?.items.filter((item) => item.browser_id === browser.id) ?? [];
              const caches = items.filter((item) => item.kind === "cache");
              const extensions = items.filter((item) => item.kind === "extension");
              return <section className="browser-card" key={browser.id}>
                <div className="browser-card-head">
                  <span className="browser-mark"><AppWindow size={17} /></span>
                  <div><strong>{browser.name}</strong><small>{browser.profile_count}  {t("components.browserpluginspage.message_013")} {extensions.length}  {t("components.browserpluginspage.message_014")}</small></div>
                  <b className={browser.running ? "running" : "closed"}>{browser.running ? t("components.browserpluginspage.message_015") : t("components.browserpluginspage.message_016")}</b>
                </div>
                <BrowserItemGroup title={t("components.browserpluginspage.message_017")} icon="cache" items={caches} />
                <BrowserItemGroup title={t("components.browserpluginspage.message_018")} icon="extension" items={extensions} />
              </section>;
            })}
          </div>
        )}
        {!!store.scan?.browsers.length && <div className="browser-cleaner-foot">
          <div><span>{t("components.browserpluginspage.message_019")} {selectedItems.length}  {t("app.message_167")}</span><strong>{t("components.browserpluginspage.message_021")} {formatBytes(selectedBytes)}</strong></div>
          <button onClick={store.selectCaches}>{t("components.browserpluginspage.message_022")}</button><button onClick={store.clearSelection}>{t("app.message_110")}</button>
          <button className="danger-button" disabled={!selectedItems.length || runningSelected || store.cleaning} onClick={() => setConfirmOpen(true)}>
            {store.cleaning ? <Loader2 className="spinning" size={15} /> : <Trash2 size={15} />}{runningSelected ? t("components.browserpluginspage.message_024") : t("app.message_240")}
          </button>
        </div>}
      </div>

      {confirmOpen && <div className="modal-backdrop"><div className="safety-modal">
        <span className="modal-icon"><AlertTriangle size={24} /></span>
        <h2>{t("app.message_245")} {selectedItems.length}  {t("components.browserpluginspage.message_027")}</h2>
        <p>{t("components.browserpluginspage.message_021")} {formatBytes(selectedBytes)}{t("components.browserpluginspage.message_029")}</p>
        <div><button className="secondary-button" onClick={() => setConfirmOpen(false)}>{t("app.message_089")}</button><button className="danger-button" onClick={() => { setConfirmOpen(false); void store.cleanSelected(); }}><Trash2 size={15} />{t("app.message_245")}</button></div>
      </div></div>}
    </div>
  );
}

function BrowserItemGroup({ title, icon, items }: { title: string; icon: "cache" | "extension"; items: BrowserCleanupItem[] }) {
  const selectedIds = useBrowserCleanerStore((state) => state.selectedIds);
  const toggleItem = useBrowserCleanerStore((state) => state.toggleItem);
  const Icon = icon === "cache" ? HardDrive : Puzzle;
  if (!items.length) return null;
  return <div className="browser-item-group">
    <h3><Icon size={13} />{title}<span>{items.length}</span></h3>
    {items.map((item) => <button key={item.id} className={`browser-item ${selectedIds.has(item.id) ? "selected" : ""}`} onClick={() => toggleItem(item.id)}>
      <span className="cleaner-check">{selectedIds.has(item.id) && <Check size={11} />}</span>
      <span><strong>{item.name}</strong><small>{item.profile} · {item.description}</small></span>
      <em>{formatBytes(item.size)}</em>
    </button>)}
  </div>;
}

function formatBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`;
}
