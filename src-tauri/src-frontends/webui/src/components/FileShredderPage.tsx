import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle, CheckCircle2, FilePlus2, FileWarning, FolderPlus, Info,
  Loader2, LockKeyhole, ShieldAlert, Trash2, X,
} from "lucide-react";
import { useMemo, useState } from "react";
import { t } from "../i18n/index.ts";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { useFileShredderStore } from "../stores/fileShredder";
import type { ShredMethod, ShredProgress } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function FileShredderPage() {
  const store = useFileShredderStore();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmationText, setConfirmationText] = useState("");
  const blocked = store.plan?.targets.filter((target) => target.blocked_reason) ?? [];
  const methods: Array<{ id: ShredMethod; name: string; passes: string; detail: string }> = [
    { id: "quick", name: t("shredder.method.quick.name"), passes: t("shredder.method.quick.passes"), detail: t("shredder.method.quick.detail") },
    { id: "standard", name: t("shredder.method.standard.name"), passes: t("shredder.method.standard.passes"), detail: t("shredder.method.standard.detail") },
    { id: "thorough", name: t("shredder.method.thorough.name"), passes: t("shredder.method.thorough.passes"), detail: t("shredder.method.thorough.detail") },
  ];
  const progressPercent = useMemo(() => {
    if (!store.progress?.total_bytes) return 0;
    return Math.min(100, Math.round(store.progress.processed_bytes * 100 / store.progress.total_bytes));
  }, [store.progress]);

  const choose = async (directory: boolean) => {
    if (!isTauri()) return;
    const selected = await open({ directory, multiple: true, title: t(directory ? "shredder.picker.folder_title" : "shredder.picker.file_title") });
    if (!selected) return;
    store.addPaths(Array.isArray(selected) ? selected : [selected]);
  };

  return <div className="page shredder-page">
    {isTauri() && <ShredProgressListener />}
    <div className="section-header shredder-header">
      <div><h1><FileWarning size={20} />{t("shredder.title")}</h1><p>{t("shredder.subtitle")}</p></div>
      <span className="shredder-local"><LockKeyhole size={14} />{t("shredder.local_only")}</span>
    </div>

    <div className="shredder-media-warning"><ShieldAlert size={18} /><div><strong>{t("shredder.warning.title")}</strong><span>{t("shredder.warning.detail")}</span></div></div>
    {store.error && <div className="shredder-error"><AlertTriangle size={16} />{store.error}</div>}
    {store.result && <div className={store.result.failures.length ? "shredder-result partial" : "shredder-result"}><CheckCircle2 size={17} /><div><strong>{t("shredder.result.title", { value0: store.result.shredded_files })}</strong><span>{t(store.result.failures.length ? "shredder.result.detail_failures" : "shredder.result.detail", { value0: formatBytes(store.result.bytes_overwritten), value1: store.result.deleted_directories, value2: store.result.failures.length })}</span></div></div>}

    <div className="shredder-layout">
      <section className="shredder-queue card-surface">
        <header><div><strong>{t("shredder.queue.title")}</strong><span>{t("shredder.queue.count", { value0: store.paths.length })}</span></div><button disabled={!store.paths.length || store.shredding} onClick={store.clear}>{t("shredder.queue.clear")}</button></header>
        <div className="shredder-picker">
          <button className="secondary-button" disabled={!isTauri() || store.shredding} onClick={() => void choose(false)}><FilePlus2 size={16} />{t("shredder.picker.add_files")}</button>
          <button className="secondary-button" disabled={!isTauri() || store.shredding} onClick={() => void choose(true)}><FolderPlus size={16} />{t("shredder.picker.add_folders")}</button>
        </div>
        <div className="shredder-paths">
          {!isTauri() ? <ShredderEmpty text={t("shredder.empty.desktop_only")} /> : store.paths.length === 0 ? <ShredderEmpty text={t("shredder.empty.no_targets")} /> : store.paths.map((path) => {
            const target = store.plan?.targets.find((item) => item.path.toLocaleLowerCase() === path.toLocaleLowerCase())
              ?? store.plan?.targets.find((item) => item.path.endsWith(path.split(/[\\/]/).at(-1) ?? ""));
            return <div className={`shredder-path ${target?.blocked_reason ? "blocked" : ""}`} key={path}>
              <FileWarning size={15} /><div><strong>{path.split(/[\\/]/).at(-1) || path}</strong><span>{target ? t("shredder.target.summary", { value0: t(target.kind === "directory" ? "shredder.target.folder" : "shredder.target.file"), value1: target.file_count, value2: formatBytes(target.size) }) : path}</span>{target?.blocked_reason && <em>{t("shredder.target.blocked", { value0: target.blocked_reason })}</em>}</div>
              <button aria-label={t("shredder.target.remove")} disabled={store.shredding} onClick={() => store.removePath(path)}><X size={14} /></button>
            </div>;
          })}
        </div>
      </section>

      <section className="shredder-options card-surface">
        <header><strong>{t("shredder.options.title")}</strong><span>{t("shredder.options.subtitle")}</span></header>
        <div className="shredder-methods">{methods.map((method) => <button key={method.id} className={store.method === method.id ? "selected" : ""} disabled={store.shredding} onClick={() => store.setMethod(method.id)}><span className="shredder-radio" /><div><strong>{method.name}<b>{method.passes}</b></strong><small>{method.detail}</small></div></button>)}</div>

        {store.progress ? <div className="shredder-progress"><div><strong>{store.progress.message}</strong><span>{progressPercent}%</span></div><div className="shredder-progress-track"><span style={{ width: `${progressPercent}%` }} /></div><small title={store.progress.current_path}>{store.progress.current_path || t("shredder.progress.finishing")}</small></div> : store.plan ? <div className="shredder-summary"><div><span>{t("shredder.summary.files")}</span><strong>{store.plan.total_files}</strong></div><div><span>{t("shredder.summary.original_size")}</span><strong>{formatBytes(store.plan.total_bytes)}</strong></div><div><span>{t("shredder.summary.overwrite_size")}</span><strong>{formatBytes(store.plan.overwrite_bytes)}</strong></div></div> : <div className="shredder-guidance"><Info size={16} /><span>{t("shredder.guidance")}</span></div>}

        <footer>
          <span>{blocked.length ? t("shredder.status.blocked", { value0: blocked.length }) : store.plan ? t("shredder.status.locked") : t("shredder.status.ready")}</span>
          {!store.plan ? <button className="primary-button" disabled={!store.paths.length || store.planning || store.shredding || !isTauri()} onClick={() => void store.analyze()}>{store.planning ? <Loader2 className="spinning" size={15} /> : <ShieldAlert size={15} />}{t(store.planning ? "shredder.action.analyzing" : "shredder.action.analyze")}</button> : <button className="danger-button" disabled={blocked.length > 0 || store.shredding} onClick={() => { setConfirmationText(""); setConfirmOpen(true); }}><Trash2 size={15} />{t("shredder.action.shred")}</button>}
        </footer>
      </section>
    </div>

    {confirmOpen && store.plan && <div className="modal-backdrop"><div className="safety-modal shredder-confirm"><span className="modal-icon"><AlertTriangle size={24} /></span><h2>{t("shredder.confirm.title", { value0: store.plan.total_files })}</h2><p>{t("shredder.confirm.detail", { value0: formatBytes(store.plan.overwrite_bytes), value1: store.plan.confirmation_text })}</p><input autoFocus aria-label={t("shredder.confirm.input_label")} value={confirmationText} onChange={(event) => setConfirmationText(event.target.value)} placeholder={store.plan.confirmation_text} /><div><button className="secondary-button" onClick={() => setConfirmOpen(false)}>{t("shredder.confirm.cancel")}</button><button className="danger-button" disabled={confirmationText !== store.plan.confirmation_text} onClick={() => { setConfirmOpen(false); void store.execute(confirmationText); }}><Trash2 size={15} />{t("shredder.confirm.submit")}</button></div></div></div>}
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
  if (!bytes) return t("shredder.bytes.zero");
  const units = [t("shredder.bytes.b"), t("shredder.bytes.kb"), t("shredder.bytes.mb"), t("shredder.bytes.gb"), t("shredder.bytes.tb")];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`;
}
