import { Cpu, HardDrive, Info, ShieldCheck, Sparkles } from "lucide-react";
import type { ReactNode } from "react";
import { t } from "../i18n/index.ts";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function AboutPage() {
  return (
    <section className="page about-page">
      <div className="section-header about-header">
        <div>
          <h1><Info size={20} />{t("app.message_044")}</h1>
          <p>{t("about.subtitle")}</p>
        </div>
      </div>

      <div className="about-hero card-surface">
        <span className="about-mark"><ShieldCheck size={28} /></span>
        <div>
          <h2>{t("common.brand.name")}</h2>
          <strong>{t("common.brand.version")}</strong>
          <p>{t("about.description")}</p>
        </div>
        <span className={`about-runtime ${isTauri() ? "native" : "preview"}`}>
          {isTauri() ? t("about.runtime.native") : t("about.runtime.preview")}
        </span>
      </div>

      <div className="about-principles">
        <AboutCard icon={<ShieldCheck size={20} />} title={t("about.safety.title")} detail={t("about.safety.detail")} />
        <AboutCard icon={<HardDrive size={20} />} title={t("about.local.title")} detail={t("about.local.detail")} />
        <AboutCard icon={<Cpu size={20} />} title={t("about.platform.title")} detail={t("about.platform.detail")} />
      </div>

      <div className="about-stack card-surface">
        <div><Sparkles size={17} /><span><strong>{t("about.stack.title")}</strong><small>{t("about.stack.detail")}</small></span></div>
        <p>{t("about.disclaimer")}</p>
      </div>
    </section>
  );
}

function AboutCard({ icon, title, detail }: { icon: ReactNode; title: string; detail: string }) {
  return <article className="about-card card-surface"><span>{icon}</span><div><strong>{title}</strong><p>{detail}</p></div></article>;
}
