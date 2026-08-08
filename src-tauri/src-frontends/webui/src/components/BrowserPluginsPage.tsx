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
          <h1><AppWindow size={20} />浏览器插件</h1>
          <p>管理 Chrome、Edge、Brave 的扩展，并清理可安全重建的缓存数据</p>
        </div>
        <button className="icon-button" title="重新扫描" disabled={store.scanning || store.cleaning} onClick={() => void store.scanData()}>
          <RefreshCw className={store.scanning ? "spinning" : ""} size={17} />
        </button>
      </div>

      {!isTauri() ? (
        <div className="browser-runtime-note card-surface"><Info size={18} /><div><strong>请在 Rust Yu 桌面应用中使用浏览器清理</strong><p>网页预览不会读取或修改本机浏览器数据。</p></div></div>
      ) : (
        <div className="browser-safety-note"><ShieldCheck size={17} /><span><strong>安全模式：</strong>缓存默认选中，扩展必须手动选择；执行前会重新验证目标。</span></div>
      )}

      {store.error && <div className="browser-cleaner-error"><AlertTriangle size={15} />{store.error}</div>}
      {store.result && <div className="browser-cleaner-success"><CheckCircle2 size={15} />成功处理 {store.result.outcomes.filter((item) => item.success).length} 项，释放 {formatBytes(store.result.bytes_freed)}</div>}

      <div className="browser-cleaner-body card-surface">
        {store.scanning ? (
          <div className="browser-cleaner-empty"><Loader2 className="spinning" size={20} />正在后台扫描浏览器配置…</div>
        ) : !store.scan?.browsers.length ? (
          <div className="browser-cleaner-empty"><AppWindow size={30} /><strong>{isTauri() ? "未发现受支持的浏览器配置" : "等待桌面运行时"}</strong></div>
        ) : (
          <div className="browser-list">
            {store.scan.browsers.map((browser) => {
              const items = store.scan?.items.filter((item) => item.browser_id === browser.id) ?? [];
              const caches = items.filter((item) => item.kind === "cache");
              const extensions = items.filter((item) => item.kind === "extension");
              return <section className="browser-card" key={browser.id}>
                <div className="browser-card-head">
                  <span className="browser-mark"><AppWindow size={17} /></span>
                  <div><strong>{browser.name}</strong><small>{browser.profile_count} 个配置文件 · {extensions.length} 个扩展</small></div>
                  <b className={browser.running ? "running" : "closed"}>{browser.running ? "正在运行" : "已退出"}</b>
                </div>
                <BrowserItemGroup title="缓存与临时文件" icon="cache" items={caches} />
                <BrowserItemGroup title="已安装扩展" icon="extension" items={extensions} />
              </section>;
            })}
          </div>
        )}
        {!!store.scan?.browsers.length && <div className="browser-cleaner-foot">
          <div><span>已选 {selectedItems.length} 项</span><strong>预计释放 {formatBytes(selectedBytes)}</strong></div>
          <button onClick={store.selectCaches}>选择缓存</button><button onClick={store.clearSelection}>清空</button>
          <button className="danger-button" disabled={!selectedItems.length || runningSelected || store.cleaning} onClick={() => setConfirmOpen(true)}>
            {store.cleaning ? <Loader2 className="spinning" size={15} /> : <Trash2 size={15} />}{runningSelected ? "请先退出浏览器" : "清理所选"}
          </button>
        </div>}
      </div>

      {confirmOpen && <div className="modal-backdrop"><div className="safety-modal">
        <span className="modal-icon"><AlertTriangle size={24} /></span>
        <h2>确认清理 {selectedItems.length} 项？</h2>
        <p>预计释放 {formatBytes(selectedBytes)}。扩展和本地数据删除后不可自动撤销，启用同步的扩展可能被重新安装。</p>
        <div><button className="secondary-button" onClick={() => setConfirmOpen(false)}>取消</button><button className="danger-button" onClick={() => { setConfirmOpen(false); void store.cleanSelected(); }}><Trash2 size={15} />确认清理</button></div>
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
