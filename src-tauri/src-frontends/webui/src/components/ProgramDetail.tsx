import { t } from "../i18n/index.ts";
import {
  Trash2,
  SearchCode,
  FolderOpen,
    ArrowLeft,
  Package,
} from "lucide-react";
import { useProgramsStore } from "../stores/programs";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader } from "./ui/card";
import { Badge } from "./ui/badge";
import { formatBytes, formatSource } from "../lib/utils";
import { getProgramIconSrc } from "../lib/icon";
import { formatWindowsDate } from "../lib/date";

export function ProgramDetail() {
  const { selectedProgram, setViewMode, scanTraces, selectProgram } =
    useProgramsStore();

  if (!selectedProgram) return null;
  const p = selectedProgram;
  const iconSrc = getProgramIconSrc(p);

  return (
    <div className="flex min-w-0 flex-col h-full overflow-y-auto overflow-x-hidden">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700">
        <button
          onClick={() => selectProgram(null)}
          className="text-slate-400 hover:text-white transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
        </button>
        <h2 className="text-lg font-semibold text-white">{p.name}</h2>
        <Badge
          variant={
            p.install_source === "store"
              ? "success"
              : p.install_source === "msi"
                ? "warning"
                : "default"
          }
        >
          {formatSource(p.install_source)}
        </Badge>
      </div>

      <div className="flex-1 min-w-0 p-4 space-y-4">
        {/* 图标与基本信息 */}
        <Card className="min-w-0">
          <CardContent className="flex items-center gap-4 py-4">
            {iconSrc ? (
              <img
                src={iconSrc}
                alt=""
                className="h-12 w-12 rounded"
              />
            ) : (
              <div className="flex h-12 w-12 items-center justify-center rounded bg-slate-700">
                <Package className="h-6 w-6 text-slate-400" />
              </div>
            )}
            <div>
              <h3 className="text-base font-semibold text-white">{p.name}</h3>
              <p className="text-sm text-slate-400">
                {p.publisher ?? t("app.message_016")} {p.version && `· v${p.version}`}
              </p>
            </div>
          </CardContent>
        </Card>

        {/* 详细信息 */}
        <Card>
          <CardHeader>
            <span className="text-sm font-medium text-slate-300">{t("app.message_068")}</span>
          </CardHeader>
          <CardContent className="min-w-0 space-y-2 text-sm">
            <InfoRow label={t("app.message_297")} value={p.id} />
            <InfoRow label={t("app.message_300")} value={formatSource(p.install_source)} />
            <InfoRow label={t("app.message_301")} value={p.uninstall_kind} />
            <InfoRow label={t("app.message_304")} value={p.install_location} />
            <InfoRow label={t("app.message_056")} value={formatWindowsDate(p.install_date) ?? t("app.message_067")} />
            <InfoRow label={t("app.message_055")} value={p.size ? formatBytes(p.size) : null} />
            <InfoRow label={t("components.programdetail.message_009")} value={p.estimated_size ? formatBytes(p.estimated_size) : null} />
            <InfoRow label={t("app.message_305")} value={p.uninstall_string} mono />
            <InfoRow label={t("app.message_306")} value={p.quiet_uninstall_string} mono />
            <InfoRow label={t("app.message_307")} value={p.uninstall_registry_key_path} mono />
          </CardContent>
        </Card>

        {/* 操作按钮 */}
        <div className="flex gap-3">
          <Button
            variant="destructive"
            onClick={() => setViewMode("uninstall")}
          >
            <Trash2 className="h-4 w-4" />

            {t("components.programdetail.message_013")}
          </Button>
          <Button
            variant="secondary"
            onClick={() => scanTraces(p.name)}
          >
            <SearchCode className="h-4 w-4" />

            {t("app.message_069")}
          </Button>
          {p.install_location && (
            <a
              href={`file:///${p.install_location}`}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 rounded-md px-4 py-2 text-sm font-medium text-slate-300 hover:bg-slate-700 hover:text-white transition-colors"
            >
              <FolderOpen className="h-4 w-4" />

              {t("components.programdetail.message_015")}
            </a>
          )}
        </div>
      </div>
    </div>
  );
}

function InfoRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string | null | undefined;
  mono?: boolean;
}) {
  if (!value) return null;
  return (
    <div className="flex min-w-0 gap-2">
      <span className="w-24 shrink-0 text-slate-500">{label}</span>
      <span
        className={
          mono
            ? "min-w-0 whitespace-pre-wrap break-all font-mono text-xs text-slate-300"
            : "min-w-0 whitespace-pre-wrap break-words text-slate-300"
        }
      >
        {value}
      </span>
    </div>
  );
}

