import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  AlertTriangle,
  Archive,
  Check,
  CheckCircle2,
  Database,
  FileCode2,
  FolderOpen,
  Info,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { t } from "../i18n/index.ts";
import { formatBytes } from "../lib/utils";
import { groupTraces, selectedTraceBytes, summarizeCleanResults } from "../lib/traceManager";
import { useProgramsStore } from "../stores/programs";
import type { Trace } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function TracePanel() {
  const programs = useProgramsStore((state) => state.programs);
  const loadingPrograms = useProgramsStore((state) => state.loading);
  const error = useProgramsStore((state) => state.error);
  const traces = useProgramsStore((state) => state.traces);
  const tracesLoading = useProgramsStore((state) => state.tracesLoading);
  const selectedTraces = useProgramsStore((state) => state.selectedTraces);
  const cleanResults = useProgramsStore((state) => state.cleanResults);
  const scanTraces = useProgramsStore((state) => state.scanTraces);
  const toggleTrace = useProgramsStore((state) => state.toggleTrace);
  const cleanTraces = useProgramsStore((state) => state.cleanTraces);
  const resetTraces = useProgramsStore((state) => state.resetTraces);
  const [selectedProgramId, setSelectedProgramId] = useState("");
  const [scanAttempted, setScanAttempted] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [cleaning, setCleaning] = useState(false);

  useEffect(() => {
    if (programs.length === 0) return;
    if (programs.some((program) => program.id === selectedProgramId)) return;
    setSelectedProgramId(programs[0].id);
  }, [programs, selectedProgramId]);

  const selectedProgram = useMemo(
    () => programs.find((program) => program.id === selectedProgramId) ?? null,
    [programs, selectedProgramId],
  );
  const groups = useMemo(() => groupTraces(traces), [traces]);
  const selectedBytes = selectedTraceBytes(traces, selectedTraces);
  const resultSummary = summarizeCleanResults(cleanResults);
  const hasResults = cleanResults.length > 0;

  const selectProgram = (programId: string) => {
    setSelectedProgramId(programId);
    setScanAttempted(false);
    setConfirmOpen(false);
    resetTraces();
  };

  const scan = async () => {
    if (!isTauri() || !selectedProgram || tracesLoading || cleaning) return;
    setScanAttempted(true);
    setConfirmOpen(false);
    await scanTraces(selectedProgram.name, selectedProgram);
  };

  const clean = async () => {
    setConfirmOpen(false);
    setCleaning(true);
    try {
      await cleanTraces(true);
    } finally {
      setCleaning(false);
    }
  };

  return (
    <section className="page trace-manager-page">
      <div className="section-header trace-manager-header">
        <div>
          <h1><Archive size={20} />{t("app.message_037")}</h1>
          <p>{t("traces.subtitle")}</p>
        </div>
        <button
          type="button"
          className="icon-button"
          title={t("traces.action.rescan")}
          disabled={!isTauri() || !selectedProgram || tracesLoading || cleaning}
          onClick={() => void scan()}
        >
          <RefreshCw size={17} className={tracesLoading ? "spinning" : ""} />
        </button>
      </div>

      <div className="trace-safety card-surface">
        <ShieldCheck size={17} />
        <div><strong>{t("traces.safety.title")}</strong><p>{t("traces.safety.detail")}</p></div>
      </div>

      {!isTauri() && (
        <div className="trace-runtime-note card-surface">
          <Info size={17} />
          <div><strong>{t("traces.runtime.title")}</strong><p>{t("traces.runtime.detail")}</p></div>
        </div>
      )}

      <div className="trace-program-bar card-surface">
        <label>
          <span>{t("traces.program.label")}</span>
          <select
            aria-label={t("traces.program.label")}
            value={selectedProgramId}
            disabled={!isTauri() || loadingPrograms || programs.length === 0 || tracesLoading || cleaning}
            onChange={(event) => selectProgram(event.target.value)}
          >
            {programs.length === 0 && <option value="">{loadingPrograms ? t("traces.program.loading") : t("traces.program.empty")}</option>}
            {programs.map((program) => (
              <option value={program.id} key={program.id}>
                {t("traces.program.option", { value0: program.name, value1: program.publisher ?? t("app.message_016") })}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="primary-button"
          disabled={!isTauri() || !selectedProgram || tracesLoading || cleaning}
          onClick={() => void scan()}
        >
          {tracesLoading ? <Loader2 size={14} className="spinning" /> : <Archive size={14} />}
          {tracesLoading ? t("traces.action.scanning") : t("traces.action.scan")}
        </button>
      </div>

      {error && <div className="trace-notice error" role="alert"><AlertTriangle size={15} /><span>{error}</span></div>}
      {hasResults && (
        <div className={`trace-notice ${resultSummary.failed > 0 ? "warning" : "success"}`} role="status">
          {resultSummary.failed > 0 ? <AlertTriangle size={15} /> : <CheckCircle2 size={15} />}
          <span>{t("traces.result.summary", { value0: resultSummary.succeeded, value1: resultSummary.failed, value2: formatBytes(resultSummary.bytesFreed) })}</span>
        </div>
      )}

      <div className="trace-summary">
        <TraceStat label={t("traces.summary.found")} value={traces.length} />
        <TraceStat label={t("traces.summary.selected")} value={selectedTraces.size} tone={selectedTraces.size > 0 ? "warning" : undefined} />
        <TraceStat label={t("traces.summary.size")} value={formatBytes(selectedBytes)} />
        <TraceStat label={t("traces.summary.protected")} value={traces.filter((trace) => trace.is_critical).length} tone="safe" />
      </div>

      <div className="trace-workspace card-surface">
        {tracesLoading ? (
          <TraceEmpty icon={<Loader2 className="spinning" size={24} />} title={t("traces.empty.scanning")} detail={t("traces.empty.scanning_detail")} />
        ) : !scanAttempted ? (
          <TraceEmpty icon={<Archive size={27} />} title={t("traces.empty.ready")} detail={t("traces.empty.ready_detail")} />
        ) : traces.length === 0 ? (
          <TraceEmpty icon={<CheckCircle2 size={27} />} title={t("traces.empty.clean")} detail={t("traces.empty.clean_detail")} />
        ) : (
          <div className="trace-manager-groups">
            <TraceGroup title={t("traces.group.files", { value0: groups.files.length })} icon={<FolderOpen size={15} />} traces={groups.files} selectedIds={selectedTraces} disabled={hasResults || cleaning} onToggle={toggleTrace} />
            <TraceGroup title={t("traces.group.registry", { value0: groups.registry.length })} icon={<Database size={15} />} traces={groups.registry} selectedIds={selectedTraces} disabled={hasResults || cleaning} onToggle={toggleTrace} />
            <TraceGroup title={t("traces.group.system", { value0: groups.system.length })} icon={<ShieldCheck size={15} />} traces={groups.system} selectedIds={selectedTraces} disabled={hasResults || cleaning} onToggle={toggleTrace} />
          </div>
        )}
      </div>

      <footer className="trace-manager-footer">
        <span>{selectedTraces.size === 0 ? t("traces.footer.none") : t("traces.footer.selected", { value0: selectedTraces.size, value1: formatBytes(selectedBytes) })}</span>
        <button
          type="button"
          className="danger-button"
          disabled={selectedTraces.size === 0 || hasResults || cleaning}
          onClick={() => setConfirmOpen(true)}
        >
          {cleaning ? <Loader2 size={15} className="spinning" /> : <Trash2 size={15} />}
          {cleaning ? t("traces.action.cleaning") : t("traces.action.clean")}
        </button>
      </footer>

      {confirmOpen && (
        <div className="modal-backdrop">
          <div className="safety-modal trace-confirm-modal">
            <span className="modal-icon"><AlertTriangle size={24} /></span>
            <h2>{t("traces.confirm.title", { value0: selectedTraces.size })}</h2>
            <p>{t("traces.confirm.detail", { value0: formatBytes(selectedBytes) })}</p>
            <div>
              <button type="button" className="secondary-button" onClick={() => setConfirmOpen(false)}>{t("app.message_089")}</button>
              <button type="button" className="danger-button" onClick={() => void clean()}>{t("traces.confirm.submit")}</button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function TraceGroup({
  title,
  icon,
  traces,
  selectedIds,
  disabled,
  onToggle,
}: {
  title: string;
  icon: ReactNode;
  traces: Trace[];
  selectedIds: ReadonlySet<string>;
  disabled: boolean;
  onToggle: (traceId: string) => void;
}) {
  if (traces.length === 0) return null;
  return (
    <section className="trace-manager-group">
      <h2>{icon}{title}</h2>
      {traces.map((trace) => (
        <label className={`trace-manager-row${trace.is_critical ? " protected" : ""}`} key={trace.id}>
          <input
            type="checkbox"
            checked={selectedIds.has(trace.id)}
            disabled={disabled || trace.is_critical}
            aria-label={t("traces.item.select", { value0: trace.path })}
            onChange={() => onToggle(trace.id)}
          />
          <span className="trace-manager-check"><Check size={12} /></span>
          <span className="trace-manager-icon"><TraceTypeIcon trace={trace} /></span>
          <span className="trace-manager-copy"><strong>{trace.description?.trim() || traceTypeLabel(trace)}</strong><small>{trace.path}</small></span>
          <span className="trace-manager-size">{formatBytes(trace.size)}</span>
          {trace.is_critical
            ? <span className="trace-manager-confidence protected">{t("traces.confidence.protected")}</span>
            : <span className={`trace-manager-confidence ${trace.confidence}`}>{confidenceLabel(trace.confidence)}</span>}
        </label>
      ))}
    </section>
  );
}

function TraceTypeIcon({ trace }: { trace: Trace }) {
  if (trace.trace_type === "registry_key" || trace.trace_type === "registry_value") return <Database size={14} />;
  if (trace.trace_type === "scheduled_task" || trace.trace_type === "service" || trace.trace_type === "driver") return <ShieldCheck size={14} />;
  return <FileCode2 size={14} />;
}

function traceTypeLabel(trace: Trace): string {
  return {
    registry_key: t("traces.type.registry_key"),
    registry_value: t("traces.type.registry_value"),
    file: t("traces.type.file"),
    appdata: t("traces.type.appdata"),
    shortcut: t("traces.type.shortcut"),
    scheduled_task: t("traces.type.scheduled_task"),
    service: t("traces.type.service"),
    driver: t("traces.type.driver"),
  }[trace.trace_type];
}

function confidenceLabel(confidence: Trace["confidence"]): string {
  return {
    high: t("traces.confidence.high"),
    medium: t("traces.confidence.medium"),
    low: t("traces.confidence.low"),
  }[confidence];
}

function TraceStat({ label, value, tone }: { label: string; value: number | string; tone?: "warning" | "safe" }) {
  return <div className="trace-stat card-surface"><span>{label}</span><strong className={tone ?? ""}>{value}</strong></div>;
}

function TraceEmpty({ icon, title, detail }: { icon: ReactNode; title: string; detail: string }) {
  return <div className="trace-manager-empty">{icon}<strong>{title}</strong><span>{detail}</span></div>;
}
