import { t, type TranslationKey } from "../i18n/index.ts";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Archive, CheckCircle2, Database, FileSearch, Loader2, Moon, RefreshCw, ShieldCheck, Sun, Upload } from "lucide-react";
import { policyActionLabel } from "../lib/evidenceLabels";
import { useProgramsStore } from "../stores/programs";
import type { CleanupPolicyKind, CleanupPolicyProfile, CommandError, EvidenceConfidence, HibernationPlan, HibernationResult, InventoryBaseline, InventoryComparison, ReconstructedEvidencePacket } from "../types";

type Tab = "forensics" | "hibernate" | "baseline" | "policy";
const isTauri = () => "__TAURI_INTERNALS__" in window;

const policyTextKeys: Record<CleanupPolicyKind, { title: TranslationKey; badge: TranslationKey; description: TranslationKey }> = {
  audit: {
    title: "components.evidencecenter.policy.audit.title",
    badge: "components.evidencecenter.policy.audit.badge",
    description: "components.evidencecenter.policy.audit.description",
  },
  safe: {
    title: "components.evidencecenter.policy.safe.title",
    badge: "components.evidencecenter.policy.safe.badge",
    description: "components.evidencecenter.policy.safe.description",
  },
  recovery: {
    title: "components.evidencecenter.policy.recovery.title",
    badge: "components.evidencecenter.policy.recovery.badge",
    description: "components.evidencecenter.policy.recovery.description",
  },
};

const evidenceCategoryKeys: Record<string, TranslationKey> = {
  uninstall_registry: "components.evidencecenter.category.uninstall_record",
  install_directory: "components.evidencecenter.category.install_folder",
  registry: "components.evidencecenter.category.system_setting",
  filesystem: "components.evidencecenter.category.file",
  appdata: "components.evidencecenter.category.personal_data",
  scheduled_task: "components.evidencecenter.category.scheduled_start",
  service: "components.evidencecenter.category.background_service",
  driver: "components.evidencecenter.category.driver",
};

const evidenceSourceKeys: Record<string, TranslationKey> = {
  "installed_program.uninstall_registry_key_path": "components.evidencecenter.source.software_list",
  "installed_program.install_location": "components.evidencecenter.source.install_information",
  post_hoc_scanner: "components.evidencecenter.source.safety_scan",
};

const confidenceKeys: Record<EvidenceConfidence, TranslationKey> = {
  high: "components.evidencecenter.confidence.high",
  medium: "components.evidencecenter.confidence.medium",
  low: "components.evidencecenter.confidence.low",
  unknown: "components.evidencecenter.confidence.unknown",
};

function evidenceLabel(value: string, keys: Record<string, TranslationKey>): string {
  const key = keys[value];
  return key ? t(key) : t("components.evidencecenter.value.unknown");
}

function signatureLabel(status: string): string {
  const normalized = status.toLowerCase().replace(/[^a-z]/g, "");
  return normalized === "valid"
    ? t("components.evidencecenter.signature.valid")
    : normalized === "notsigned"
      ? t("components.evidencecenter.signature.unsigned")
      : t("components.evidencecenter.signature.unknown");
}

function differenceStatusLabel(status: string): string {
  const keys: Record<string, TranslationKey> = {
    missing: "components.evidencecenter.difference.missing",
    added: "components.evidencecenter.difference.added",
    version_changed: "components.evidencecenter.difference.version_changed",
  };
  return evidenceLabel(status, keys);
}

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
    setPacket(result); setNotice(t("components.evidencecenter.message_001", { value0: result.evidence.length }));
  });
  const planHibernate = () => selectedProgram && run(async () => {
    const result = await invoke<HibernationPlan>("plan_program_hibernation", { program: selectedProgram });
    setHibernatePlan(result); setHibernateResult(null); setConfirmed(false);
    setNotice(t("components.evidencecenter.message_002", { value0: result.selected_item_ids.length }));
  });
  const applyHibernate = () => hibernatePlan && run(async () => {
    const result = await invoke<HibernationResult>("apply_program_hibernation", { plan: hibernatePlan, itemIds: hibernatePlan.selected_item_ids, confirm: confirmed });
    setHibernateResult(result); setNotice(result.applied ? t("components.evidencecenter.message_003") : t("components.evidencecenter.message_004"));
  });
  const wake = () => hibernateResult && run(async () => {
    const result = await invoke<HibernationResult>("wake_program_hibernation", { changeIds: hibernateResult.change_ids, confirm: true });
    setHibernateResult(result); setNotice(result.applied ? t("components.evidencecenter.message_005") : t("components.evidencecenter.message_006"));
  });
  const createBaseline = () => run(async () => {
    const result = await invoke<InventoryBaseline>("create_inventory_baseline", { machineLabel: window.navigator.userAgent });
    setBaseline(result); setBaselineJson(JSON.stringify(result, null, 2)); setNotice(t("components.evidencecenter.message_007", { value0: result.entries.length }));
  });
  const compareBaseline = () => run(async () => {
    let imported: InventoryBaseline;
    try { imported = JSON.parse(baselineJson) as InventoryBaseline; } catch { throw new Error(t("components.evidencecenter.message_008")); }
    const result = await invoke<InventoryComparison>("compare_inventory_baseline", { baseline: imported });
    setComparison(result); setNotice(t("components.evidencecenter.message_009", { value0: result.differences.length }));
  });

  const tabs = useMemo(() => [
    ["forensics", t("components.evidencecenter.message_010"), FileSearch], ["hibernate", t("components.evidencecenter.message_011"), Moon],
    ["baseline", t("components.evidencecenter.message_012"), Database], ["policy", t("components.evidencecenter.message_013"), ShieldCheck],
  ] as const, []);

  return <section className="page evidence-center">
    <div className="section-header evidence-header"><div><h1><Archive size={20} />{t("components.evidencecenter.message_014")}</h1><p>{t("components.evidencecenter.message_015")}</p></div><button className="icon-button" title={t("components.evidencecenter.message_016")} disabled={busy || !isTauri()} onClick={() => void reloadPrograms({ refresh: true })}><RefreshCw className={busy ? "spinning" : ""} size={17} /></button></div>
    {!isTauri() ? <div className="evidence-notice info card-surface">{t("components.evidencecenter.message_017")}</div> : <>
      <div className="evidence-tabs">{tabs.map(([id, label, Icon]) => <button key={id} className={tab === id ? "active" : ""} onClick={() => setTab(id)}><Icon size={14} />{label}</button>)}</div>
      {error && <div className="evidence-notice error"><AlertTriangle size={14} />{error}</div>}
      {notice && !error && <div className="evidence-notice success"><CheckCircle2 size={14} />{notice}</div>}
      {(tab === "forensics" || tab === "hibernate") && <div className="evidence-program-bar card-surface"><label><span>{t("components.evidencecenter.message_018")}</span><select value={programId} onChange={(event) => { setProgramId(event.target.value); setPacket(null); setHibernatePlan(null); }}>{programs.map((program) => <option key={program.id} value={program.id}>{program.name} · {program.publisher ?? t("app.message_016")}</option>)}</select></label><small>{t("components.evidencecenter.message_020")}</small></div>}
      {tab === "forensics" && <Forensics packet={packet} busy={busy} onRun={reconstruct} />}
      {tab === "hibernate" && <Hibernate plan={hibernatePlan} result={hibernateResult} busy={busy} confirmed={confirmed} onConfirm={setConfirmed} onPlan={planHibernate} onApply={applyHibernate} onWake={wake} />}
      {tab === "baseline" && <Baseline baseline={baseline} json={baselineJson} comparison={comparison} busy={busy} onJson={setBaselineJson} onCreate={createBaseline} onCompare={compareBaseline} />}
      {tab === "policy" && <Policy profiles={profiles} />}
    </>}
  </section>;
}

function Forensics({ packet, busy, onRun }: { packet: ReconstructedEvidencePacket | null; busy: boolean; onRun: () => void }) {
  return <div className="evidence-panel card-surface"><header><div><strong>{t("components.evidencecenter.message_021")}</strong><span>{t("components.evidencecenter.message_022")}</span></div><button className="primary-button" disabled={busy} onClick={onRun}>{busy ? <Loader2 className="spinning" size={14} /> : <FileSearch size={14} />}{t("components.evidencecenter.message_023")}</button></header>{packet ? <><div className="evidence-callout"><AlertTriangle size={14} /><span>{t("components.evidencecenter.message_070")}</span></div><div className="evidence-facts"><span>{t("app.message_054")}</span><strong>{packet.vendor ?? t("app.message_296")}</strong><span>{t("components.evidencecenter.message_026")}</span><strong>{signatureLabel(packet.signature_status)}</strong><span>{t("components.evidencecenter.message_027")}</span><strong>{packet.evidence.length}</strong><span>{t("components.evidencecenter.message_071")}</span><strong>{t("components.evidencecenter.message_072")}</strong></div><div className="evidence-table"><div className="evidence-row head"><span>{t("components.evidencecenter.message_028")}</span><span>{t("components.evidencecenter.message_029")}</span><span>{t("components.evidencecenter.message_030")}</span><span>{t("app.message_248")}</span><span>{t("components.evidencecenter.message_032")}</span></div>{packet.evidence.map((item) => <div className="evidence-row" key={item.id}><span>{evidenceLabel(item.category, evidenceCategoryKeys)}</span><code title={item.target}>{item.target}</code><span>{evidenceLabel(item.source, evidenceSourceKeys)}</span><span className={`confidence ${item.confidence}`}>{t(confidenceKeys[item.confidence])}</span><span>{item.destructive_eligible ? t("components.evidencecenter.message_033") : t("components.evidencecenter.message_034")}</span></div>)}</div></> : <Empty icon={<FileSearch size={28} />} title={t("components.evidencecenter.message_035")} text={t("components.evidencecenter.message_036")} />}</div>;
}

function Hibernate({ plan, result, busy, confirmed, onConfirm, onPlan, onApply, onWake }: { plan: HibernationPlan | null; result: HibernationResult | null; busy: boolean; confirmed: boolean; onConfirm: (value: boolean) => void; onPlan: () => void; onApply: () => void; onWake: () => void }) {
  return <div className="evidence-panel card-surface"><header><div><strong>{t("components.evidencecenter.message_037")}</strong><span>{t("components.evidencecenter.message_038")}</span></div><button className="secondary-button" disabled={busy} onClick={onPlan}><Moon size={14} />{t("components.evidencecenter.message_039")}</button></header>{plan ? <><div className="evidence-facts"><span>{t("components.evidencecenter.message_040")}</span><strong>{plan.last_used ?? t("app.message_296")}</strong><span>{t("components.evidencecenter.message_042")}</span><strong>{plan.times_used ?? t("app.message_296")}</strong><span>{t("components.evidencecenter.message_044")}</span><strong>{plan.selected_item_ids.length}</strong><span>{t("components.evidencecenter.message_045")}</span><strong>{plan.candidates.length}</strong></div><div className="hibernate-list">{plan.candidates.map((item) => <div key={item.item_id} className={item.prohibited ? "prohibited" : "allowed"}><div><strong>{item.name}</strong><code>{item.command ?? item.source}</code></div><span>{t(item.prohibited ? "components.evidencecenter.hibernate.protected" : "components.evidencecenter.hibernate.restorable")}</span></div>)}</div><footer><label className="confirm-check"><input type="checkbox" checked={confirmed} onChange={(event) => onConfirm(event.target.checked)} />{t("components.evidencecenter.message_046")}</label><button className="primary-button" disabled={busy || !confirmed || plan.selected_item_ids.length === 0} onClick={onApply}><Moon size={14} />{t("components.evidencecenter.message_047")}</button>{result?.change_ids.length ? <button className="secondary-button" disabled={busy} onClick={onWake}><Sun size={14} />{t("components.evidencecenter.message_048")}</button> : null}</footer></> : <Empty icon={<Moon size={28} />} title={t("components.evidencecenter.message_049")} text={t("components.evidencecenter.message_050")} />}</div>;
}

function Baseline({ baseline, json, comparison, busy, onJson, onCreate, onCompare }: { baseline: InventoryBaseline | null; json: string; comparison: InventoryComparison | null; busy: boolean; onJson: (value: string) => void; onCreate: () => void; onCompare: () => void }) {
  return <div className="baseline-grid"><section className="evidence-panel card-surface"><header><div><strong>{t("components.evidencecenter.message_051")}</strong><span>{t("components.evidencecenter.message_052")}</span></div><button className="primary-button" disabled={busy} onClick={onCreate}><Database size={14} />{t("components.evidencecenter.message_053")}</button></header><textarea className="baseline-json" value={json} onChange={(event) => onJson(event.target.value)} placeholder={t("components.evidencecenter.message_054")} /><footer><span>{baseline ? t("components.evidencecenter.message_055", { value0: baseline.entries.length, value1: baseline.captured_at }) : t("components.evidencecenter.message_056")}</span><button className="secondary-button" disabled={busy || !json.trim()} onClick={onCompare}><Upload size={14} />{t("components.evidencecenter.message_057")}</button></footer></section><section className="evidence-panel card-surface"><header><div><strong>{t("components.evidencecenter.message_058")}</strong><span>{t("components.evidencecenter.message_059")}</span></div></header>{comparison ? <div className="difference-list"><div className="evidence-callout"><ShieldCheck size={14} />{t("components.evidencecenter.baseline.read_only")}</div>{comparison.differences.map((item) => <div key={item.key}><span className={`difference-status ${item.status}`}>{differenceStatusLabel(item.status)}</span><strong>{item.name}</strong><small>{item.baseline_version ?? "—"} → {item.current_version ?? "—"}</small></div>)}</div> : <Empty icon={<Database size={28} />} title={t("components.evidencecenter.message_060")} text={t("components.evidencecenter.message_061")} />}</section></div>;
}

function Policy({ profiles }: { profiles: CleanupPolicyProfile[] }) {
  return <div className="policy-grid">{profiles.map((profile) => {
    const copy = policyTextKeys[profile.kind];
    return <section key={profile.kind} className="policy-card card-surface"><header><ShieldCheck size={20} /><div><strong>{t(copy.title)}</strong><span>{t(copy.badge)}</span></div></header><p>{t(copy.description)}</p><dl><dt>{t("components.evidencecenter.message_062")}</dt><dd>{profile.require_confirmation ? t("components.evidencecenter.message_063") : t("components.evidencecenter.policy.not_required")}</dd><dt>{t("components.evidencecenter.message_064")}</dt><dd>{profile.require_backup ? t("components.evidencecenter.message_063") : profile.kind === "recovery" ? t("components.evidencecenter.message_066") : t("components.evidencecenter.message_067")}</dd><dt>{t("components.evidencecenter.message_068")}</dt><dd>{profile.allowed_actions.map(policyActionLabel).join(" · ")}</dd><dt>{t("components.evidencecenter.message_069")}</dt><dd>{profile.irreversible_actions.length ? profile.irreversible_actions.map(policyActionLabel).join(" · ") : t("app.message_067")}</dd></dl></section>;
  })}</div>;
}

function Empty({ icon, title, text }: { icon: React.ReactNode; title: string; text: string }) { return <div className="evidence-empty">{icon}<strong>{title}</strong><span>{text}</span></div>; }
