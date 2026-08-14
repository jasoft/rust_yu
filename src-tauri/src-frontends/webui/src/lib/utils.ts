import { t } from "../i18n/index.ts";
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并 Tailwind 类名 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 格式化字节数为可读字符串 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  if (bytes === 0) return t("shredder.bytes.zero");
  const k = 1024;
  const sizes = [
    t("shredder.bytes.b"),
    t("shredder.bytes.kb"),
    t("shredder.bytes.mb"),
    t("shredder.bytes.gb"),
    t("shredder.bytes.tb"),
  ];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const unitIndex = Math.min(i, sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, unitIndex)).toFixed(1))} ${sizes[unitIndex]}`;
}

/** 格式化安装来源为中文标签 */
export function formatSource(source: string): string {
  switch (source) {
    case "registry": return t("app.message_024");
    case "msi": return t("common.format.msi");
    case "store": return t("lib.programfilters.message_003");
    default: return source;
  }
}
