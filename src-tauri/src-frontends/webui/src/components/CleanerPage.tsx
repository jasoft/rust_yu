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
          <h1><Sparkles size={20} />系统清理</h1>
          <p>基于 FluentCleaner / Winapp2 规则库，先分析、再确认、最后清理</p>
        </div>
        {catalog && <div className="cleaner-database">数据库 {catalog.database_version}<span>检测到 {catalog.detected_rule_count} / {catalog.total_rule_count} 条规则</span></div>}
      </div>

      {!isTauri() ? (
        <div className="cleaner-runtime-note card-surface"><Info size={18} /><div><strong>请在 Rust Yu 桌面应用中使用系统清理</strong><p>浏览器预览不会调用文件系统或注册表，因此不会显示本机规则。</p></div></div>
      ) : store.error ? (
        <div className="cleaner-error"><AlertTriangle size={16} />{store.error}</div>
      ) : null}

      <div className="cleaner-layout">
        <aside className="cleaner-rules card-surface">
          <div className="cleaner-filters">
            <label className="search-box"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索清理规则…" /></label>
            <select value={category} onChange={(event) => setCategory(event.target.value)}>
              <option value="all">全部分类</option>
              {categories.map((item) => <option key={item} value={item}>{item}</option>)}
            </select>
            <div><span>已选 {store.selectedEntries.size} 条规则</span><button onClick={store.selectRecommended}>选择推荐项</button><button onClick={store.clearEntries}>清空</button></div>
          </div>
          <div className="cleaner-rule-list">
            {store.loadingCatalog ? <CleanerLoading text="正在检测本机规则…" /> : entries.length === 0 ? (
              <div className="cleaner-empty">{isTauri() ? "没有检测到匹配规则" : "桌面运行时可显示本机规则"}</div>
            ) : entries.map((entry) => (
              <button key={entry.id} className={`cleaner-rule ${store.selectedEntries.has(entry.id) ? "selected" : ""}`} onClick={() => store.toggleEntry(entry.id)}>
                <span className="cleaner-check">{store.selectedEntries.has(entry.id) && <Check size={11} />}</span>
                <span><strong>{entry.name}</strong><small>{entry.category} · {entry.file_rule_count} 文件规则 · {entry.registry_rule_count} 注册表规则</small>{entry.warning && <em>注意：{entry.warning}</em>}</span>
              </button>
            ))}
          </div>
          <div className="cleaner-rule-action">
            <button className="primary-button" disabled={!isTauri() || store.selectedEntries.size === 0 || store.scanning} onClick={() => void store.analyze()}>
              {store.scanning ? <Loader2 className="spinning" size={15} /> : <Search size={15} />}{store.scanning ? "正在分析…" : "分析选中规则"}
            </button>
          </div>
        </aside>

        <section className="cleaner-results card-surface">
          {store.scanning ? <CleanerLoading text="正在后台读取文件系统和注册表…" /> : store.scan ? (
            <CleanerResults confirmOpen={confirmOpen} onConfirmOpen={setConfirmOpen} />
          ) : (
            <div className="cleaner-welcome"><span><ShieldCheck size={34} /></span><h2>选择规则并进行安全分析</h2><p>分析阶段只读取文件系统和注册表，不会修改任何内容。发现的目标默认不勾选。</p></div>
          )}
          {store.logs.length > 0 && <div className="cleaner-log"><strong>实时日志</strong>{store.logs.slice(-7).map((line, index) => <p key={`${index}-${line}`}>{line}</p>)}</div>}
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
      <div><strong>发现 {scan.targets.length} 个目标</strong><span>预计可释放 {formatCleanerBytes(scan.total_bytes)}</span></div>
      <button onClick={store.selectAllSafeTargets}>选择全部安全项</button><button onClick={store.clearTargets}>取消全选</button>
    </div>
    {scan.truncated && <div className="cleaner-warning"><AlertTriangle size={14} />结果超过 30,000 项，请缩小规则范围。</div>}
    {store.result && <div className="cleaner-success"><CheckCircle2 size={15} />成功处理 {store.result.items.filter((item) => item.success).length} 项，释放 {formatCleanerBytes(store.result.bytes_freed)}</div>}
    <div className="cleaner-target-list">
      {scan.targets.length === 0 ? <div className="cleaner-empty">没有发现可清理内容</div> : scan.targets.map((target) => <CleanerTargetRow key={target.id} target={target} />)}
    </div>
    <div className="cleaner-result-foot"><span>已选 {store.selectedTargets.size} 项 · {formatCleanerBytes(selectedBytes)}</span><button className="danger-button" disabled={store.selectedTargets.size === 0 || store.cleaning} onClick={() => onConfirmOpen(true)}><Trash2 size={15} />清理所选</button></div>
    {confirmOpen && <div className="modal-backdrop"><div className="safety-modal"><span className="modal-icon"><AlertTriangle size={24} /></span><h2>确认清理 {store.selectedTargets.size} 个目标？</h2><p>预计释放 {formatCleanerBytes(selectedBytes)}。删除文件和注册表项不可自动撤销，后端会在执行前重新验证全部目标。</p><div><button className="secondary-button" onClick={() => onConfirmOpen(false)}>取消</button><button className="danger-button" disabled={store.cleaning} onClick={() => { onConfirmOpen(false); void store.clean(); }}>{store.cleaning ? <Loader2 className="spinning" size={15} /> : <Trash2 size={15} />}确认清理</button></div></div></div>}
  </div>;
}

function CleanerTargetRow({ target }: { target: CleanerTarget }) {
  const selected = useCleanerStore((state) => state.selectedTargets.has(target.id));
  const toggle = useCleanerStore((state) => state.toggleTarget);
  const Icon = target.kind === "file" ? FileCode2 : Database;
  return <button className={`cleaner-target ${selected ? "selected" : ""} ${target.blocked_reason ? "blocked" : ""}`} disabled={Boolean(target.blocked_reason)} onClick={() => toggle(target.id)}>
    <span className="cleaner-check">{selected && <Check size={11} />}</span><Icon size={15} />
    <span><strong>{target.entry_name}</strong><small>{target.value_name ? `${target.path} → ${target.value_name}` : target.path}</small>{target.blocked_reason && <em>已阻止：{target.blocked_reason}</em>}</span>
    <span className="cleaner-target-meta">{target.requires_admin && <b>管理员</b>}{target.size > 0 && formatCleanerBytes(target.size)}</span>
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
