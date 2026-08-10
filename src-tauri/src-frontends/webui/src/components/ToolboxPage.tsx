import { t } from "../i18n/index.ts";
import { useMemo, useState } from "react";
import {
  Archive,
  Search,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import { filterToolboxItems, toolboxItems, type ToolboxItem, type ToolboxTarget } from "../lib/toolbox";

export function ToolboxPage({ onNavigate }: { onNavigate: (target: ToolboxTarget) => void }) {
  const [query, setQuery] = useState("");
  const visibleItems = useMemo(() => filterToolboxItems(query), [query]);

  return (
    <section className="page toolbox-page">
      <div className="section-header toolbox-header">
        <div>
          <h1><Wrench size={20} />{t("app.message_042")}</h1>
          <p>{t("components.toolboxpage.message_002")}</p>
        </div>
        <span className="toolbox-count">{visibleItems.length} / {toolboxItems.length}  {t("components.toolboxpage.message_003")}</span>
      </div>

      <div className="toolbox-safety card-surface">
        <ShieldCheck size={18} />
        <div>
          <strong>{t("components.toolboxpage.message_004")}</strong>
          <p>{t("components.toolboxpage.message_005")}</p>
        </div>
      </div>

      <label className="toolbox-search card-surface">
        <Search size={16} />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("components.toolboxpage.message_006")} aria-label={t("components.toolboxpage.message_007")} />
        {query && <button type="button" onClick={() => setQuery("")} aria-label={t("components.toolboxpage.message_008")}><span>{t("components.toolboxpage.message_009")}</span></button>}
      </label>

      {visibleItems.length > 0 ? (
        <div className="toolbox-grid">
          {visibleItems.map((item) => <ToolboxCard key={item.id} item={item} onOpen={() => onNavigate(item.id)} />)}
        </div>
      ) : (
        <div className="toolbox-empty card-surface">
          <Search size={23} />
          <strong>{t("components.toolboxpage.message_010")}</strong>
          <span>{t("components.toolboxpage.message_011")}</span>
          <button type="button" className="secondary-button" onClick={() => setQuery("")}>{t("components.toolboxpage.message_012")}</button>
        </div>
      )}

      <div className="toolbox-footer"><Archive size={14} /><span>{t("components.toolboxpage.message_013")}</span></div>
    </section>
  );
}

function ToolboxCard({ item, onOpen }: { item: ToolboxItem; onOpen: () => void }) {
  const Icon = item.icon;
  return (
    <button type="button" className="toolbox-card card-surface" onClick={onOpen}>
      <span className={`toolbox-icon ${item.tone}`}><Icon size={20} /></span>
      <span className="toolbox-card-copy"><strong>{item.title}</strong><span>{item.description}</span><small>{item.detail}</small></span>
      <span className="toolbox-card-arrow" aria-hidden="true">›</span>
    </button>
  );
}
