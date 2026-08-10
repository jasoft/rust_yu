import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Database,
  Download,
  FileCode2,
  FileText,
  Folder,
  Info,
  Loader2,
  Play,
  RefreshCw,
  Search,
  ShieldCheck,
  SquareActivity,
} from "lucide-react";
import { useInstallMonitorStore } from "../stores/installMonitor";
import { useProgramsStore } from "../stores/programs";
import type {
  InstallMonitorPlan,
  InstallMonitorSession,
  InstallMonitorSessionInfo,
  InstallMonitorStartRequest,
  InstallMonitorStatus,
  MonitorChange,
  MonitorChangeKind,
  MonitorConfidence,
  MonitorItemKind,
  MonitorRootInfo,
  Trace,
} from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

type ChangeFilter = "all" | MonitorChangeKind;

const statusMeta: Record<InstallMonitorStatus, { label: string; className: string }> = {
  waiting: { label: "等待安装完成", className: "waiting" },
  completed: { label: "已完成", className: "completed" },
  failed: { label: "存在读取失败", className: "failed" },
  cancelled: { label: "已停止", className: "cancelled" },
  expired: { label: "已过期", className: "expired" },
};

const changeMeta: Record<MonitorChangeKind, { label: string; className: string }> = {
  added: { label: "新增", className: "added" },
  removed: { label: "移除", className: "removed" },
  modified: { label: "修改", className: "modified" },
};

const itemLabels: Record<MonitorItemKind, string> = {
  file: "文件",
  directory: "目录",
  registry_key: "注册表键",
  registry_value: "注册表值",
};

const confidenceLabels: Record<MonitorConfidence, string> = {
  high: "高置信度",
  medium: "中置信度",
};

export function InstallMonitorManager() {
  const programs = useProgramsStore((state) => state.programs);
  const {
    plan,
    sessions,
    selectedSession,
    activeSessionId,
    loading,
    actionLoading,
    error,
    notice,
    load,
    planFor,
    start,
    complete,
    cancel,
    deleteSession,
    select,
    exportSession,
    getTraces,
    clearMessages,
  } = useInstallMonitorStore();
  const [selectedProgramId, setSelectedProgramId] = useState("");
  const [extraFiles, setExtraFiles] = useState("");
  const [extraRegistry, setExtraRegistry] = useState("");
  const [activityKind, setActivityKind] = useState<InstallMonitorStartRequest["activity_kind"]>("update");
  const [expiresMinutes, setExpiresMinutes] = useState(1440);
  const [changeFilter, setChangeFilter] = useState<ChangeFilter>("all");
  const [query, setQuery] = useState("");
  const [evidence, setEvidence] = useState<Trace[]>([]);

  useEffect(() => {
    if (isTauri()) void load();
  }, [load]);

  useEffect(() => {
    if (programs.length === 0 || programs.some((program) => program.id === selectedProgramId)) return;
    setSelectedProgramId(programs[0].id);
  }, [programs, selectedProgramId]);

  const selectedProgram = programs.find((program) => program.id === selectedProgramId) ?? null;
  const activeSession = activeSessionId ? sessions.find((session) => session.id === activeSessionId) ?? null : null;
  const request = selectedProgram ? buildRequest(selectedProgram, extraFiles, extraRegistry, activityKind, expiresMinutes) : null;

  const filteredChanges = useMemo(() => {
    if (!selectedSession) return [];
    const normalized = query.trim().toLocaleLowerCase("zh-CN");
    return selectedSession.changes.filter((change) => {
      if (changeFilter !== "all" && change.kind !== changeFilter) return false;
      if (!normalized) return true;
      return [change.path, change.description, change.evidence, itemLabels[change.item_kind]]
        .some((value) => value.toLocaleLowerCase("zh-CN").includes(normalized));
    });
  }, [changeFilter, query, selectedSession]);

  const handlePlan = async () => {
    if (!request) return;
    await planFor(request);
  };

  const handleStart = async () => {
    if (!request) return;
    const prepared = plan ?? await planFor(request);
    if (!prepared) return;
    await start(request);
  };

  const handleSelect = async (sessionId: string) => {
    setEvidence([]);
    await select(sessionId);
  };

  const handleComplete = async () => {
    if (!activeSessionId) return;
    const session = await complete(activeSessionId);
    if (session) setEvidence([]);
  };

  const handleEvidence = async () => {
    if (!selectedSession) return;
    const traces = await getTraces(selectedSession.id);
    setEvidence(traces);
  };

  return (
    <section className="page install-monitor-page">
      <div className="section-header install-monitor-header">
        <div>
          <h1><SquareActivity size={20} />安装监控</h1>
          <p>记录安装前后明确范围内的文件与注册表变化，帮助确认安装器实际写入了什么。</p>
        </div>
        <button type="button" className="icon-button" title="刷新监控会话" onClick={() => void load()} disabled={!isTauri() || loading || actionLoading}>
          <RefreshCw className={loading ? "spinning" : ""} size={17} />
        </button>
      </div>

      {!isTauri() ? (
        <div className="monitor-runtime-note card-surface"><Info size={17} /><div><strong>请在 Rust Yu 桌面应用中使用安装监控</strong><p>浏览器预览不会读取本机文件系统或注册表，也不会创建快照。</p></div></div>
      ) : (
        <>
          {error && <div className="monitor-notice error"><AlertTriangle size={15} /><span>{error}</span><button type="button" onClick={clearMessages}>关闭</button></div>}
          {notice && !error && <div className="monitor-notice success"><CheckCircle2 size={15} /><span>{notice}</span><button type="button" onClick={clearMessages}>关闭</button></div>}

          <div className="monitor-workspace">
            <section className="monitor-setup card-surface">
              <div className="monitor-panel-header"><div><strong>创建监控会话</strong><span>开始安装前先保存基线，安装结束后再生成差异。</span></div><ShieldCheck size={19} /></div>
              <div className="monitor-form">
                <label className="monitor-field"><span>已安装程序</span><select value={selectedProgramId} onChange={(event) => { setSelectedProgramId(event.target.value); useInstallMonitorStore.getState().setPlan(null); }} disabled={actionLoading || programs.length === 0}><option value="">选择程序…</option>{programs.map((program) => <option key={program.id} value={program.id}>{program.name}{program.publisher ? ` · ${program.publisher}` : ""}</option>)}</select></label>
                <label className="monitor-field"><span>会话类型</span><select value={activityKind} onChange={(event) => setActivityKind(event.target.value as InstallMonitorStartRequest["activity_kind"])} disabled={actionLoading}><option value="install">全新安装</option><option value="update">软件更新</option><option value="normal_run">正常运行</option></select></label>
                <label className="monitor-field"><span>有效期</span><select value={expiresMinutes} onChange={(event) => setExpiresMinutes(Number(event.target.value))} disabled={actionLoading}><option value={60}>1 小时</option><option value={1440}>24 小时</option><option value={10080}>7 天</option></select></label>
                <label className="monitor-field"><span>额外文件目录 <small>可选，每行一个</small></span><textarea value={extraFiles} onChange={(event) => { setExtraFiles(event.target.value); useInstallMonitorStore.getState().setPlan(null); }} placeholder="例如：D:\\Shared\\MyApp" rows={2} disabled={actionLoading} /></label>
                <label className="monitor-field"><span>额外注册表键 <small>可选，每行一个</small></span><textarea value={extraRegistry} onChange={(event) => { setExtraRegistry(event.target.value); useInstallMonitorStore.getState().setPlan(null); }} placeholder="例如：HKCU\\Software\\Vendor\\Product" rows={2} disabled={actionLoading} /></label>
              </div>
              <div className="monitor-form-actions"><button type="button" className="secondary-button" disabled={!request || actionLoading} onClick={() => void handlePlan()}>{actionLoading && !activeSessionId ? <Loader2 className="spinning" size={14} /> : <Search size={14} />}生成范围预览</button><button type="button" className="primary-button" disabled={!request || !plan || actionLoading || activeSessionId !== null} onClick={() => void handleStart()}><Play size={14} />开始记录安装前快照</button></div>
              {activeSession && <div className="monitor-active-session"><span className="monitor-live-dot" /><div><strong>正在记录：{activeSession.program_name}</strong><small>{activeSession.activity_kind} · 到期 {activeSession.expires_at ? new Date(activeSession.expires_at).toLocaleString("zh-CN") : "未设置"}</small></div><button type="button" className="secondary-button" disabled={actionLoading} onClick={() => void cancel(activeSession.id)}>停止</button><button type="button" className="primary-button" disabled={actionLoading} onClick={() => void handleComplete()}>{actionLoading ? <Loader2 className="spinning" size={14} /> : <Check size={14} />}采集结束快照</button></div>}
            </section>

            <PlanPreview plan={plan} />
          </div>

          <div className="monitor-content-grid">
            <SessionList sessions={sessions} loading={loading} selectedId={selectedSession?.id ?? null} activeId={activeSessionId} onSelect={(id) => void handleSelect(id)} />
            <SessionDetail session={selectedSession} filteredChanges={filteredChanges} changeFilter={changeFilter} query={query} onFilter={setChangeFilter} onQuery={setQuery} evidence={evidence} onEvidence={() => void handleEvidence()} onExport={(format) => selectedSession && void exportSession(selectedSession.id, format)} onDelete={() => selectedSession && void deleteSession(selectedSession.id)} busy={actionLoading} />
          </div>
        </>
      )}
    </section>
  );
}

function buildRequest(program: InstallMonitorStartRequest["program"], extraFiles: string, extraRegistry: string, activityKind: InstallMonitorStartRequest["activity_kind"], expiresMinutes: number): InstallMonitorStartRequest {
  return {
    program,
    extra_file_roots: splitLines(extraFiles),
    extra_registry_roots: splitLines(extraRegistry),
    activity_kind: activityKind,
    expires_after_minutes: expiresMinutes,
  };
}

function splitLines(value: string): string[] {
  return value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
}

function PlanPreview({ plan }: { plan: InstallMonitorPlan | null }) {
  if (!plan) return <section className="monitor-plan card-surface monitor-plan-empty"><Info size={24} /><strong>先选择程序并预览监控范围</strong><span>范围会限制在安装目录、程序相关用户数据、卸载键和精确推导的注册表键。</span></section>;
  const enabledFileRoots = plan.file_roots.filter((root) => root.enabled).length;
  const enabledRegistryRoots = plan.registry_roots.filter((root) => root.enabled).length;
  return <section className="monitor-plan card-surface">
    <div className="monitor-panel-header"><div><strong>监控范围预览</strong><span>{plan.program_name} · 只读快照，不会修改系统</span></div><span className="monitor-plan-count">{enabledFileRoots + enabledRegistryRoots} 个范围</span></div>
    {plan.requires_admin && <div className="monitor-inline-warning"><AlertTriangle size={14} /><span>部分范围可能需要管理员权限。读取失败会记录为警告，不会被伪装成“没有变化”。</span></div>}
    <div className="monitor-root-groups"><RootGroup icon={<Folder size={14} />} title={`文件目录 ${enabledFileRoots}`} roots={plan.file_roots} /><RootGroup icon={<Database size={14} />} title={`注册表键 ${enabledRegistryRoots}`} roots={plan.registry_roots} /></div>
    {plan.warnings.length > 0 && <div className="monitor-plan-warnings"><AlertTriangle size={13} /><span>{plan.warnings.length > 2 ? `${plan.warnings.slice(0, 2).join("；")} 等 ${plan.warnings.length} 条提示` : plan.warnings.join("；")}</span></div>}
  </section>;
}

function RootGroup({ icon, title, roots }: { icon: React.ReactNode; title: string; roots: MonitorRootInfo[] }) {
  return <div className="monitor-root-group"><h3>{icon}{title}</h3>{roots.length === 0 ? <span className="monitor-root-empty">没有推导出默认范围</span> : roots.map((root) => <div className={`monitor-root ${root.enabled ? "enabled" : "disabled"}`} key={`${root.source}-${root.path}`}><span className="monitor-root-state">{root.enabled ? <Check size={10} /> : <AlertTriangle size={10} />}</span><div><strong>{root.path}</strong><small>{root.source}{root.reason ? ` · ${root.reason}` : ""}</small></div></div>)}</div>;
}

function SessionList({ sessions, loading, selectedId, activeId, onSelect }: { sessions: InstallMonitorSessionInfo[]; loading: boolean; selectedId: string | null; activeId: string | null; onSelect: (id: string) => void }) {
  return <section className="monitor-sessions card-surface"><div className="monitor-panel-header"><div><strong>监控会话</strong><span>本机保存的安装前后快照</span></div><span className="monitor-session-total">{sessions.length}</span></div><div className="monitor-session-list">{loading && sessions.length === 0 ? <div className="monitor-empty"><Loader2 className="spinning" size={18} />正在读取会话…</div> : sessions.length === 0 ? <div className="monitor-empty"><FileText size={23} /><span>还没有安装监控会话</span><small>选择程序后开始保存安装前快照。</small></div> : sessions.map((session) => <button type="button" className={`monitor-session ${selectedId === session.id ? "selected" : ""}`} key={session.id} onClick={() => onSelect(session.id)}><span className="monitor-session-icon">{activeId === session.id ? <span className="monitor-live-dot" /> : <SquareActivity size={15} />}</span><span className="monitor-session-copy"><strong>{session.program_name}</strong><small>{formatDate(session.created_at)} · {session.id.slice(0, 8)}</small></span><span className={`monitor-status ${statusMeta[session.status].className}`}>{statusMeta[session.status].label}</span><span className="monitor-session-count">{session.changes_count} 项变化</span></button>)}</div></section>;
}

function SessionDetail({ session, filteredChanges, changeFilter, query, onFilter, onQuery, evidence, onEvidence, onExport, onDelete, busy }: { session: InstallMonitorSession | null; filteredChanges: MonitorChange[]; changeFilter: ChangeFilter; query: string; onFilter: (filter: ChangeFilter) => void; onQuery: (query: string) => void; evidence: Trace[]; onEvidence: () => void; onExport: (format: "json" | "csv") => void; onDelete: () => void; busy: boolean }) {
  if (!session) return <section className="monitor-detail card-surface monitor-detail-empty"><span><FileCode2 size={28} /></span><strong>选择会话查看安装差异</strong><p>新增与修改项目会保留证据说明；移除项目只用于审计，不会自动变成卸载删除目标。</p></section>;
  const added = session.changes.filter((change) => change.kind === "added").length;
  const removed = session.changes.filter((change) => change.kind === "removed").length;
  const modified = session.changes.filter((change) => change.kind === "modified").length;
  return <section className="monitor-detail card-surface"><div className="monitor-detail-header"><div><strong>{session.program.name}</strong><small>{session.activity_kind} · {formatDate(session.created_at)}{session.completed_at ? ` → ${formatDate(session.completed_at)}` : " · 尚未完成"}</small></div><div className="monitor-detail-actions"><button type="button" className="secondary-button compact-button" disabled={busy} onClick={() => onExport("json")}><Download size={13} />JSON</button><button type="button" className="secondary-button compact-button" disabled={busy} onClick={() => onExport("csv")}><Download size={13} />CSV</button><button type="button" className="danger-button compact-button" disabled={busy || session.status === "waiting"} onClick={onDelete}>删除</button></div></div><div className="monitor-detail-summary"><MonitorStat label="新增" value={added} tone="added" /><MonitorStat label="修改" value={modified} tone="modified" /><MonitorStat label="移除" value={removed} tone="removed" /><MonitorStat label="证据事件" value={session.evidence_events.length} tone="warning" /></div>{session.warnings.length > 0 && <div className="monitor-detail-warning"><AlertTriangle size={13} /><span>{session.warnings.length} 条读取警告：{session.warnings[0]}{session.warnings.length > 1 ? " 等" : ""}</span></div>}<div className="monitor-change-toolbar"><label className="search-box"><Search size={14} /><input value={query} onChange={(event) => onQuery(event.target.value)} placeholder="搜索路径或证据说明" /></label><div className="monitor-filter-pills"><button className={changeFilter === "all" ? "active" : ""} onClick={() => onFilter("all")}>全部 {session.changes.length}</button>{(["added", "modified", "removed"] as MonitorChangeKind[]).map((kind) => <button key={kind} className={changeFilter === kind ? "active" : ""} onClick={() => onFilter(kind)}>{changeMeta[kind].label} {kind === "added" ? added : kind === "modified" ? modified : removed}</button>)}</div></div><div className="monitor-change-list">{filteredChanges.length === 0 ? <div className="monitor-empty"><Search size={22} /><span>没有符合条件的变化</span></div> : filteredChanges.map((change) => <ChangeRow change={change} key={change.id} />)}</div><div className="monitor-detail-footer"><div><span>显示 {filteredChanges.length} / {session.changes.length} 项 · 事件图 {session.evidence_events.length} 条</span><small>{evidence.length > 0 ? `已转换 ${evidence.length} 项高/中置信度 Trace` : "Trace 仅包含新增和修改项目"}</small></div><button type="button" className="primary-button" disabled={busy || session.status !== "completed"} onClick={onEvidence}><ShieldCheck size={14} />转换为卸载证据</button></div>{evidence.length > 0 && <div className="monitor-evidence"><CheckCircle2 size={14} /><span>已生成标准卸载证据，可在卸载审查阶段复核：</span>{evidence.slice(0, 3).map((trace) => <code key={trace.id}>{trace.path}</code>)}{evidence.length > 3 && <small>等 {evidence.length} 项</small>}</div>}</section>;
}

function ChangeRow({ change }: { change: MonitorChange }) {
  const meta = changeMeta[change.kind];
  return <article className="monitor-change-row"><span className={`monitor-change-badge ${meta.className}`}>{meta.label}</span><span className="monitor-change-kind">{itemLabels[change.item_kind]}</span><div className="monitor-change-copy"><strong title={change.path}>{change.path}</strong><small>{change.description} · {change.evidence}</small></div><span className={`monitor-confidence ${change.confidence}`}>{confidenceLabels[change.confidence]}</span><span className="monitor-change-size">{formatChangeSize(change.size_before, change.size_after)}</span></article>;
}

function MonitorStat({ label, value, tone }: { label: string; value: number; tone: string }) {
  return <div className={`monitor-stat ${tone}`}><span>{label}</span><strong>{value}</strong></div>;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString("zh-CN");
}

function formatChangeSize(before: number | null, after: number | null): string {
  if (before === null && after === null) return "—";
  if (before === null) return `+${formatBytes(after ?? 0)}`;
  if (after === null) return `-${formatBytes(before)}`;
  return `${formatBytes(before)} → ${formatBytes(after)}`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}
