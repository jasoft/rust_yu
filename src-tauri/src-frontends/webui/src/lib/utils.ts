import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并 Tailwind 类名 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 格式化字节数为可读字符串 */
export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes === 0) return "";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

/** 格式化安装来源为中文标签 */
export function formatSource(source: string): string {
  switch (source) {
    case "registry": return "注册表";
    case "msi": return "MSI";
    case "store": return "商店";
    default: return source;
  }
}
