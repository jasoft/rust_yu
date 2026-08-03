import { convertFileSrc, isTauri } from "@tauri-apps/api/core";
import type { InstalledProgram } from "../types";

/**
 * 返回程序图标的可加载地址。
 *
 * 优先使用后端已经生成的 data URL；桌面端则通过 Tauri asset protocol
 * 读取受限的本地图标缓存目录，避免把任意本地文件暴露给 WebView。
 */
export function getProgramIconSrc(program: InstalledProgram): string | null {
  const dataUrl =
    program.icon_data_url_32 || program.icon_data_url_48 || program.icon_data_url;
  if (dataUrl) return dataUrl;

  if (!isTauri()) return null;

  const cachePath = program.icon_cache_path_32 || program.icon_cache_path_48;
  if (!cachePath) return null;

  return convertFileSrc(cachePath);
}
