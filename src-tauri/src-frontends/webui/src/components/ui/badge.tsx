import type { HTMLAttributes } from "react";
import { cn } from "../../lib/utils";

type Variant = "default" | "secondary" | "outline" | "success" | "warning" | "destructive";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: Variant;
}

const variantStyles: Record<Variant, string> = {
  default: "bg-blue-600/20 text-blue-400 border-blue-500/30",
  secondary: "bg-slate-600/30 text-slate-300 border-slate-500/30",
  outline: "text-slate-400 border-slate-500/50",
  success: "bg-green-600/20 text-green-400 border-green-500/30",
  warning: "bg-yellow-600/20 text-yellow-400 border-yellow-500/30",
  destructive: "bg-red-600/20 text-red-400 border-red-500/30",
};

export function Badge({ className, variant = "default", ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium",
        variantStyles[variant],
        className,
      )}
      {...props}
    />
  );
}
