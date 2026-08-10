import { getLanguage, t } from "../i18n/index.ts";
import { useEffect, useMemo } from "react";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  ExternalLink,
  Info,
  Loader2,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { useHealthStore } from "../stores/health";
import type { HealthSeverity, ProgramHealth, StartupImpact } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function HealthCenter() {
  const report = useHealthStore((state) => state.report);
  const loading = useHealthStore((state) => state.loading);
  const error = useHealthStore((state) => state.error);
  const load = useHealthStore((state) => state.load);

  useEffect(() => {
    if (isTauri()) void load();
  }, [load]);

  const averageScore = useMemo(() => {
    if (!report || report.programs.length === 0) return 0;
    return Math.round(report.programs.reduce((sum, item) => sum + item.score, 0) / report.programs.length);
  }, [report]);
  const startupCount = report?.programs.reduce((sum, item) => sum + item.startup_entry_count, 0) ?? 0;
  const updateHints = report?.programs.filter((item) => item.update_hint) ?? [];

  if (!isTauri()) {
    return <section className="page health-center"><HealthHeader loading={false} onRefresh={() => undefined} /><div className="health-runtime-note card-surface"><Info size={17} /><div><strong>{t("components.healthcenter.message_001")}</strong><p>{t("components.healthcenter.message_002")}</p></div></div></section>;
  }

  return <section className="page health-center"><HealthHeader loading={loading} onRefresh={() => void load()} /><div className="health-disclaimer card-surface"><ShieldCheck size={17} /><span>{t("components.healthcenter.message_003")}</span></div>{error && <div className="health-notice error"><AlertTriangle size={15} /><span>{error}</span></div>}{report ? <><div className="health-summary"><HealthStat label={t("components.healthcenter.message_004")} value={`${averageScore}/100`} tone={averageScore >= 70 ? "success" : "warning"} /><HealthStat label={t("components.healthcenter.message_005")} value={report.review_count} tone={report.review_count > 0 ? "warning" : "success"} /><HealthStat label={t("components.healthcenter.message_006")} value={startupCount} /><HealthStat label={t("components.healthcenter.message_007")} value={updateHints.length} /></div>{report.warnings.length > 0 && <div className="health-warning-list">{report.warnings.map((warning) => <p key={warning}><AlertTriangle size={14} />{warning}</p>)}</div>}<div className="health-layout"><section className="health-list card-surface"><header><div><strong>{t("components.healthcenter.message_008")}</strong><span>{report.total_programs}  {t("components.healthcenter.message_009")} {formatDate(report.evaluated_at)}</span></div><span className="health-count-pill">{report.review_count}  {t("components.healthcenter.message_010")}</span></header><div className="health-list-body">{report.programs.length === 0 ? <HealthEmpty text={t("components.healthcenter.message_011")} /> : report.programs.map((item) => <HealthRow key={item.program_id} item={item} />)}</div></section><section className="health-guide card-surface"><h2><Activity size={17} />{t("components.healthcenter.message_012")}</h2><div><strong>{t("components.healthcenter.message_013")}</strong><p>{t("components.healthcenter.message_014")}</p></div><div><strong>{t("components.healthcenter.message_015")}</strong><p>{t("components.healthcenter.message_016")}</p></div><div><strong>{t("components.healthcenter.message_017")}</strong><p>{t("components.healthcenter.message_018")}</p></div></section></div></> : loading ? <div className="health-loading card-surface"><Loader2 className="spinning" size={22} /><span>{t("components.healthcenter.message_019")}</span></div> : null}</section>;
}

function HealthHeader({ loading, onRefresh }: { loading: boolean; onRefresh: () => void }) {
  return <div className="section-header health-header"><div><h1><Activity size={20} />{t("app.message_033")}</h1><p>{t("components.healthcenter.message_021")}</p></div><button type="button" className="icon-button" title={t("components.healthcenter.message_022")} onClick={onRefresh} disabled={!isTauri() || loading}><RefreshCw className={loading ? "spinning" : ""} size={17} /></button></div>;
}

function HealthRow({ item }: { item: ProgramHealth }) {
  const topFinding = item.findings[0];
  return <article className="health-row"><div className={`health-score ${item.status}`}><strong>{item.score}</strong><small>/100</small></div><div className="health-row-main"><div className="health-row-title"><div><strong>{item.program_name}</strong><span>{item.publisher ?? t("app.message_016")}{item.version ? ` · ${item.version}` : ""}</span></div><span className={`health-status ${item.status}`}>{item.status === "healthy" ? t("components.healthcenter.message_024") : t("components.healthcenter.message_005")}</span></div><div className="health-row-facts"><span>{t("components.healthcenter.message_026")} {item.startup_entry_count}  {t("components.cleanerpage.message_036")} {formatStartupImpact(item.startup_impact)}</span><span>{item.last_used ? t("components.healthcenter.message_028", { value0: formatDate(item.last_used) }) : t("components.healthcenter.message_029")}</span>{item.times_used !== null && <span>{t("components.evidencecenter.message_042")} {item.times_used}</span>}</div>{topFinding && <p className={`health-finding ${topFinding.severity}`}><FindingIcon severity={topFinding.severity} />{topFinding.title}：{topFinding.detail}</p>}{item.findings.length > 1 && <small className="health-more-findings">{t("components.healthcenter.message_031")} {item.findings.length - 1}  {t("components.healthcenter.message_032")}</small>}</div>{item.update_hint && <a className="health-update-link" href={item.update_hint.url} target="_blank" rel="noreferrer" title={item.update_hint.message}><ExternalLink size={14} />{t("components.healthcenter.message_033")}</a>}</article>;
}

function HealthStat({ label, value, tone }: { label: string; value: number | string; tone?: "success" | "warning" }) {
  return <div className="health-stat"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>;
}

function HealthEmpty({ text }: { text: string }) {
  return <div className="health-empty"><CheckCircle2 size={22} /><strong>{text}</strong></div>;
}

function FindingIcon({ severity }: { severity: HealthSeverity }) {
  return severity === "critical" ? <AlertTriangle size={14} /> : severity === "warning" ? <AlertTriangle size={14} /> : <Info size={14} />;
}

function formatStartupImpact(value: StartupImpact): string {
  return { none: t("components.healthcenter.message_034"), low: t("components.healthcenter.message_035"), medium: t("components.healthcenter.message_036"), high: t("components.healthcenter.message_037"), unknown: t("app.message_296") }[value];
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString(getLanguage());
}
