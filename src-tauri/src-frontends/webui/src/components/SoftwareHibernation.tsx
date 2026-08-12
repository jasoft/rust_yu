import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, Loader2, Moon, RefreshCw, Sun } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { t } from "../i18n/index.ts";
import { useProgramsStore } from "../stores/programs";
import type { HibernationPlan, HibernationResult } from "../types";

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}

export function SoftwareHibernation() {
  const programs = useProgramsStore((state) => state.programs);
  const reloadPrograms = useProgramsStore((state) => state.reloadPrograms);
  const [programId, setProgramId] = useState("");
  const [plan, setPlan] = useState<HibernationPlan | null>(null);
  const [result, setResult] = useState<HibernationResult | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (!programId && programs.length > 0) setProgramId(programs[0].id);
  }, [programId, programs]);

  const selectedProgram = useMemo(
    () => programs.find((program) => program.id === programId) ?? null,
    [programId, programs],
  );

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };

  const inspect = () => selectedProgram && run(async () => {
    const nextPlan = await invoke<HibernationPlan>("plan_program_hibernation", { program: selectedProgram });
    setPlan(nextPlan);
    setResult(null);
    setConfirmed(false);
    setNotice(t("components.softwarehibernation.analysis_complete", { value0: nextPlan.selected_item_ids.length }));
  });

  const apply = () => plan && run(async () => {
    const nextResult = await invoke<HibernationResult>("apply_program_hibernation", {
      plan,
      itemIds: plan.selected_item_ids,
      confirm: true,
    });
    setResult(nextResult);
    setNotice(nextResult.applied ? t("components.softwarehibernation.applied") : t("components.softwarehibernation.partial"));
  });

  const wake = () => result && run(async () => {
    const nextResult = await invoke<HibernationResult>("wake_program_hibernation", {
      changeIds: result.change_ids,
      confirm: true,
    });
    setResult(nextResult);
    setNotice(nextResult.applied ? t("components.softwarehibernation.restored") : t("components.softwarehibernation.restore_partial"));
  });

  if (!isTauriRuntime()) {
    return <div className="software-hibernate-preview card-surface"><Moon size={26} /><strong>{t("components.softwarehibernation.desktop_only_title")}</strong><span>{t("components.softwarehibernation.desktop_only_description")}</span></div>;
  }

  return (
    <section className="software-hibernate-panel card-surface">
      <header>
        <div>
          <strong>{t("components.softwarehibernation.title")}</strong>
          <span>{t("components.softwarehibernation.subtitle")}</span>
        </div>
        <button type="button" className="icon-button" title={t("components.softwarehibernation.refresh")} disabled={busy} onClick={() => void reloadPrograms({ refresh: true })}>
          <RefreshCw className={busy ? "spinning" : ""} size={16} />
        </button>
      </header>

      <div className="hibernate-program-picker">
        <label>
          <span>{t("components.softwarehibernation.program")}</span>
          <select value={programId} onChange={(event) => { setProgramId(event.target.value); setPlan(null); setResult(null); setConfirmed(false); }}>
            {programs.map((program) => <option value={program.id} key={program.id}>{program.name} · {program.publisher ?? t("app.message_016")}</option>)}
          </select>
        </label>
        <button type="button" className="primary-button" disabled={busy || !selectedProgram} onClick={() => void inspect()}>
          {busy ? <Loader2 className="spinning" size={14} /> : <Moon size={14} />}
          {t("components.softwarehibernation.inspect")}
        </button>
      </div>

      {error && <div className="hibernate-notice error"><AlertTriangle size={14} />{error}</div>}
      {notice && !error && <div className="hibernate-notice success"><CheckCircle2 size={14} />{notice}</div>}

      {plan ? (
        <>
          <div className="hibernate-impact-summary">
            <div><span>{t("components.softwarehibernation.safe_count")}</span><strong>{plan.selected_item_ids.length}</strong></div>
            <div><span>{t("components.softwarehibernation.total_count")}</span><strong>{plan.candidates.length}</strong></div>
            <div><span>{t("components.softwarehibernation.protected_count")}</span><strong>{plan.candidates.filter((item) => item.prohibited).length}</strong></div>
          </div>
          <div className="software-hibernate-list">
            {plan.candidates.map((item) => (
              <div key={item.item_id} className={item.prohibited ? "prohibited" : "restorable"}>
                <span className="hibernate-state">{item.prohibited ? t("components.softwarehibernation.protected") : t("components.softwarehibernation.restorable")}</span>
                <div><strong>{item.name}</strong><code>{item.command ?? item.source}</code></div>
                <small>{item.reason}</small>
              </div>
            ))}
            {plan.candidates.length === 0 && <div className="hibernate-empty"><CheckCircle2 size={24} />{t("components.softwarehibernation.no_candidates")}</div>}
          </div>
          <footer>
            <label><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />{t("components.softwarehibernation.confirm")}</label>
            <button type="button" className="primary-button" disabled={busy || !confirmed || plan.selected_item_ids.length === 0} onClick={() => void apply()}><Moon size={14} />{t("components.softwarehibernation.apply")}</button>
            {result?.change_ids.length ? <button type="button" className="secondary-button" disabled={busy} onClick={() => void wake()}><Sun size={14} />{t("components.softwarehibernation.wake")}</button> : null}
          </footer>
        </>
      ) : (
        <div className="hibernate-empty"><Moon size={28} /><strong>{t("components.softwarehibernation.empty_title")}</strong><span>{t("components.softwarehibernation.empty_description")}</span></div>
      )}
    </section>
  );
}
