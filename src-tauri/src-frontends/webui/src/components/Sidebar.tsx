import { Package, FileText, Settings, Trash2 } from "lucide-react";
import { cn } from "../lib/utils";

type NavItem = "programs" | "reports" | "settings";

interface SidebarProps {
  active: NavItem;
  onNavigate: (item: NavItem) => void;
}

const navItems = [
  { id: "programs" as NavItem, label: "软件列表", icon: Package },
  { id: "reports" as NavItem, label: "卸载记录", icon: FileText },
  { id: "settings" as NavItem, label: "设置", icon: Settings },
];

export function Sidebar({ active, onNavigate }: SidebarProps) {
  return (
    <aside className="flex w-56 flex-col border-r border-slate-700 bg-slate-900">
      <div className="flex items-center gap-2 px-4 py-4 border-b border-slate-700">
        <Trash2 className="h-6 w-6 text-blue-500" />
        <span className="text-lg font-semibold text-white">Rust Yu</span>
      </div>

      <nav className="flex-1 px-2 py-3 space-y-1">
        {navItems.map((item) => (
          <button
            key={item.id}
            onClick={() => onNavigate(item.id)}
            className={cn(
              "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              active === item.id
                ? "bg-blue-600/20 text-blue-400"
                : "text-slate-400 hover:bg-slate-800 hover:text-white",
            )}
          >
            <item.icon className="h-4 w-4" />
            {item.label}
          </button>
        ))}
      </nav>

      <div className="border-t border-slate-700 px-4 py-3">
        <p className="text-xs text-slate-500">Rust Yu v0.1.3</p>
      </div>
    </aside>
  );
}
