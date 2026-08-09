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
          <h1><Wrench size={20} />工具箱</h1>
          <p>把常用维护能力收敛到一个入口，进入后仍沿用各页面的分析、确认和权限边界。</p>
        </div>
        <span className="toolbox-count">{visibleItems.length} / {toolboxItems.length} 个工具</span>
      </div>

      <div className="toolbox-safety card-surface">
        <ShieldCheck size={18} />
        <div>
          <strong>安全边界保持不变</strong>
          <p>工具箱只负责导航和搜索，不会绕过原页面的 dry-run、确认、管理员权限或浏览器关闭检查。浏览器预览不会执行本机操作。</p>
        </div>
      </div>

      <label className="toolbox-search card-surface">
        <Search size={16} />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索工具、功能或关键词…" aria-label="搜索工具" />
        {query && <button type="button" onClick={() => setQuery("")} aria-label="清空搜索"><span>清除</span></button>}
      </label>

      {visibleItems.length > 0 ? (
        <div className="toolbox-grid">
          {visibleItems.map((item) => <ToolboxCard key={item.id} item={item} onOpen={() => onNavigate(item.id)} />)}
        </div>
      ) : (
        <div className="toolbox-empty card-surface">
          <Search size={23} />
          <strong>没有匹配的工具</strong>
          <span>试试“启动”“备份”“插件”或“报告”。</span>
          <button type="button" className="secondary-button" onClick={() => setQuery("")}>显示全部工具</button>
        </div>
      )}

      <div className="toolbox-footer"><Archive size={14} /><span>破坏性操作仍从原功能页开始，并在任务记录中保留结果。</span></div>
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
