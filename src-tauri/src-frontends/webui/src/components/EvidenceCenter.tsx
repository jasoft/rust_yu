import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Archive, CheckCircle2, Database, FileSearch, Loader2, Moon, RefreshCw, ShieldCheck, Sun, Upload } from "lucide-react";
import { useProgramsStore } from "../stores/programs";
import type { CleanupPolicyProfile, CommandError, HibernationPlan, HibernationResult, InventoryBaseline, InventoryComparison, ReconstructedEvidencePacket } from "../types";

type Tab = "forensics" | "hibernate" | "baseline" | "policy";
const isTauri = () => "__TAURI_INTERNALS__" in window;

function errorText(error: unknown) {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String((error as CommandError).message);
  return String(error);
}

export function EvidenceCenter() {
  const programs = useProgramsStore((state) => state.programs);
  const reloadPrograms = useProgramsStore((state) => state.reloadPrograms);
  const [tab, setTab] = useState<Tab>("forensics");
  const [programId, setProgramId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [packet, setPacket] = useState<ReconstructedEvidencePacket | null>(null);
  const [hibernatePlan, setHibernatePlan] = useState<HibernationPlan | null>(null);
  const [hibernateResult, setHibernateResult] = useState<HibernationResult | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [baseline, setBaseline] = useState<InventoryBaseline | null>(null);
  const [baselineJson, setBaselineJson] = useState("");
  const [comparison, setComparison] = useState<InventoryComparison | null>(null);
  const [profiles, setProfiles] = useState<CleanupPolicyProfile[]>([]);
  const selectedProgram = programs.find((program) => program.id === programId) ?? null;

  useEffect(() => {
    if (!isTauri()) return;
    if (programs.length === 0) void reloadPrograms();
    void invoke<CleanupPolicyProfile[]>("list_cleanup_policy_profiles").then(setProfiles).catch((reason) => setError(errorText(reason)));
  }, [programs.length, reloadPrograms]);
  useEffect(() => {
    if (!programId && programs[0]) setProgramId(programs[0].id);
  }, [programId, programs]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true); setError(null); setNotice(null);
    try { await action(); } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };

  const reconstruct = () => selectedProgram && run(async () => {
    const result = await invoke<ReconstructedEvidencePacket>("reconstruct_installation", { program: selectedProgram });
    setPacket(result); setNotice(`已生成 ${result.evidence.length} 条事后证据；未知与低置信度项目保持保留。`);
  });
  const planHibernate = () => selectedProgram && run(async () => {
    const result = await invoke<HibernationPlan>("plan_program_hibernation", { program: selectedProgram });
    setHibernatePlan(result); setHibernateResult(null); setConfirmed(false);
    setNotice(`影响分析完成：${result.selected_item_ids.length} 项可安全休眠。`);
  });
  const applyHibernate = () => hibernatePlan && run(async () => {
    const result = await invoke<HibernationResult>("apply_program_hibernation", { plan: hibernatePlan, itemIds: hibernatePlan.selected_item_ids, confirm: confirmed });
    setHibernateResult(result); setNotice(result.applied ? "休眠已应用，并保存了可回滚变更。" : "休眠部分失败，请检查错误后唤醒已变更项目。");
  });
  const wake = () => hibernateResult && run(async () => {
    const result = await invoke<HibernationResult>("wake_program_hibernation", { changeIds: hibernateResult.change_ids, confirm: true });
    setHibernateResult(result); setNotice(result.applied ? "已恢复休眠前状态。" : "部分恢复失败，可再次重试。");
  });
  const createBaseline = () => run(async () => {
    const result = await invoke<InventoryBaseline>("create_inventory_baseline", { machineLabel: window.navigator.userAgent });
    setBaseline(result); setBaselineJson(JSON.stringify(result, null, 2)); setNotice(`已保存 ${result.entries.length} 个软件条目的本机基线。`);
  });
  const compareBaseline = () => run(async () => {
    let imported: InventoryBaseline;
    try { imported = JSON.parse(baselineJson) as InventoryBaseline; } catch { throw new Error("基线 JSON 无法解析。"); }
    const result = await invoke<InventoryComparison>("compare_inventory_baseline", { baseline: imported });
    setComparison(result); setNotice(`只读比较完成：${result.differences.length} 项差异。`);
  });

  const tabs = useMemo(() => [
    ["forensics", "存量取证", FileSearch], ["hibernate", "安全休眠", Moon],
    ["baseline", "迁移基线", Database], ["policy", "清理策略", ShieldCheck],
  ] as const, []);

  return <section className="page evidence-center">
    <div className="section-header evidence-header"><div><h1><Archive size={20} />证据与策略中心</h1><p>事后取证、可回滚休眠、只读迁移对比与不可绕过的清理边界。</p></div><button className="icon-button" title="刷新软件清单" disabled={busy || !isTauri()} onClick={() => void reloadPrograms({ refresh: true })}><RefreshCw className={busy ? "spinning" : ""} size={17} /></button></div>
    {!isTauri() ? <div className="evidence-notice info card-surface">请在 Rust Yu 桌面应用中使用本机取证与系统管理能力。</div> : <>
      <div className="evidence-tabs">{tabs.map(([id, label, Icon]) => <button key={id} className={tab === id ? "active" : ""} onClick={() => setTab(id)}><Icon size={14} />{label}</button>)}</div>
      {error && <div className="evidence-notice error"><AlertTriangle size={14} />{error}</div>}
      {notice && !error && <div className="evidence-notice success"><CheckCircle2 size={14} />{notice}</div>}
      {(tab === "forensics" || tab === "hibernate") && <div className="evidence-program-bar card-surface"><label><span>目标软件</span><select value={programId} onChange={(event) => { setProgramId(event.target.value); setPacket(null); setHibernatePlan(null); }}>{programs.map((program) => <option key={program.id} value={program.id}>{program.name} · {program.publisher ?? "未知发布者"}</option>)}</select></label><small>所有分析先读取证据；不会因名称相似自动提升置信度。</small></div>}
      {tab === "forensics" && <Forensics packet={packet} busy={busy} onRun={reconstruct} />}
      {tab === "hibernate" && <Hibernate plan={hibernatePlan} result={hibernateResult} busy={busy} confirmed={confirmed} onConfirm={setConfirmed} onPlan={planHibernate} onApply={applyHibernate} onWake={wake} />}
      {tab === "baseline" && <Baseline baseline={baseline} json={baselineJson} comparison={comparison} busy={busy} onJson={setBaselineJson} onCreate={createBaseline} onCompare={compareBaseline} />}
      {tab === "policy" && <Policy profiles={profiles} />}
    </>}
  </section>;
}

function Forensics({ packet, busy, onRun }: { packet: ReconstructedEvidencePacket | null; busy: boolean; onRun: () => void }) {
  return <div className="evidence-panel card-surface"><header><div><strong>F-14 · 存量安装取证</strong><span>重建卸载键、目录、AppData、注册表与系统集成证据。</span></div><button className="primary-button" disabled={busy} onClick={onRun}>{busy ? <Loader2 className="spinning" size={14} /> : <FileSearch size={14} />}开始只读取证</button></header>{packet ? <><div className="evidence-callout"><AlertTriangle size={14} /><span>{packet.inference_notice}</span></div><div className="evidence-facts"><span>发布者</span><strong>{packet.vendor ?? "未知"}</strong><span>签名</span><strong>{packet.signature_status}</strong><span>证据数</span><strong>{packet.evidence.length}</strong><span>SHA-256</span><code>{packet.immutable_sha256.slice(0, 20)}…</code></div><div className="evidence-table"><div className="evidence-row head"><span>类别</span><span>目标</span><span>来源</span><span>置信度</span><span>处置</span></div>{packet.evidence.map((item) => <div className="evidence-row" key={item.id}><span>{item.category}</span><code title={item.target}>{item.target}</code><span>{item.source}</span><span className={`confidence ${item.confidence}`}>{item.confidence}</span><span>{item.destructive_eligible ? "可进入备份门禁" : "默认保留"}</span></div>)}</div></> : <Empty icon={<FileSearch size={28} />} title="尚未生成事后证据包" text="低置信度和未知项不会自动成为删除候选。" />}</div>;
}

function Hibernate({ plan, result, busy, confirmed, onConfirm, onPlan, onApply, onWake }: { plan: HibernationPlan | null; result: HibernationResult | null; busy: boolean; confirmed: boolean; onConfirm: (value: boolean) => void; onPlan: () => void; onApply: () => void; onWake: () => void }) {
  return <div className="evidence-panel card-surface"><header><div><strong>F-18 · 安全休眠与影响分析</strong><span>只禁用与安装目录精确关联且有回滚快照的启动项或服务。</span></div><button className="secondary-button" disabled={busy} onClick={onPlan}><Moon size={14} />生成影响分析</button></header>{plan ? <><div className="evidence-facts"><span>最近使用</span><strong>{plan.last_used ?? "未知"}</strong><span>使用次数</span><strong>{plan.times_used ?? "未知"}</strong><span>可休眠</span><strong>{plan.selected_item_ids.length}</strong><span>候选总数</span><strong>{plan.candidates.length}</strong></div><div className="hibernate-list">{plan.candidates.map((item) => <div key={item.item_id} className={item.prohibited ? "prohibited" : "allowed"}><div><strong>{item.name}</strong><code>{item.command ?? item.source}</code></div><span>{item.reason}</span></div>)}</div><footer><label className="confirm-check"><input type="checkbox" checked={confirmed} onChange={(event) => onConfirm(event.target.checked)} />我确认仅禁用上述可回滚项目</label><button className="primary-button" disabled={busy || !confirmed || plan.selected_item_ids.length === 0} onClick={onApply}><Moon size={14} />应用休眠</button>{result?.change_ids.length ? <button className="secondary-button" disabled={busy} onClick={onWake}><Sun size={14} />唤醒并恢复</button> : null}</footer></> : <Empty icon={<Moon size={28} />} title="先生成只读影响分析" text="共享组件、系统路径和无法证明归属的项目默认禁止休眠。" />}</div>;
}

function Baseline({ baseline, json, comparison, busy, onJson, onCreate, onCompare }: { baseline: InventoryBaseline | null; json: string; comparison: InventoryComparison | null; busy: boolean; onJson: (value: string) => void; onCreate: () => void; onCompare: () => void }) {
  return <div className="baseline-grid"><section className="evidence-panel card-surface"><header><div><strong>F-19 · 软件清单基线</strong><span>导出来源、版本、路径和卸载能力；保存在本机。</span></div><button className="primary-button" disabled={busy} onClick={onCreate}><Database size={14} />创建当前基线</button></header><textarea className="baseline-json" value={json} onChange={(event) => onJson(event.target.value)} placeholder="创建基线，或粘贴另一台电脑导出的基线 JSON…" /><footer><span>{baseline ? `${baseline.entries.length} 项 · ${baseline.captured_at}` : "基线带 SHA-256，修改后会拒绝比较。"}</span><button className="secondary-button" disabled={busy || !json.trim()} onClick={onCompare}><Upload size={14} />只读比较当前系统</button></footer></section><section className="evidence-panel card-surface"><header><div><strong>迁移差异</strong><span>不下载、不安装、不卸载。</span></div></header>{comparison ? <div className="difference-list"><div className="evidence-callout"><ShieldCheck size={14} />{comparison.read_only_notice}</div>{comparison.differences.map((item) => <div key={item.key}><span className={`difference-status ${item.status}`}>{item.status}</span><strong>{item.name}</strong><small>{item.baseline_version ?? "—"} → {item.current_version ?? "—"} · {item.note}</small></div>)}</div> : <Empty icon={<Database size={28} />} title="尚未进行迁移比较" text="导入基线后只展示缺失、新增和版本变化。" />}</section></div>;
}

function Policy({ profiles }: { profiles: CleanupPolicyProfile[] }) {
  return <div className="policy-grid">{profiles.map((profile) => <section key={profile.kind} className="policy-card card-surface"><header><ShieldCheck size={20} /><div><strong>{profile.title}</strong><span>{profile.kind}</span></div></header><p>{profile.description}</p><dl><dt>用户确认</dt><dd>强制</dd><dt>备份门禁</dt><dd>{profile.require_backup ? "强制" : profile.kind === "recovery" ? "使用历史会话" : "不执行删除"}</dd><dt>允许操作</dt><dd>{profile.allowed_actions.join(" · ")}</dd><dt>不可逆操作</dt><dd>{profile.irreversible_actions.length ? profile.irreversible_actions.join(" · ") : "无"}</dd></dl></section>)}</div>;
}

function Empty({ icon, title, text }: { icon: React.ReactNode; title: string; text: string }) { return <div className="evidence-empty">{icon}<strong>{title}</strong><span>{text}</span></div>; }
