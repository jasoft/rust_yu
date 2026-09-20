import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  FilePlus2,
  FileWarning,
  FolderPlus,
  Info,
  Loader2,
  LockKeyhole,
  ShieldAlert,
  Trash2,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { useFileShredderStore } from "../stores/fileShredder";
import type { ShredMethod, ShredProgress } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

const methods: Array<{ id: ShredMethod; name: string; passes: string; detail: string }> = [
  { id: "quick", name: "快速", passes: "1 遍", detail: "零填充并回读校验，适合普通非敏感文件" },
  { id: "standard", name: "标准", passes: "3 遍", detail: "零、全 1、随机数据；推荐的平衡方案" },
  { id: "thorough", name: "彻底", passes: "7 遍", detail: "多种固定模式与随机数据，耗时最长" },
];

export function FileShredderPage() {
  const store = useFileShredderStore();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmationText, setConfirmationText] = useState("");
  const blocked = store.plan?.targets.filter((target) => target.blocked_reason) ?? [];
  const progressPercent = useMemo(() => {
    if (!store.progress?.total_bytes) return 0;
    return Math.min(100, Math.round(store.progress.processed_bytes * 100 / store.progress.total_bytes));
  }, [store.progress]);

  const choose = async (directory: boolean) => {
    if (!isTauri()) return;
    const selected = await open({ directory, multiple: true, title: directory ? "选择要永久粉碎的文件夹" : "选择要永久粉碎的文件" });
    if (!selected) return;
    store.addPaths(Array.isArray(selected) ? selected : [selected]);
  };

  return <div className="page shredder-page">
    {isTauri() && <ShredProgressListener />}
    <div className="section-header shredder-header">
      <div><h1><FileWarning size={20} />文件粉碎器</h1><p>覆盖、回读验证并移除文件；执行后无法恢复</p></div>
      <span className="shredder-local"><LockKeyhole size={14} />完全本机处理</span>
    </div>

    <div className="shredder-media-warning"><ShieldAlert size={18} /><div><strong>文件级粉碎不是所有存储介质上的绝对擦除</strong><span>SSD 磨损均衡、TRIM、文件系统日志、快照和云同步副本可能保留数据。极高敏感数据请使用厂商整盘安全擦除。</span></div></div>
    {store.error && <div className="shredder-error"><AlertTriangle size={16} />{store.error}</div>}
    {store.result && <div className={store.result.failures.length ? "shredder-result partial" : "shredder-result"}><CheckCircle2 size={17} /><div><strong>已粉碎 {store.result.shredded_files} 个文件</strong><span>覆盖 {formatBytes(store.result.bytes_overwritten)} · 移除 {store.result.deleted_directories} 个目录{store.result.failures.length ? ` · ${store.result.failures.length} 项失败并保留` : ""}</span></div></div>}

    <div className="shredder-layout">
      <section className="shredder-queue card-surface">
        <header><div><strong>待处理目标</strong><span>{store.paths.length} 个选择</span></div><button disabled={!store.paths.length || store.shredding} onClick={store.clear}>清空</button></header>
        <div className="shredder-picker">
          <button className="secondary-button" disabled={!isTauri() || store.shredding} onClick={() => void choose(false)}><FilePlus2 size={16} />添加文件</button>
          <button className="secondary-button" disabled={!isTauri() || store.shredding} onClick={() => void choose(true)}><FolderPlus size={16} />添加文件夹</button>
        </div>
        <div className="shredder-paths">
          {!isTauri() ? <ShredderEmpty text="请在 Rust Yu 桌面应用中选择本机文件" /> : store.paths.length === 0 ? <ShredderEmpty text="添加文件或文件夹后再进行安全分析" /> : store.paths.map((path) => {
            const target = store.plan?.targets.find((item) => item.path.toLocaleLowerCase() === path.toLocaleLowerCase())
              ?? store.plan?.targets.find((item) => item.path.endsWith(path.split(/[\\/]/).at(-1) ?? ""));
            return <div className={`shredder-path ${target?.blocked_reason ? "blocked" : ""}`} key={path}>
              <FileWarning size={15} /><div><strong>{path.split(/[\\/]/).at(-1) || path}</strong><span>{target ? `${target.kind === "directory" ? "文件夹" : "文件"} · ${target.file_count} 个文件 · ${formatBytes(target.size)}` : path}</span>{target?.blocked_reason && <em>已阻止：{target.blocked_reason}</em>}</div>
              <button aria-label="移除" disabled={store.shredding} onClick={() => store.removePath(path)}><X size={14} /></button>
            </div>;
          })}
        </div>
      </section>

      <section className="shredder-options card-surface">
        <header><strong>擦除方式</strong><span>每一遍覆盖后都会落盘并回读验证</span></header>
        <div className="shredder-methods">{methods.map((method) => <button key={method.id} className={store.method === method.id ? "selected" : ""} disabled={store.shredding} onClick={() => store.setMethod(method.id)}><span className="shredder-radio" /><div><strong>{method.name}<b>{method.passes}</b></strong><small>{method.detail}</small></div></button>)}</div>

        {store.progress ? <div className="shredder-progress"><div><strong>{store.progress.message}</strong><span>{progressPercent}%</span></div><div className="shredder-progress-track"><span style={{ width: `${progressPercent}%` }} /></div><small title={store.progress.current_path}>{store.progress.current_path || "正在收尾…"}</small></div> : store.plan ? <div className="shredder-summary"><div><span>文件</span><strong>{store.plan.total_files}</strong></div><div><span>原始大小</span><strong>{formatBytes(store.plan.total_bytes)}</strong></div><div><span>总覆盖量</span><strong>{formatBytes(store.plan.overwrite_bytes)}</strong></div></div> : <div className="shredder-guidance"><Info size={16} /><span>“分析”只读取文件信息，不会修改文件。确认令牌会绑定文件身份、大小和修改时间。</span></div>}

        <footer>
          <span>{blocked.length ? `${blocked.length} 个目标被安全策略阻止，请先移除` : store.plan ? "计划已锁定；文件变化会触发重新分析" : "先分析，再确认粉碎"}</span>
          {!store.plan ? <button className="primary-button" disabled={!store.paths.length || store.planning || store.shredding || !isTauri()} onClick={() => void store.analyze()}>{store.planning ? <Loader2 className="spinning" size={15} /> : <ShieldAlert size={15} />}{store.planning ? "正在分析…" : "安全分析"}</button> : <button className="danger-button" disabled={blocked.length > 0 || store.shredding} onClick={() => { setConfirmationText(""); setConfirmOpen(true); }}><Trash2 size={15} />永久粉碎</button>}
        </footer>
      </section>
    </div>

    {confirmOpen && store.plan && <div className="modal-backdrop"><div className="safety-modal shredder-confirm"><span className="modal-icon"><AlertTriangle size={24} /></span><h2>永久粉碎 {store.plan.total_files} 个文件？</h2><p>将写入 {formatBytes(store.plan.overwrite_bytes)} 数据。此操作不能撤销，也不会进入回收站。请输入 <strong>{store.plan.confirmation_text}</strong> 继续。</p><input autoFocus value={confirmationText} onChange={(event) => setConfirmationText(event.target.value)} placeholder={store.plan.confirmation_text} /><div><button className="secondary-button" onClick={() => setConfirmOpen(false)}>取消</button><button className="danger-button" disabled={confirmationText !== store.plan.confirmation_text} onClick={() => { setConfirmOpen(false); void store.execute(confirmationText); }}><Trash2 size={15} />确认永久粉碎</button></div></div></div>}
  </div>;
}

function ShredProgressListener() {
  const setProgress = useFileShredderStore((state) => state.setProgress);
  useTauriEvent<ShredProgress>("file-shred-progress", setProgress);
  return null;
}

function ShredderEmpty({ text }: { text: string }) {
  return <div className="shredder-empty"><FilePlus2 size={25} /><span>{text}</span></div>;
}

function formatBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`;
}
