import { convertFileSrc, isTauri } from "@tauri-apps/api/core";
import type { InstalledProgram } from "../types";

/**
 * 程序图标的可加载地址。
 *
 * - `src`：当前展示尺寸使用的图片。
 * - `srcSet`：列表行等场景在 2x 高分屏下改用 128px 高清档，避免小图被放大变糊。
 *
 * 优先使用后端已经生成的 data URL；桌面端则通过 Tauri asset protocol
 * 读取受限的本地图标缓存目录，避免把任意本地文件暴露给 WebView。
 */
export interface ProgramIconSources {
  src: string;
  srcSet?: string;
}

export function getProgramIconSources(
  program: InstalledProgram,
  options: { large?: boolean } = {},
): ProgramIconSources | null {
  const dataUrl =
    program.icon_data_url_32 || program.icon_data_url_48 || program.icon_data_url;
  if (dataUrl) return { src: dataUrl };

  if (!isTauri()) return null;

  const small = program.icon_cache_path_32;
  const large = program.icon_cache_path_large;

  if (options.large) {
    // 大图槽位（54px）直接用高清档，1x/2x 都保持清晰。
    const best = large ?? small;
    return best ? { src: convertFileSrc(best) } : null;
  }

  const src = small ?? large;
  if (!src) return null;

  // 列表 32px 槽位：1x 用 32px 缓存，2x 用 128px 高清档。
  const srcSet =
    small && large
      ? `${convertFileSrc(small)} 1x, ${convertFileSrc(large)} 2x`
      : undefined;
  return { src: convertFileSrc(src), srcSet };
}
