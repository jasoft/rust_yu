import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, Database, Loader2, ShieldCheck, Upload } from "lucide-react";
import { useState } from "react";
import { t } from "../i18n/index.ts";
import type { InventoryBaseline, InventoryComparison } from "../types";

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}

function differenceLabel(status: string): string {
  if (status === "missing") return t("components.evidencecenter.difference.missing");
  if (status === "added") return t("components.evidencecenter.difference.added");
  if (status === "version_changed") return t("components.evidencecenter.difference.version_changed");
  return t("components.evidencecenter.value.unknown");
}

export function SoftwareInventoryComparison() {
  const [baseline, setBaseline] = useState<InventoryBaseline | null>(null);
  const [baselineJson, setBaselineJson] = useState("");
  const [comparison, setComparison] = useState<InventoryComparison | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const native = isTauriRuntime();

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

  const createBaseline = () => run(async () => {
    const nextBaseline = await invoke<InventoryBaseline>("create_inventory_baseline", { machineLabel: window.navigator.userAgent });
    setBaseline(nextBaseline);
    setBaselineJson(JSON.stringify(nextBaseline, null, 2));
    setComparison(null);
    setNotice(t("components.softwareinventory.created", { value0: nextBaseline.entries.length }));
  });

  const compare = () => run(async () => {
    let imported: InventoryBaseline;
    try {
      imported = JSON.parse(baselineJson) as InventoryBaseline;
    } catch {
      throw new Error(t("components.softwareinventory.invalid_json"));
    }
    const nextComparison = await invoke<InventoryComparison>("compare_inventory_baseline", { baseline: imported });
    setComparison(nextComparison);
    setNotice(t("components.softwareinventory.compared", { value0: nextComparison.differences.length }));
  });

  return (
    <section className="page software-inventory-page">
      <div className="section-header inventory-header">
        <div><h1><Database size={20} />{t("components.softwareinventory.title")}</h1><p>{t("components.softwareinventory.subtitle")}</p></div>
      </div>

      <div className="inventory-guide card-surface">
        <ShieldCheck size={18} />
        <div><strong>{t("components.softwareinventory.read_only_title")}</strong><p>{t("components.softwareinventory.read_only_description")}</p></div>
      </div>

      {!native && <div className="inventory-notice info">{t("components.softwareinventory.desktop_only")}</div>}
      {error && <div className="inventory-notice error"><AlertTriangle size={14} />{error}</div>}
      {notice && !error && <div className="inventory-notice success"><CheckCircle2 size={14} />{notice}</div>}

      <div className="inventory-workspace">
        <section className="inventory-source card-surface">
          <header>
            <div><strong>{t("components.softwareinventory.source_title")}</strong><span>{t("components.softwareinventory.source_description")}</span></div>
            <button type="button" className="primary-button" disabled={busy || !native} onClick={() => void createBaseline()}>
              {busy ? <Loader2 className="spinning" size={14} /> : <Database size={14} />}
              {t("components.softwareinventory.create")}
            </button>
          </header>
          <textarea value={baselineJson} onChange={(event) => { setBaselineJson(event.target.value); setComparison(null); }} placeholder={t("components.softwareinventory.placeholder")} />
          <footer>
            <span>{baseline ? t("components.softwareinventory.baseline_summary", { value0: baseline.entries.length, value1: baseline.captured_at }) : t("components.softwareinventory.checksum_hint")}</span>
            <button type="button" className="secondary-button" disabled={busy || !native || !baselineJson.trim()} onClick={() => void compare()}><Upload size={14} />{t("components.softwareinventory.compare")}</button>
          </footer>
        </section>

        <section className="inventory-results card-surface">
          <header><div><strong>{t("components.softwareinventory.results_title")}</strong><span>{t("components.softwareinventory.results_description")}</span></div></header>
          {comparison ? (
            <div className="inventory-differences">
              {comparison.differences.map((item) => (
                <div key={item.key}>
                  <span className={`difference-status ${item.status}`}>{differenceLabel(item.status)}</span>
                  <strong>{item.name}</strong>
                  <small>{item.baseline_version ?? "—"} → {item.current_version ?? "—"}</small>
                </div>
              ))}
              {comparison.differences.length === 0 && <div className="inventory-empty"><CheckCircle2 size={28} /><strong>{t("components.softwareinventory.no_difference")}</strong><span>{t("components.softwareinventory.no_difference_description")}</span></div>}
            </div>
          ) : (
            <div className="inventory-empty"><Database size={28} /><strong>{t("components.softwareinventory.empty_title")}</strong><span>{t("components.softwareinventory.empty_description")}</span></div>
          )}
        </section>
      </div>
    </section>
  );
}
