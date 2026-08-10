import { t } from "../../i18n/index.ts";
import type { Trace } from "../../types";

interface ResidueReviewProps {
  traces: Trace[];
  selectedIds: Set<string>;
  onToggle: (traceId: string) => void;
  onSkip: () => void;
  onClean: () => void;
}

export function ResidueReview({ traces, selectedIds, onToggle, onSkip, onClean }: ResidueReviewProps) {
  return (
    <section aria-labelledby="residue-review-title" className="space-y-3">
      <div>
        <h3 id="residue-review-title" className="text-base font-semibold text-white">{t("app.message_227")}</h3>
        <p className="text-xs text-slate-400">{t("components.uninstall.residuereview.message_002")}</p>
      </div>
      <div className="max-h-64 space-y-2 overflow-auto rounded-md bg-slate-900 p-3">
        {traces.map((trace) => (
          <label key={trace.id} className="flex cursor-pointer items-start gap-2 text-sm text-slate-200">
            <input type="checkbox" checked={selectedIds.has(trace.id)} onChange={() => onToggle(trace.id)} />
            <span className="min-w-0">
              <span className="block truncate">{trace.description || trace.trace_type}</span>
              <span className="block truncate text-xs text-slate-400">{trace.path}</span>
              <span className="text-xs text-slate-500">{t("components.uninstall.residuereview.message_003")}{trace.confidence} · {trace.size ?? 0} bytes</span>
            </span>
          </label>
        ))}
        {traces.length === 0 && <p className="text-sm text-slate-400">{t("components.uninstall.residuereview.message_004")}</p>}
      </div>
      <div className="flex justify-end gap-2">
        <button className="rounded border border-slate-600 px-3 py-2 text-sm text-slate-200" onClick={onSkip}>{t("app.message_238")}</button>
        <button className="rounded bg-red-600 px-3 py-2 text-sm text-white disabled:opacity-50" disabled={selectedIds.size === 0} onClick={onClean}>{t("app.message_240")}</button>
      </div>
    </section>
  );
}
