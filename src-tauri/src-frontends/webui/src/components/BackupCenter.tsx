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
  prepared: { label: "已备份，未清理", className: "prepared" },
  partially_cleaned: { label: "已尝试清理，可恢复", className: "ready" },
  restored: { label: "已恢复", className: "restored" },
  restore_failed: { label: "恢复失败，可重试", className: "failed" },
};

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString("zh-CN");
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
    if (!window.confirm(`将恢复“${session.reason}”中的 ${session.restorable_count} 个项目。\n\n恢复不会覆盖清理后重新出现的文件、目录或注册表内容，是否继续？`)) return;
    void restore(session.id);
  };

  return (
    <section className="page backup-center">
      <div className="section-header backup-header">
        <div>
          <h1><Archive size={20} />备份与恢复</h1>
          <p>每次文件或注册表清理前自动保存快照；恢复时校验内容并拒绝覆盖新数据。</p>
        </div>
        <button type="button" className="icon-button" title="刷新备份会话" onClick={() => void load()} disabled={!isTauri() || loading || restoringId !== null}>
          <RefreshCw className={loading ? "spinning" : ""} size={17} />
        </button>
      </div>

      {!isTauri() ? (
        <div className="backup-runtime-note card-surface"><Info size={17} /><div><strong>请在 Rust Yu 桌面应用中使用恢复中心</strong><p>浏览器预览不会读取本机备份目录，也不会执行恢复。</p></div></div>
      ) : (
        <>
          <div className="backup-summary">
            <BackupStat label="备份会话" value={sessions.length} />
            <BackupStat label="可恢复项目" value={restorableCount} tone="success" />
            <BackupStat label="失败待处理" value={failedCount} tone={failedCount > 0 ? "warning" : undefined} />
            <BackupStat label="保护数据量" value={formatBytes(totalBytes)} />
          </div>

          {error && <div className="backup-notice error"><AlertTriangle size={15} /><span>{error}</span><button type="button" onClick={clearMessages}>关闭</button></div>}
          {notice && !error && <div className="backup-notice success"><CheckCircle2 size={15} /><span>{notice}</span><button type="button" onClick={clearMessages}>关闭</button></div>}

          <div className="backup-safety-note card-surface"><ShieldCheck size={16} /><span>恢复采用“只创建、不覆盖”策略。目标已被重新创建、内容发生变化或父目录包含链接时，该项目会保留失败状态并允许稍后重试。</span></div>

          <div className="backup-list card-surface">
            {loading && sessions.length === 0 ? (
              <div className="backup-empty"><Loader2 className="spinning" size={20} />正在读取备份会话…</div>
            ) : sessions.length === 0 ? (
              <div className="backup-empty"><Archive size={27} /><strong>还没有备份会话</strong><span>清理软件残留时，文件和注册表项目会在删除前自动出现在这里。</span></div>
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
        <div className="backup-card-copy"><strong>{session.reason}</strong><small>{formatDate(session.created_at)} · 会话 {session.id.slice(0, 8)}</small></div>
        <span className={`backup-status ${status.className}`}>{status.label}</span>
      </div>
      <div className="backup-card-meta">
        <span>保护 {session.item_count} 项</span>
        <span>可恢复 {session.restorable_count} 项</span>
        <span>数据 {formatBytes(session.bytes)}</span>
        {session.failed_count > 0 && <span className="backup-failed-count">失败 {session.failed_count} 项</span>}
      </div>
      <div className="backup-card-action">
        <span>{session.failed_count > 0 ? "失败项目不会被强行覆盖，可调整目标后重试。" : "恢复前仍会重新检查目标，避免误覆盖。"}</span>
        <button type="button" className="secondary-button compact-button" disabled={!canRestore || busy} onClick={onRestore}>
          {busy ? <Loader2 className="spinning" size={13} /> : <RotateCcw size={13} />}{busy ? "恢复中…" : session.status === "restore_failed" ? "重试恢复" : "恢复"}
        </button>
      </div>
    </article>
  );
}

function BackupStat({ label, value, tone }: { label: string; value: number | string; tone?: "success" | "warning" }) {
  return <div className="backup-stat card-surface"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>;
}
