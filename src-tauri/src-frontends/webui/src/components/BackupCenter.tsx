import { getLanguage, t } from "../i18n/index.ts";
import { useEffect } from "react";
import {
  AlertTriangle,
  Archive,
  CheckCircle2,
  Info,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
} from "lucide-react";
import { useBackupsStore } from "../stores/backups";
import type { BackupSessionInfo, BackupSessionStatus } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

const statusMeta: Record<BackupSessionStatus, { label: string; className: string }> = {
  prepared: { label: t("components.backupcenter.message_001"), className: "prepared" },
  partially_cleaned: { label: t("components.backupcenter.message_002"), className: "ready" },
  restored: { label: t("components.backupcenter.message_003"), className: "restored" },
  restore_failed: { label: t("components.backupcenter.message_004"), className: "failed" },
};

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString(getLanguage());
}

export function BackupCenter() {
  const { sessions, loading, restoringId, error, notice, load, restore, clearMessages } = useBackupsStore();

  useEffect(() => {
    if (isTauri()) void load();
  }, [load]);

  const restorableCount = sessions.reduce((total, session) => total + session.restorable_count, 0);
  const failedCount = sessions.reduce((total, session) => total + session.failed_count, 0);
  const totalBytes = sessions.reduce((total, session) => total + session.bytes, 0);

  const handleRestore = (session: BackupSessionInfo) => {
    if (!window.confirm(t("components.backupcenter.message_005", { value0: session.reason, value1: session.restorable_count }))) return;
    void restore(session.id);
  };

  return (
    <section className="page backup-center">
      <div className="section-header backup-header">
        <div>
          <h1><Archive size={20} />{t("app.message_036")}</h1>
          <p>{t("components.backupcenter.message_007")}</p>
        </div>
        <button type="button" className="icon-button" title={t("components.backupcenter.message_008")} onClick={() => void load()} disabled={!isTauri() || loading || restoringId !== null}>
          <RefreshCw className={loading ? "spinning" : ""} size={17} />
        </button>
      </div>

      {!isTauri() ? (
        <div className="backup-runtime-note card-surface"><Info size={17} /><div><strong>{t("components.backupcenter.message_009")}</strong><p>{t("components.backupcenter.message_010")}</p></div></div>
      ) : (
        <>
          <div className="backup-summary">
            <BackupStat label={t("components.backupcenter.message_011")} value={sessions.length} />
            <BackupStat label={t("components.backupcenter.message_012")} value={restorableCount} tone="success" />
            <BackupStat label={t("components.backupcenter.message_013")} value={failedCount} tone={failedCount > 0 ? "warning" : undefined} />
            <BackupStat label={t("components.backupcenter.message_014")} value={formatBytes(totalBytes)} />
          </div>

          {error && <div className="backup-notice error"><AlertTriangle size={15} /><span>{error}</span><button type="button" onClick={clearMessages}>{t("app.message_031")}</button></div>}
          {notice && !error && <div className="backup-notice success"><CheckCircle2 size={15} /><span>{notice}</span><button type="button" onClick={clearMessages}>{t("app.message_031")}</button></div>}

          <div className="backup-safety-note card-surface"><ShieldCheck size={16} /><span>{t("components.backupcenter.message_017")}</span></div>

          <div className="backup-list card-surface">
            {loading && sessions.length === 0 ? (
              <div className="backup-empty"><Loader2 className="spinning" size={20} />{t("components.backupcenter.message_018")}</div>
            ) : sessions.length === 0 ? (
              <div className="backup-empty"><Archive size={27} /><strong>{t("components.backupcenter.message_019")}</strong><span>{t("components.backupcenter.message_020")}</span></div>
            ) : sessions.map((session) => <BackupCard key={session.id} session={session} busy={restoringId === session.id} onRestore={() => handleRestore(session)} />)}
          </div>
        </>
      )}
    </section>
  );
}

function BackupCard({ session, busy, onRestore }: { session: BackupSessionInfo; busy: boolean; onRestore: () => void }) {
  const status = statusMeta[session.status];
  const canRestore = session.restorable_count > 0 && session.status !== "restored";
  return (
    <article className="backup-card">
      <div className="backup-card-main">
        <div className="backup-card-icon"><Archive size={18} /></div>
        <div className="backup-card-copy"><strong>{session.reason}</strong><small>{formatDate(session.created_at)}  {t("components.backupcenter.message_021")} {session.id.slice(0, 8)}</small></div>
        <span className={`backup-status ${status.className}`}>{status.label}</span>
      </div>
      <div className="backup-card-meta">
        <span>{t("components.backupcenter.message_022")} {session.item_count}  {t("app.message_167")}</span>
        <span>{t("components.backupcenter.message_024")} {session.restorable_count}  {t("app.message_167")}</span>
        <span>{t("components.backupcenter.message_026")} {formatBytes(session.bytes)}</span>
        {session.failed_count > 0 && <span className="backup-failed-count">{t("app.message_108")} {session.failed_count}  {t("app.message_167")}</span>}
      </div>
      <div className="backup-card-action">
        <span>{session.failed_count > 0 ? t("components.backupcenter.message_029") : t("components.backupcenter.message_030")}</span>
        <button type="button" className="secondary-button compact-button" disabled={!canRestore || busy} onClick={onRestore}>
          {busy ? <Loader2 className="spinning" size={13} /> : <RotateCcw size={13} />}{busy ? t("components.backupcenter.message_031") : session.status === "restore_failed" ? t("components.backupcenter.message_032") : t("components.backupcenter.message_033")}
        </button>
      </div>
    </article>
  );
}

function BackupStat({ label, value, tone }: { label: string; value: number | string; tone?: "success" | "warning" }) {
  return <div className="backup-stat card-surface"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>;
}
