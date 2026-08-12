import { ArchiveRestore, Eye, ShieldCheck } from "lucide-react";
import { t } from "../i18n/index.ts";

const rules = [
  { id: "review", icon: Eye, title: "settings.safety.review.title", description: "settings.safety.review.description" },
  { id: "clean", icon: ShieldCheck, title: "settings.safety.clean.title", description: "settings.safety.clean.description" },
  { id: "restore", icon: ArchiveRestore, title: "settings.safety.restore.title", description: "settings.safety.restore.description" },
] as const;

export function CleanupSafetyRules() {
  return (
    <section className="settings-card safety-rules-card card-surface">
      <div className="safety-rules-heading">
        <div><h2>{t("settings.safety.title")}</h2><p>{t("settings.safety.description")}</p></div>
        <span><ShieldCheck size={15} />{t("settings.safety.enforced")}</span>
      </div>
      <div className="safety-rule-list">
        {rules.map((rule, index) => {
          const Icon = rule.icon;
          return (
            <div key={rule.id}>
              <span className="safety-rule-index">{index + 1}</span>
              <Icon size={18} />
              <div><strong>{t(rule.title)}</strong><p>{t(rule.description)}</p></div>
            </div>
          );
        })}
      </div>
      <p className="settings-note"><ShieldCheck size={14} />{t("settings.safety.note")}</p>
    </section>
  );
}
