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
    return <section className="page health-center"><HealthHeader loading={false} onRefresh={() => undefined} /><div className="health-runtime-note card-surface"><Info size={17} /><div><strong>请在 Rust Yu 桌面应用中运行软件健康检查</strong><p>健康结果来自本机程序清单、自启动项和缓存，不会在浏览器预览中伪造。</p></div></div></section>;
  }

  return <section className="page health-center"><HealthHeader loading={loading} onRefresh={() => void load()} /><div className="health-disclaimer card-surface"><ShieldCheck size={17} /><span>这是本地证据提示，不是安全认证，也不会联网判断“最新版本”或自动升级软件。</span></div>{error && <div className="health-notice error"><AlertTriangle size={15} /><span>{error}</span></div>}{report ? <><div className="health-summary"><HealthStat label="平均健康分" value={`${averageScore}/100`} tone={averageScore >= 70 ? "success" : "warning"} /><HealthStat label="需要复核" value={report.review_count} tone={report.review_count > 0 ? "warning" : "success"} /><HealthStat label="启用自启动" value={startupCount} /><HealthStat label="手动更新入口" value={updateHints.length} /></div>{report.warnings.length > 0 && <div className="health-warning-list">{report.warnings.map((warning) => <p key={warning}><AlertTriangle size={14} />{warning}</p>)}</div>}<div className="health-layout"><section className="health-list card-surface"><header><div><strong>软件健康概览</strong><span>{report.total_programs} 个本机清单项 · {formatDate(report.evaluated_at)}</span></div><span className="health-count-pill">{report.review_count} 项待复核</span></header><div className="health-list-body">{report.programs.length === 0 ? <HealthEmpty text="没有可分析的程序清单" /> : report.programs.map((item) => <HealthRow key={item.program_id} item={item} />)}</div></section><section className="health-guide card-surface"><h2><Activity size={17} />怎么解读</h2><div><strong>分数低不等于程序有毒</strong><p>缺少卸载命令、安装位置不可读或同名条目只表示需要人工核对；Rust Yu 不把营销信息、未知日期或缺少大小当作删除依据。</p></div><div><strong>更新提示是手动入口</strong><p>只有程序注册表声明了 HTTP(S) 厂商页面时才显示入口；点击后由系统浏览器打开，Rust Yu 不下载、不静默安装。</p></div><div><strong>自启动影响可单独管理</strong><p>健康页只展示匹配到的自启动数量，禁用或删除仍必须进入自启动管理页的预览、权限和回滚流程。</p></div></section></div></> : loading ? <div className="health-loading card-surface"><Loader2 className="spinning" size={22} /><span>正在读取程序、自启动和本机使用缓存…</span></div> : null}</section>;
}

function HealthHeader({ loading, onRefresh }: { loading: boolean; onRefresh: () => void }) {
  return <div className="section-header health-header"><div><h1><Activity size={20} />软件健康</h1><p>用本机证据发现需要复核的清单项，并提供安全的手动更新入口。</p></div><button type="button" className="icon-button" title="刷新健康检查" onClick={onRefresh} disabled={!isTauri() || loading}><RefreshCw className={loading ? "spinning" : ""} size={17} /></button></div>;
}

function HealthRow({ item }: { item: ProgramHealth }) {
  const topFinding = item.findings[0];
  return <article className="health-row"><div className={`health-score ${item.status}`}><strong>{item.score}</strong><small>/100</small></div><div className="health-row-main"><div className="health-row-title"><div><strong>{item.program_name}</strong><span>{item.publisher ?? "未知发布者"}{item.version ? ` · ${item.version}` : ""}</span></div><span className={`health-status ${item.status}`}>{item.status === "healthy" ? "状态良好" : "需要复核"}</span></div><div className="health-row-facts"><span>自启动 {item.startup_entry_count} 项 · {formatStartupImpact(item.startup_impact)}</span><span>{item.last_used ? `最近使用 ${formatDate(item.last_used)}` : "最近使用未知"}</span>{item.times_used !== null && <span>使用次数 {item.times_used}</span>}</div>{topFinding && <p className={`health-finding ${topFinding.severity}`}><FindingIcon severity={topFinding.severity} />{topFinding.title}：{topFinding.detail}</p>}{item.findings.length > 1 && <small className="health-more-findings">另有 {item.findings.length - 1} 项提示</small>}</div>{item.update_hint && <a className="health-update-link" href={item.update_hint.url} target="_blank" rel="noreferrer" title={item.update_hint.message}><ExternalLink size={14} />检查更新</a>}</article>;
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
  return { none: "无启用项", low: "低影响", medium: "中影响", high: "高影响", unknown: "未知" }[value];
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString("zh-CN");
}
