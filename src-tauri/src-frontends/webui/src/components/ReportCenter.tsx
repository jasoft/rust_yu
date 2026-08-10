import { getLanguage, t } from "../i18n/index.ts";
import { useEffect, useMemo, type ReactNode } from "react";
import { AlertTriangle, Archive, CheckCircle2, Download, FileText, Info, Loader2, PackageOpen, RefreshCw, Trash2, X } from "lucide-react";
import { useReportsStore } from "../stores/reports";
import type { ReportExportFormat, ReportInfo, UninstallerReport } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function ReportCenter() {
  const store = useReportsStore();
  const load = useReportsStore((state) => state.load);
  useEffect(() => { if (isTauri()) void load(); }, [load]);
  const totalFailures = useMemo(() => store.reports.reduce((sum, report) => sum + report.failed_count, 0), [store.reports]);
  const totalWarnings = useMemo(() => store.reports.reduce((sum, report) => sum + report.warning_count, 0), [store.reports]);
  const deleteSelected = () => {
    if (!store.selected || !window.confirm(t("components.reportcenter.message_001", { value0: store.selected.program_name }))) return;
    void store.deleteReport(store.selected.id);
  };
  return <section className="page report-center">
    <div className="section-header report-center-header"><div><h1><FileText size={20} />{t("app.message_040")}</h1><p>{t("components.reportcenter.message_003")}</p></div><button type="button" className="icon-button" title={t("components.reportcenter.message_004")} onClick={() => void store.load()} disabled={!isTauri() || store.loading || store.actionLoading}><RefreshCw className={store.loading ? "spinning" : ""} size={17} /></button></div>
    {!isTauri() ? <div className="report-runtime-note card-surface"><Info size={17} /><div><strong>{t("components.reportcenter.message_005")}</strong><p>{t("components.reportcenter.message_006")}</p></div></div> : <>
      <div className="report-summary"><ReportStat label={t("components.reportcenter.message_007")} value={store.reports.length} /><ReportStat label={t("components.reportcenter.message_008")} value={totalFailures} tone={totalFailures > 0 ? "warning" : undefined} /><ReportStat label={t("components.reportcenter.message_009")} value={totalWarnings} tone={totalWarnings > 0 ? "warning" : undefined} /><ReportStat label={t("components.reportcenter.message_010")} value="ZIP · JSON · CSV · REG" /></div>
      {store.error && <div className="report-notice error"><AlertTriangle size={15} /><span>{store.error}</span><button type="button" onClick={store.clearMessages}>{t("app.message_031")}</button></div>}
      {store.notice && !store.error && <div className="report-notice success"><CheckCircle2 size={15} /><span>{store.notice}</span><button type="button" onClick={store.clearMessages}>{t("app.message_031")}</button></div>}
      <div className="report-center-layout">
        <section className="report-history card-surface"><div className="report-panel-header"><div><strong>{t("components.reportcenter.message_013")}</strong><span>{t("components.reportcenter.message_014")}</span></div><span className="report-count-pill">{store.reports.length}</span></div><div className="report-history-list">{store.loading && store.reports.length === 0 ? <ReportEmpty icon={<Loader2 className="spinning" size={19} />} text={t("components.reportcenter.message_015")} /> : store.reports.length === 0 ? <ReportEmpty icon={<Archive size={24} />} text={t("components.reportcenter.message_016")} detail={t("components.reportcenter.message_017")} /> : store.reports.map((report) => <ReportHistoryRow key={report.id} report={report} selected={store.selected?.id === report.id} onSelect={() => void store.open(report.id)} />)}</div></section>
        <ReportDetail report={store.selected} loading={store.actionLoading} onExport={(format) => store.selected && void store.exportReport(store.selected.id, format)} onBundle={() => store.selected && void store.exportEvidenceBundle(store.selected.id)} onDelete={deleteSelected} onClose={() => useReportsStore.setState({ selected: null })} />
      </div>
    </>}
  </section>;
}

function ReportHistoryRow({ report, selected, onSelect }: { report: ReportInfo; selected: boolean; onSelect: () => void }) {
  return <button type="button" className={`report-history-row ${selected ? "selected" : ""}`} onClick={onSelect}><span className={`report-history-icon ${report.success ? "success" : "failed"}`}>{report.success ? <CheckCircle2 size={15} /> : <AlertTriangle size={15} />}</span><span className="report-history-copy"><strong>{report.name}</strong><small>{formatDate(report.created_at)} · {report.id.slice(0, 8)}</small></span><span className={`report-history-status ${report.success ? "success" : "failed"}`}>{report.success ? t("components.reportcenter.message_018") : t("components.reportcenter.message_019")}</span><span className="report-history-count">{report.failed_count > 0 ? t("components.reportcenter.message_020", { value0: report.failed_count }) : t("app.message_135", { value0: report.traces_count })}</span></button>;
}

function ReportDetail({ report, loading, onExport, onBundle, onDelete, onClose }: { report: UninstallerReport | null; loading: boolean; onExport: (format: ReportExportFormat) => void; onBundle: () => void; onDelete: () => void; onClose: () => void }) {
  if (!report) return <section className="report-detail card-surface report-detail-empty"><span><FileText size={28} /></span><strong>{t("components.reportcenter.message_022")}</strong><p>{t("components.reportcenter.message_023")}</p></section>;
  const failed = report.traces_removed.filter((result) => !result.success);
  return <section className="report-detail card-surface">
    <header><div><strong>{report.program_name}</strong><small>{formatDate(report.generated_at)}  {t("components.reportcenter.message_024")} {report.id.slice(0, 8)}</small></div><button type="button" className="icon-button" title={t("components.reportcenter.message_025")} onClick={onClose}><X size={15} /></button></header>
    <div className="report-detail-summary"><ReportStat label={t("components.cleanerpage.message_026")} value={report.traces_found.length} /><ReportStat label={t("components.browserpluginspage.message_008")} value={report.traces_removed.filter((result) => result.success).length} tone="success" /><ReportStat label={t("app.message_108")} value={failed.length} tone={failed.length > 0 ? "warning" : undefined} /><ReportStat label={t("app.message_211")} value={formatBytes(report.total_size_freed)} /></div>
    {report.warnings.length > 0 && <div className="report-detail-warning"><AlertTriangle size={14} /><span>{report.warnings.join("；")}</span></div>}
    <div className="report-detail-body"><div className="report-detail-section"><h3><Archive size={14} />{t("components.reportcenter.message_030")}</h3><div className="report-facts"><span>{t("components.reportcenter.message_031")}</span><strong>{report.job?.snapshot.route ?? t("components.reportcenter.message_007")}</strong><span>{t("components.reportcenter.message_033")}</span><strong>{report.job?.phase ?? "—"}</strong><span>{t("components.reportcenter.message_034")}</span><strong>{report.job?.events.length ?? 0}</strong></div></div><div className="report-detail-section"><h3><AlertTriangle size={14} />{t("components.reportcenter.message_008")} {failed.length}</h3>{failed.length === 0 ? <p className="report-detail-empty-line"><CheckCircle2 size={14} />{t("components.reportcenter.message_036")}</p> : <div className="report-failure-list">{failed.map((result) => <div key={result.trace_id}><strong>{result.path}</strong><small>{result.error ?? t("components.reportcenter.message_037")}</small></div>)}</div>}</div><div className="report-detail-section report-events"><h3><FileText size={14} />{t("components.reportcenter.message_038")} {report.job?.events.length ?? 0}</h3><div>{(report.job?.events ?? []).map((event) => <p key={`${event.sequence}-${event.phase}`}>#{event.sequence} · {event.phase} · {event.payload.kind}</p>)}</div></div></div>
    <footer><span>{t("components.reportcenter.message_039")}</span><button type="button" className="primary-button compact-button" disabled={loading} onClick={onBundle}><PackageOpen size={13} />{t("components.reportcenter.message_040")}</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("json")}><Download size={13} />{t("common.format.json")}</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("html")}><Download size={13} />{t("common.format.html")}</button><button type="button" className="secondary-button compact-button" disabled={loading} onClick={() => onExport("text")}><Download size={13} />{t("components.reportcenter.message_041")}</button><button type="button" className="danger-button compact-button" disabled={loading} onClick={onDelete}><Trash2 size={13} />{t("components.installmonitormanager.message_066")}</button></footer>
  </section>;
}

function ReportStat({ label, value, tone }: { label: string; value: number | string; tone?: "success" | "warning" }) { return <div className="report-stat"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>; }
function ReportEmpty({ icon, text, detail }: { icon: ReactNode; text: string; detail?: string }) { return <div className="report-empty-state">{icon}<strong>{text}</strong>{detail && <small>{detail}</small>}</div>; }
function formatDate(value: string) { const date = new Date(value); return Number.isNaN(date.valueOf()) ? value : date.toLocaleString(getLanguage()); }
function formatBytes(bytes: number) { if (bytes === 0) return "0 B"; const units = ["B", "KB", "MB", "GB", "TB"]; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1); return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`; }
