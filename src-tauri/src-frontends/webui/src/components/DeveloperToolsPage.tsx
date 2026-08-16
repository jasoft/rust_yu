import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  FileCode2,
  Loader2,
  PackagePlus,
  RefreshCw,
  ShieldCheck,
  X,
} from "lucide-react";
import { t, type TranslationKey } from "../i18n";
import {
  getDeveloperCommandErrorCode,
  installDeveloperFixtures,
  listenToFixtureInstallProgress,
  listDeveloperFixtures,
  type DeveloperFixture,
  type DeveloperFixtureInstallProgress,
  type FixtureInstallResult,
} from "../lib/developerTools";
import { useProgramsStore } from "../stores/programs";

const fixtureNameKeys: Record<string, TranslationKey> = {
  "xplorer-msi": "developer.fixture.xplorer.name",
  "legacy-inno": "developer.fixture.legacy.name",
};

const fixtureDescriptionKeys: Record<string, TranslationKey> = {
  "xplorer-msi": "developer.fixture.xplorer.description",
  "legacy-inno": "developer.fixture.legacy.description",
};

const fixtureKindKeys: Record<DeveloperFixture["kind"], TranslationKey> = {
  msi: "developer.fixture.kind.msi",
  inno: "developer.fixture.kind.inno",
};

const commandErrorKeys: Record<string, TranslationKey> = {
  admin_required: "developer.error.admin_required",
  developer_fixture_confirmation_required: "developer.error.confirmation_required",
  developer_fixture_selection_required: "developer.error.selection_required",
  developer_fixture_root_missing: "developer.error.root_missing",
  developer_fixture_missing: "developer.error.fixture_missing",
  developer_fixture_install_busy: "developer.error.busy",
  developer_fixture_timed_out: "developer.error.timeout",
};

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function fixtureName(id: string): string {
  return t(fixtureNameKeys[id] ?? "developer.fixture.unknown");
}

function fixtureDescription(id: string): string {
  return t(fixtureDescriptionKeys[id] ?? "developer.fixture.unknown_description");
}

function formatFixtureSize(bytes: number | null): string {
  if (bytes === null) return t("developer.fixture.size_unknown");
  const megabytes = bytes / 1024 / 1024;
  return t("developer.fixture.size", {
    value0: new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(megabytes),
  });
}

function commandErrorMessage(error: unknown): string {
  const code = getDeveloperCommandErrorCode(error);
  return t((code && commandErrorKeys[code]) || "developer.error.generic");
}

export function DeveloperToolsPage() {
  const nativeRuntime = isTauriRuntime();
  const reloadPrograms = useProgramsStore((state) => state.reloadPrograms);
  const [fixtures, setFixtures] = useState<DeveloperFixture[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(nativeRuntime);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<DeveloperFixtureInstallProgress | null>(null);
  const [results, setResults] = useState<FixtureInstallResult[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pendingIds, setPendingIds] = useState<string[] | null>(null);

  const availableIds = useMemo(
    () => fixtures.filter((fixture) => fixture.available).map((fixture) => fixture.id),
    [fixtures],
  );

  useEffect(() => {
    if (!nativeRuntime) return;
    let active = true;
    void listDeveloperFixtures()
      .then((catalog) => {
        if (!active) return;
        setFixtures(catalog);
        setSelectedIds(new Set(catalog.filter((fixture) => fixture.available).map((fixture) => fixture.id)));
      })
      .catch((reason: unknown) => active && setError(commandErrorMessage(reason)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [nativeRuntime]);

  useEffect(() => {
    if (!nativeRuntime) return;
    let unlisten: (() => void) | undefined;
    void listenToFixtureInstallProgress(setProgress).then((dispose) => {
      unlisten = dispose;
    });
    return () => unlisten?.();
  }, [nativeRuntime]);

  const toggleFixture = (fixture: DeveloperFixture) => {
    if (!fixture.available || installing) return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(fixture.id)) next.delete(fixture.id);
      else next.add(fixture.id);
      return next;
    });
  };

  const requestInstall = (ids: string[]) => {
    if (ids.length === 0 || installing) return;
    setPendingIds(ids);
  };

  const confirmInstall = async () => {
    if (!pendingIds) return;
    const fixtureIds = pendingIds;
    setPendingIds(null);
    setInstalling(true);
    setError(null);
    setResults([]);
    setProgress({ fixture_id: fixtureIds[0], index: 1, total: fixtureIds.length, phase: "installing", status: null });
    try {
      const response = await installDeveloperFixtures(fixtureIds);
      setResults(response.results);
      await reloadPrograms({ refresh: true });
    } catch (reason) {
      setError(commandErrorMessage(reason));
    } finally {
      setInstalling(false);
    }
  };

  const progressPercent = progress
    ? Math.round((((progress.index - 1) + (progress.phase === "completed" ? 1 : 0.2)) / progress.total) * 100)
    : 0;

  return (
    <div className="page developer-page" data-testid="developer-tools">
      <div className="section-header developer-header">
        <div>
          <h1><FileCode2 size={22} />{t("developer.title")}</h1>
          <p>{t("developer.subtitle")}</p>
        </div>
        <span className="developer-badge">{t("developer.badge")}</span>
      </div>

      <section className="developer-warning card-surface">
        <AlertTriangle size={20} />
        <div><strong>{t("developer.warning.title")}</strong><p>{t("developer.warning.description")}</p></div>
      </section>

      {!nativeRuntime ? (
        <section className="developer-runtime card-surface">
          <ShieldCheck size={22} />
          <div><strong>{t("developer.runtime.title")}</strong><p>{t("developer.runtime.description")}</p></div>
        </section>
      ) : (
        <section className="developer-fixtures card-surface">
          <header>
            <div><h2>{t("developer.fixtures.title")}</h2><p>{t("developer.fixtures.description")}</p></div>
            <span>{t("developer.fixtures.available", { value0: availableIds.length })}</span>
          </header>

          {loading ? (
            <div className="developer-loading"><Loader2 className="spin" size={22} />{t("developer.fixtures.loading")}</div>
          ) : (
            <div className="developer-fixture-list">
              {fixtures.map((fixture) => {
                const selected = selectedIds.has(fixture.id);
                const result = results.find((item) => item.fixture_id === fixture.id);
                return (
                  <button
                    type="button"
                    key={fixture.id}
                    data-testid="developer-fixture"
                    data-fixture-id={fixture.id}
                    className={`developer-fixture ${selected ? "selected" : ""}`}
                    disabled={!fixture.available || installing}
                    aria-pressed={selected}
                    onClick={() => toggleFixture(fixture)}
                  >
                    <span className="developer-fixture-check">{selected && <Check size={14} />}</span>
                    <PackagePlus size={22} />
                    <span className="developer-fixture-copy">
                      <strong>{fixtureName(fixture.id)}</strong>
                      <small>{fixtureDescription(fixture.id)}</small>
                      <code>{fixture.path}</code>
                    </span>
                    <span className="developer-fixture-meta">
                      <small>{t(fixtureKindKeys[fixture.kind])} · {formatFixtureSize(fixture.size)}</small>
                      {!fixture.available && <em>{t("developer.fixture.missing")}</em>}
                      {result && <span data-testid="developer-fixture-result" data-status={result.status}><FixtureResult result={result} /></span>}
                    </span>
                  </button>
                );
              })}
            </div>
          )}

          {error && <div className="developer-notice error"><AlertTriangle size={16} />{error}</div>}
          {installing && progress && (
            <div className="developer-progress">
              <div><span>{t("developer.progress.installing", { value0: fixtureName(progress.fixture_id), value1: progress.index, value2: progress.total })}</span><strong>{progressPercent}%</strong></div>
              <span><i style={{ width: `${progressPercent}%` }} /></span>
            </div>
          )}

          <footer>
            <span>{selectedIds.size === 0 ? t("developer.selection.empty") : t("developer.selection.count", { value0: selectedIds.size })}</span>
            <div>
              <button data-testid="developer-install-all" className="secondary-button" disabled={installing || availableIds.length === 0} onClick={() => requestInstall(availableIds)}>
                <PackagePlus size={15} />{t("developer.action.install_all")}
              </button>
              <button className="primary-button" disabled={installing || selectedIds.size === 0} onClick={() => requestInstall([...selectedIds])}>
                {installing ? <Loader2 className="spin" size={15} /> : <RefreshCw size={15} />}{installing ? t("developer.action.installing") : t("developer.action.install_selected")}
              </button>
            </div>
          </footer>
        </section>
      )}

      {pendingIds && (
        <div className="modal-backdrop" onMouseDown={() => setPendingIds(null)}>
          <section className="developer-confirm card-surface" onMouseDown={(event) => event.stopPropagation()}>
            <header><div><AlertTriangle size={20} /><h2>{t("developer.confirm.title")}</h2></div><button aria-label={t("common.action.close")} onClick={() => setPendingIds(null)}><X size={17} /></button></header>
            <p>{t("developer.confirm.description", { value0: pendingIds.length })}</p>
            <ul>{pendingIds.map((id) => <li key={id}><PackagePlus size={15} />{fixtureName(id)}</li>)}</ul>
            <footer><button className="secondary-button" onClick={() => setPendingIds(null)}>{t("developer.confirm.cancel")}</button><button data-testid="developer-confirm-install" className="primary-button" onClick={() => void confirmInstall()}>{t("developer.confirm.submit")}</button></footer>
          </section>
        </div>
      )}
    </div>
  );
}

function FixtureResult({ result }: { result: FixtureInstallResult }) {
  if (result.status === "installed") return <em className="success">{t("developer.result.installed")}</em>;
  if (result.status === "reboot_required") return <em className="warning">{t("developer.result.reboot_required")}</em>;
  return (
    <em className="error">
      {result.exit_code === null
        ? t("developer.result.failed")
        : t("developer.result.failed_exit", { value0: result.exit_code })}
    </em>
  );
}
