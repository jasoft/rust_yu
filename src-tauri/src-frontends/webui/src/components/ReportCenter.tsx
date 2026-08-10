import { useEffect, useMemo, type ReactNode } from "react";
import { AlertTriangle, Archive, CheckCircle2, Download, FileText, Info, Loader2, PackageOpen, RefreshCw, RotateCcw, ShieldCheck, Trash2, X } from "lucide-react";
import { useReportsStore } from "../stores/reports";
import type { BackupSessionInfo, BackupSessionStatus, ReportExportFormat, ReportInfo, UninstallerReport } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function ReportCenter() {
  const store = useReportsStore();
  const load = useReportsStore((state) => state.load);
  useEffect(() => { if (isTauri()) void load(); }, [load]);
  const totalFailures = useMemo(() => store.reports.reduce((sum, report) => sum + report.failed_count, 0), [store.reports]);
  const totalWarnings = useMemo(() => store.reports.reduce((sum, report) => sum + report.warning_count, 0), [store.reports]);
  const deleteSelected = () => {
    if (!store.selected || !window.confirm(`将删除“${store.selected.program_name}”的本地报告副本，是否继续？`)) return;
    void store.deleteReport(store.selected.id);
  };
  const selectedInfo = store.selected ? store.reports.find((report) => report.id === store.selected?.id) : undefined;
  const restoreBackup = (session: BackupSessionInfo) => {
    if (!window.confirm(`将恢复“${store.selected?.program_name ?? session.reason}”本次卸载清理的 ${session.restorable_count} 个项目。\n\n恢复不会覆盖后来重新创建的文件、目录或注册表内容，是否继续？`)) return;
    void store.restoreBackup(session.id);
  };
  return <section className="page report-center">
    <div className="section-header report-center-header"><div><h1><FileText size={20} />卸载记录</h1><p>从不可变任务快照重开历史报告，并导出带来源、结果和 SHA-256 的专业证据包。</p></div><button type="button" className="icon-button" title="刷新卸载记录" onClick={() => void store.load()} disabled={!isTauri() || store.loading || store.actionLoading}><RefreshCw className={store.loading ? "spinning" : ""} size={17} /></button></div>
    {!isTauri() ? <div className="report-runtime-note card-surface"><Info size={17} /><div><strong>请在 Rust Yu 桌面应用中使用卸载记录</strong><p>浏览器预览不会读取本机报告目录，也不会写入导出文件。</p></div></div> : <>
      <div className="report-summary"><ReportStat label="历史报告" value={store.reports.length} /><ReportStat label="失败项目" value={totalFailures} tone={totalFailures > 0 ? "warning" : undefined} /><ReportStat label="读取警告" value={totalWarnings} tone={totalWarnings > 0 ? "warning" : undefined} /><ReportStat label="证据格式" value="ZIP · JSON · CSV · REG" /></div>
      {store.error && <div className="report-notice error"><AlertTriangle size={15} /><span>{store.error}</span><button type="button" onClick={store.clearMessages}>关闭</button></div>}
      {store.notice && !store.error && <div className="report-notice success"><CheckCircle2 size={15} /><span>{store.notice}</span><button type="button" onClick={store.clearMessages}>关闭</button></div>}
      <div className="report-center-layout">
        <section className="report-history card-surface"><div className="report-panel-header"><div><strong>历史任务</strong><span>读取本机 Rust Yu 数据目录中的真实任务快照</span></div><span className="report-count-pill">{store.reports.length}</span></div><div className="report-history-list">{store.loading && store.reports.length === 0 ? <ReportEmpty icon={<Loader2 className="spinning" size={19} />} text="正在读取报告历史…" /> : store.reports.length === 0 ? <ReportEmpty icon={<Archive size={24} />} text="还没有已完成的卸载报告" detail="完成一次卸载后，报告会自动保存在本机。" /> : store.reports.map((report) => <ReportHistoryRow key={report.id} report={report} selected={store.selected?.id === report.id} onSelect={() => void store.open(report.id)} />)}</div></section>
        <ReportDetail report={store.selected} backupSessions={selectedInfo?.backup_sessions ?? []} loading={store.actionLoading} restoringId={store.restoringId} onRestore={restoreBackup} onExport={(format) => store.selected && void store.exportReport(store.selected.id, format)} onBundle={() => store.selected && void store.exportEvidenceBundle(store.selected.id)} onDelete={deleteSelected} onClose={() => useReportsStore.setState({ selected: null })} />
      </div>
    </>}
  </section>;
}

function ReportHistoryRow({ report, selected, onSelect }: { report: ReportInfo; selected: boolean; onSelect: () => void }) {
  const restorableCount = report.backup_sessions.reduce((total, session) => total + session.restorable_count, 0);
  return <button type="button" className={`report-history-row ${selected ? "selected" : ""}`} onClick={onSelect}><span className={`report-history-icon ${report.success ? "success" : "failed"}`}>{report.success ? <CheckCircle2 size={15} /> : <AlertTriangle size={15} />}</span><span className="report-history-copy"><strong>{report.name}</strong><small>{formatDate(report.created_at)} · {report.id.slice(0, 8)}</small></span><span className={`report-history-status ${report.success ? "success" : "failed"}`}>{report.success ? "成功" : "部分失败"}</span><span className={`report-history-count ${restorableCount > 0 ? "restorable" : ""}`}>{restorableCount > 0 ? `可恢复 ${restorableCount}` : report.failed_count > 0 ? `失败 ${report.failed_count}` : `${report.traces_count} 项`}</span></button>;
}

function ReportDetail({ report, backupSessions, loading, restoringId, onRestore, onExport, onBundle, onDelete, onClose }: { report: UninstallerReport | null; backupSessions: BackupSessionInfo[]; loading: boolean; restoringId: string | null; onRestore: (session: BackupSessionInfo) => void; onExport: (format: ReportExportFormat) => void; onBundle: () => void; onDelete: () => void; onClose: () => void }) {
  if (!report) return <section className="report-detail card-surface report-detail-empty"><span><FileText size={28} /></span><strong>选择历史任务</strong><p>报告详情、失败项目、警告和原始任务事件都会从保存的快照中重开。</p></section>;
  const failed = report.traces_removed.filter((result) => !result.success);
  return <section className="report-detail card-surface">
    <header><div><strong>{report.program_name}</strong><small>{formatDate(report.generated_at)} · 报告 {report.id.slice(0, 8)}</small></div><button type="button" className="icon-button" title="关闭详情" onClick={onClose}><X size={15} /></button></header>
    <div className="report-detail-summary"><ReportStat label="发现" value={report.traces_found.length} /><ReportStat label="成功处理" value={report.traces_removed.filter((result) => result.success).length} tone="success" /><ReportStat label="失败" value={failed.length} tone={failed.length > 0 ? "warning" : undefined} /><ReportStat label="释放空间" value={formatBytes(report.total_size_freed)} /></div>
    {report.warnings.length > 0 && <div className="report-detail-warning"><AlertTriangle size={14} /><span>{report.warnings.join("；")}</span></div>}
    <div className="report-detail-body"><div className="report-detail-section"><h3><Archive size={14} />任务快照</h3><div className="report-facts"><span>卸载路线</span><strong>{report.job?.snapshot.route ?? "历史报告"}</strong><span>最终阶段</span><strong>{report.job?.phase ?? "—"}</strong><span>事件数量</span><strong>{report.job?.events.length ?? 0}</strong></div></div><ReportBackups sessions={backupSessions} restoringId={restoringId} onRestore={onRestore} /><div className="report-detail-section"><h3><AlertTriangle size={14} />失败项目 {failed.length}</h3>{failed.length === 0 ? <p className="report-detail-empty-line"><CheckCircle2 size={14} />没有失败的清理项目</p> : <div className="report-failure-list">{failed.map((result) => <div key={result.trace_id}><strong>{result.path}</strong><small>{result.error ?? "未提供失败原因"}</small></div>)}</div>}</div><div className="report-detail-section report-events"><h3><FileText size={14} />阶段事件 {report.job?.events.length ?? 0}</h3><div>{(report.job?.events ?? []).map((event) => <p key={`${event.sequence}-${event.phase}`}>#{event.sequence} · {event.phase} · {event.payload.kind}</p>)}</div></div></div>
    <footer><span>导出仅写入本机，不上传任何内容。</span><button type="button" className="primary-button compact-button" disabled={loading} onClick={onBundle}><PackageOpen size={13} />证据包</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("json")}><Download size={13} />JSON</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("html")}><Download size={13} />HTML</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("text")}><Download size={13} />文本</button><button type="button" className="danger-button compact-button" disabled={loading} onClick={onDelete}><Trash2 size={13} />删除</button></footer>
  </section>;
}

const backupStatusLabel: Record<BackupSessionStatus, string> = {
  prepared: "已备份，未清理",
  partially_cleaned: "已清理，可恢复",
  restored: "已恢复",
  restore_failed: "恢复失败，可重试",
};

function ReportBackups({ sessions, restoringId, onRestore }: { sessions: BackupSessionInfo[]; restoringId: string | null; onRestore: (session: BackupSessionInfo) => void }) {
  return <div className="report-detail-section report-backups"><h3><ShieldCheck size={14} />清理备份 {sessions.length}</h3>{sessions.length === 0 ? <p className="report-detail-empty-line"><Info size={14} />本次记录没有清理可备份的 leftover，或属于升级前的历史报告。</p> : <div className="report-backup-list">{sessions.map((session) => {
    const canRestore = session.restorable_count > 0 && session.status !== "restored";
    const busy = restoringId === session.id;
    return <div key={session.id}><span><strong>{session.restorable_count} 个可恢复项目</strong><small>{formatBytes(session.bytes)} · {backupStatusLabel[session.status]} · 会话 {session.id.slice(0, 8)}</small></span><button type="button" className="secondary-button compact-button" disabled={!canRestore || restoringId !== null} onClick={() => onRestore(session)}>{busy ? <Loader2 className="spinning" size={13} /> : <RotateCcw size={13} />}{busy ? "恢复中…" : session.status === "restore_failed" ? "重试恢复" : session.status === "restored" ? "已恢复" : "恢复 leftover"}</button></div>;
  })}</div>}</div>;
}

function ReportStat({ label, value, tone }: { label: string; value: number | string; tone?: "success" | "warning" }) { return <div className="report-stat"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>; }
function ReportEmpty({ icon, text, detail }: { icon: ReactNode; text: string; detail?: string }) { return <div className="report-empty-state">{icon}<strong>{text}</strong>{detail && <small>{detail}</small>}</div>; }
function formatDate(value: string) { const date = new Date(value); return Number.isNaN(date.valueOf()) ? value : date.toLocaleString("zh-CN"); }
function formatBytes(bytes: number) { if (bytes === 0) return "0 B"; const units = ["B", "KB", "MB", "GB", "TB"]; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1); return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`; }
