import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, CheckCircle2, FileSearch, Loader2, ShieldCheck, X } from "lucide-react";
import { useState } from "react";
import { t, type TranslationKey } from "../i18n/index.ts";
import type {
  EvidenceConfidence,
  InstalledProgram,
  ReconstructedEvidencePacket,
} from "../types";

const categoryKeys: Record<string, TranslationKey> = {
  uninstall_registry: "components.evidencecenter.category.uninstall_record",
  install_directory: "components.evidencecenter.category.install_folder",
  registry: "components.evidencecenter.category.system_setting",
  filesystem: "components.evidencecenter.category.file",
  appdata: "components.evidencecenter.category.personal_data",
  scheduled_task: "components.evidencecenter.category.scheduled_start",
  service: "components.evidencecenter.category.background_service",
  driver: "components.evidencecenter.category.driver",
};

const sourceKeys: Record<string, TranslationKey> = {
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

function translatedValue(value: string, keys: Record<string, TranslationKey>): string {
  const key = keys[value];
  return key ? t(key) : t("components.evidencecenter.value.unknown");
}

function signatureLabel(status: string): string {
  if (status === "valid") return t("components.evidencecenter.signature.valid");
  if (status === "unsigned") return t("components.evidencecenter.signature.unsigned");
  return t("components.evidencecenter.signature.unknown");
}

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}

export function SoftwareRecordDialog({
  program,
  onClose,
}: {
  program: InstalledProgram;
  onClose: () => void;
}) {
  const [packet, setPacket] = useState<ReconstructedEvidencePacket | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const inspect = async () => {
    setBusy(true);
    setError(null);
    try {
      setPacket(await invoke<ReconstructedEvidencePacket>("reconstruct_installation", { program }));
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section
        className="software-record-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="software-record-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <span className="record-modal-icon"><FileSearch size={20} /></span>
            <span>
              <h2 id="software-record-title">{t("components.softwarerecords.title", { value0: program.name })}</h2>
              <p>{t("components.softwarerecords.subtitle")}</p>
            </span>
          </div>
          <button type="button" aria-label={t("components.softwarerecords.close")} onClick={onClose}><X size={17} /></button>
        </header>

        <div className="record-explanation">
          <ShieldCheck size={16} />
          <div>
            <strong>{t("components.softwarerecords.read_only_title")}</strong>
            <span>{t("components.softwarerecords.read_only_description")}</span>
          </div>
          <button type="button" className="primary-button" disabled={busy} onClick={() => void inspect()}>
            {busy ? <Loader2 className="spinning" size={14} /> : <FileSearch size={14} />}
            {packet ? t("components.softwarerecords.recheck") : t("components.softwarerecords.start")}
          </button>
        </div>

        {error && <div className="record-notice error"><AlertTriangle size={15} />{error}</div>}

        {packet ? (
          <>
            <div className="record-summary">
              <div><span>{t("app.message_054")}</span><strong>{packet.vendor ?? t("app.message_296")}</strong></div>
              <div><span>{t("components.evidencecenter.message_026")}</span><strong>{signatureLabel(packet.signature_status)}</strong></div>
              <div><span>{t("components.softwarerecords.found")}</span><strong>{packet.evidence.length}</strong></div>
              <div><span>{t("components.softwarerecords.protected")}</span><strong>{packet.evidence.filter((item) => !item.destructive_eligible).length}</strong></div>
            </div>
            <div className="record-notice warning"><AlertTriangle size={15} />{t("components.softwarerecords.inference_notice")}</div>
            <div className="record-table">
              <div className="record-row head">
                <span>{t("components.evidencecenter.message_028")}</span>
                <span>{t("components.evidencecenter.message_029")}</span>
                <span>{t("components.evidencecenter.message_030")}</span>
                <span>{t("app.message_248")}</span>
                <span>{t("components.evidencecenter.message_032")}</span>
              </div>
              {packet.evidence.map((item) => (
                <div className="record-row" key={item.id}>
                  <span>{translatedValue(item.category, categoryKeys)}</span>
                  <code title={item.target}>{item.target}</code>
                  <span>{translatedValue(item.source, sourceKeys)}</span>
                  <span className={`confidence ${item.confidence}`}>{t(confidenceKeys[item.confidence])}</span>
                  <span>{item.destructive_eligible ? t("components.softwarerecords.reviewable") : t("components.evidencecenter.message_034")}</span>
                </div>
              ))}
              {packet.evidence.length === 0 && <div className="record-empty"><CheckCircle2 size={24} />{t("components.softwarerecords.empty")}</div>}
            </div>
          </>
        ) : (
          <div className="record-empty">
            <FileSearch size={30} />
            <strong>{t("components.softwarerecords.empty_title")}</strong>
            <span>{t("components.softwarerecords.empty_description")}</span>
          </div>
        )}

        <footer>
          <span>{t("components.softwarerecords.footer")}</span>
          <button type="button" className="secondary-button" onClick={onClose}>{t("app.message_031")}</button>
        </footer>
      </section>
    </div>
  );
}
