use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, UNIX_EPOCH};

use chrono::NaiveDate;
use chrono::Utc;
use walkdir::WalkDir;

use super::analyzer;
use super::models::{
    InstalledProgram, MetadataConfidence, MetadataSource, MetadataWarmupItemStatus,
    MetadataWarmupKind, SlowAppInfo,
};
use super::storage;
use crate::modules::common::text::{build_powershell_script, decode_windows_output};

const SIZE_SCAN_TIMEOUT: Duration = Duration::from_millis(300);
const SIZE_SCAN_MAX_ENTRIES: usize = 20_000;
const SIZE_SCAN_TOTAL_TIMEOUT: Duration = Duration::from_millis(900);
const SIZE_SCAN_MAX_TOTAL_ENTRIES: usize = 60_000;
const SIZE_SCAN_MAX_CANDIDATE_DIRS: usize = 8;
const ICON_SCAN_MAX_ENTRIES: usize = 128;
const ICON_SIZE_SMALL: u32 = 32;
const ICON_SIZE_LARGE: u32 = 48;
const ICON_CACHE_KEY_VERSION: u32 = 2;

/// 归一化安装日期为 YYYY-MM-DD
pub fn normalize_install_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let formats = ["%Y%m%d", "%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y"];
    for format in formats {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, format) {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    None
}

/// 清洗注册表中的图标路径并验证是否存在
pub fn sanitize_icon_path(raw: &str) -> Option<String> {
    let candidate = extract_icon_path_candidate(raw)?;
    if Path::new(&candidate).exists() {
        return Some(candidate);
    }
    None
}

/// 对程序元数据做增强和保守降级
pub fn enrich_program(program: &mut InstalledProgram) {
    // Delphi 版会在 LoadAdditionalInfo 中用卸载器命令反推真实安装目录；
    // 列表阶段只做轻量分析，不执行目录大小扫描，避免阻塞主线程。
    if let Some(analysis) = analyzer::analyze_program(program) {
        let location_missing = program
            .install_location
            .as_deref()
            .map(Path::new)
            .map(|path| !path.is_dir())
            .unwrap_or(true);
        if location_missing {
            program.install_location = analysis
                .install_location
                .as_ref()
                .map(|path| path.to_string_lossy().to_string());
        }
        if program.icon_path.is_none() {
            program.icon_path = analysis
                .executable_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string());
        }
    }

    // 安装日期：无效日期必须返回空并降级置信度
    if let Some(raw_date) = program.install_date.clone() {
        if let Some(normalized) = normalize_install_date(&raw_date) {
            program.install_date = Some(normalized);
            program.install_date_source = MetadataSource::Registry;
            program.install_date_confidence = MetadataConfidence::High;
        } else {
            program.install_date = None;
            program.install_date_source = MetadataSource::Registry;
            program.install_date_confidence = MetadataConfidence::Low;
        }
    } else {
        program.install_date_source = MetadataSource::Unknown;
        program.install_date_confidence = MetadataConfidence::Unknown;
    }

    // 图标：列表阶段仅解析来源路径，不同步生成图标缓存文件。
    resolve_program_icon_path(program);

    // 大小：列表阶段仅使用注册表中的 EstimatedSize。
    let (resolved_size, size_source, size_confidence) = resolve_cached_program_size(program);
    program.size = resolved_size;
    program.size_source = size_source;
    program.size_confidence = size_confidence;
    if program.size.is_some() {
        program.size_last_updated_at = Some(Utc::now().to_rfc3339());
    }

    refresh_metadata_confidence(program);
}

/// 批量增强元数据
pub fn enrich_programs(programs: &mut [InstalledProgram]) {
    for program in programs {
        enrich_program(program);
    }
}

/// 延迟加载程序的慢速元数据，语义对应 Delphi 的 `GetSlowAppInfo`。
///
/// 先读取旧版 ARPCache/YUCache 中的结果，再使用 Rust 分析器补全安装目录、图标、
/// 安装时间和目录大小。调用方应在后台线程中执行此函数。
pub fn load_slow_app_info(program: &InstalledProgram) -> SlowAppInfo {
    let mut info = read_legacy_slow_app_info(program).unwrap_or_default();
    let analysis = analyzer::analyze_program(program);

    let location = info
        .location
        .as_deref()
        .map(Path::new)
        .filter(|path| path.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| {
            program
                .install_location
                .as_deref()
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
        })
        .or_else(|| {
            analysis
                .as_ref()
                .and_then(|item| item.install_location.clone())
        });

    if info.image.is_none() {
        info.image = program
            .icon_path
            .as_deref()
            .and_then(sanitize_icon_path)
            .or_else(|| {
                analysis
                    .as_ref()
                    .and_then(|item| item.executable_path.as_ref())
                    .map(|path| path.to_string_lossy().to_string())
            })
            .or_else(|| {
                location
                    .as_deref()
                    .and_then(|path| find_icon_from_install_location(path.to_str()))
            });
    }

    if info.size.is_none() {
        info.size = program.estimated_size;
    }
    if info.installed.is_none() {
        info.installed = program
            .install_date
            .as_deref()
            .and_then(normalize_install_date);
    }

    if let Some(path) = location.as_deref() {
        info.location = Some(path.to_string_lossy().to_string());
        if info.installed.is_none() {
            info.installed = directory_created_at(path);
        }
        if info.last_used.is_none() {
            info.last_used = info
                .image
                .as_deref()
                .and_then(|image| Path::new(image).metadata().ok())
                .and_then(|metadata| metadata.accessed().ok())
                .and_then(system_time_to_rfc3339);
        }
    }

    info
}

/// 将慢速元数据写回统一的 Rust 应用模型。
pub fn apply_slow_app_info(program: &mut InstalledProgram, info: &SlowAppInfo) {
    if program.install_location.is_none() {
        program.install_location = info.location.clone();
    }
    if program.icon_path.is_none() {
        program.icon_path = info.image.clone();
    }
    if program.install_date.is_none() {
        program.install_date = info.installed.clone();
    }
    if program.size.is_none() {
        program.size = info.size;
    }
}

fn directory_created_at(path: &Path) -> Option<String> {
    path.metadata()
        .ok()
        .and_then(|metadata| metadata.created().ok())
        .and_then(system_time_to_rfc3339)
}

fn system_time_to_rfc3339(value: std::time::SystemTime) -> Option<String> {
    Some(chrono::DateTime::<Utc>::from(value).to_rfc3339())
}

#[cfg(windows)]
fn read_legacy_slow_app_info(program: &InstalledProgram) -> Option<SlowAppInfo> {
    use winreg::RegKey;

    let (_, uninstall_path) = crate::modules::common::utils::parse_registry_path(
        program.uninstall_registry_key_path.as_deref()?,
    )?;
    let key_name = uninstall_path.rsplit('\\').next()?;
    let (hive, _) = crate::modules::common::utils::parse_registry_path(
        program.uninstall_registry_key_path.as_deref()?,
    )?;
    let mut result = SlowAppInfo::default();

    for cache_name in ["ARPCache", "YUCache"] {
        let path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\App Management\{cache_name}\{key_name}"
        );
        let Ok(cache_key) = RegKey::predef(hive).open_subkey(path) else {
            continue;
        };
        let Ok(raw) = cache_key.get_raw_value("SlowInfoCache") else {
            continue;
        };
        let parsed = if cache_name == "ARPCache" {
            parse_arpcache_slow_info(&raw.bytes)
        } else {
            parse_yucache_slow_info(&raw.bytes)
        };
        if let Some(parsed) = parsed {
            merge_slow_app_info(&mut result, parsed);
        }
    }

    result.cache_hit.then_some(result)
}

#[cfg(not(windows))]
fn read_legacy_slow_app_info(_program: &InstalledProgram) -> Option<SlowAppInfo> {
    None
}

#[cfg(windows)]
fn parse_arpcache_slow_info(bytes: &[u8]) -> Option<SlowAppInfo> {
    if bytes.len() < 28 {
        return None;
    }

    Some(SlowAppInfo {
        size: read_u64(bytes, 8),
        last_used: read_filetime(bytes, 16),
        installed: None,
        times_used: read_i32(bytes, 24).map(|value| value.max(0) as u32),
        image: decode_legacy_text(&bytes[28..]),
        location: None,
        cache_hit: true,
    })
}

#[cfg(windows)]
fn parse_yucache_slow_info(bytes: &[u8]) -> Option<SlowAppInfo> {
    if bytes.len() < 28 {
        return None;
    }

    Some(SlowAppInfo {
        size: read_u64(bytes, 0),
        last_used: read_f64(bytes, 8).and_then(delphi_datetime_to_rfc3339),
        installed: read_f64(bytes, 16).and_then(delphi_datetime_to_rfc3339),
        times_used: read_i32(bytes, 24).map(|value| value.max(0) as u32),
        image: decode_fixed_legacy_text(bytes, 28, 261),
        location: decode_fixed_legacy_text(bytes, 28 + 261, 261),
        cache_hit: true,
    })
}

#[cfg(windows)]
fn merge_slow_app_info(target: &mut SlowAppInfo, source: SlowAppInfo) {
    target.size = target.size.or(source.size);
    target.last_used = target.last_used.take().or(source.last_used);
    target.installed = target.installed.take().or(source.installed);
    target.times_used = target.times_used.or(source.times_used);
    target.image = target.image.take().or(source.image);
    target.location = target.location.take().or(source.location);
    target.cache_hit |= source.cache_hit;
}

#[cfg(windows)]
fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let value = bytes.get(offset..end)?;
    Some(u64::from_le_bytes(value.try_into().ok()?))
}

#[cfg(windows)]
fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    let end = offset.checked_add(4)?;
    let value = bytes.get(offset..end)?;
    Some(i32::from_le_bytes(value.try_into().ok()?))
}

#[cfg(windows)]
fn read_f64(bytes: &[u8], offset: usize) -> Option<f64> {
    Some(f64::from_le_bytes(read_u64(bytes, offset)?.to_le_bytes()))
}

#[cfg(windows)]
fn read_filetime(bytes: &[u8], offset: usize) -> Option<String> {
    const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
    let ticks = read_u64(bytes, offset)?;
    let unix_ticks = ticks.checked_sub(WINDOWS_TO_UNIX_EPOCH_100NS)?;
    let seconds = i64::try_from(unix_ticks / 10_000_000).ok()?;
    let nanoseconds = (unix_ticks % 10_000_000) as u32 * 100;
    chrono::DateTime::<Utc>::from_timestamp(seconds, nanoseconds).map(|date| date.to_rfc3339())
}

#[cfg(windows)]
fn delphi_datetime_to_rfc3339(value: f64) -> Option<String> {
    if !value.is_finite() || value <= 25_569.0 {
        return None;
    }
    let seconds = ((value - 25_569.0) * 86_400.0) as i64;
    chrono::DateTime::<Utc>::from_timestamp(seconds, 0).map(|date| date.to_rfc3339())
}

#[cfg(windows)]
fn decode_legacy_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let value = if bytes.len() >= 4 {
        let utf16_bytes = bytes
            .chunks_exact(2)
            .take_while(|chunk| *chunk != [0, 0])
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        let utf16_units = utf16_bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let zero_high_bytes = utf16_bytes
            .chunks_exact(2)
            .filter(|chunk| chunk[1] == 0)
            .count();
        if !utf16_units.is_empty() && zero_high_bytes * 2 >= utf16_bytes.len().saturating_sub(2) {
            String::from_utf16(&utf16_units).ok()
        } else {
            None
        }
    } else {
        None
    }
    .unwrap_or_else(|| {
        let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
        String::from_utf8_lossy(bytes).to_string()
    });

    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(windows)]
fn decode_fixed_legacy_text(bytes: &[u8], offset: usize, length: usize) -> Option<String> {
    let end = offset.checked_add(length)?;
    decode_legacy_text(bytes.get(offset..end)?)
}

fn resolve_program_icon_path(program: &mut InstalledProgram) {
    let sanitized_registry_icon = program.icon_path.as_deref().and_then(sanitize_icon_path);
    let fallback_icon = find_icon_from_install_location(program.install_location.as_deref());
    let resolved_icon_path = sanitized_registry_icon.clone().or(fallback_icon);

    if let Some(icon_path) = resolved_icon_path {
        program.icon_path = Some(icon_path);
        program.icon_data_url = None;
        program.icon_data_url_32 = None;
        program.icon_data_url_48 = None;
        if sanitized_registry_icon.is_some() {
            program.icon_source = MetadataSource::Registry;
            program.icon_confidence = MetadataConfidence::High;
        } else {
            program.icon_source = MetadataSource::Filesystem;
            program.icon_confidence = MetadataConfidence::Medium;
        }
    } else {
        program.icon_path = None;
        program.icon_cache_path_32 = None;
        program.icon_cache_path_48 = None;
        program.icon_data_url = None;
        program.icon_data_url_32 = None;
        program.icon_data_url_48 = None;
        program.icon_source = MetadataSource::Unknown;
        program.icon_confidence = MetadataConfidence::Low;
    }
}

fn refresh_metadata_confidence(program: &mut InstalledProgram) {
    program.metadata_confidence = MetadataConfidence::lowest(&[
        program.install_date_confidence,
        program.icon_confidence,
        program.size_confidence,
    ]);
}

fn extract_icon_path_candidate(raw: &str) -> Option<String> {
    extract_icon_path_candidate_with_index(raw).map(|(path, _)| path)
}

fn extract_icon_path_candidate_with_index(raw: &str) -> Option<(String, i32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 优先解析引号路径，兼容 "C:\a\b\app.exe",0
    let (candidate_path, parsed_index) = if let Some(stripped) = trimmed.strip_prefix('"') {
        if let Some(end_idx) = stripped.find('"') {
            let quoted = stripped[..end_idx].trim();
            let remainder = stripped[end_idx + 1..].trim();
            let index = remainder
                .strip_prefix(',')
                .and_then(|value| value.trim().parse::<i32>().ok())
                .unwrap_or(0);
            (quoted.to_string(), index)
        } else {
            (trimmed.trim_matches('"').trim().to_string(), 0)
        }
    } else {
        // 再处理形如 C:\a\b\app.exe,0 的索引
        let mut parsed_index = 0i32;
        let without_index = if let Some(last_comma_idx) = trimmed.rfind(',') {
            let path_part = trimmed[..last_comma_idx].trim();
            let index_part = trimmed[last_comma_idx + 1..].trim();
            if !path_part.is_empty() && !index_part.is_empty() {
                if let Ok(value) = index_part.parse::<i32>() {
                    parsed_index = value;
                    path_part
                } else {
                    trimmed
                }
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        // 如果包含常见参数分隔符，截断参数部分
        let mut cleaned = without_index.trim_matches('"').trim().to_string();
        for marker in [" /", " -"] {
            if let Some(idx) = cleaned.find(marker) {
                cleaned = cleaned[..idx].trim().to_string();
            }
        }
        (cleaned, parsed_index)
    };

    let normalized_path = expand_windows_env_vars(candidate_path.trim_matches('"').trim());
    if normalized_path.is_empty() {
        return None;
    }
    Some((normalized_path, parsed_index))
}

#[cfg(windows)]
fn expand_windows_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut remain = input;

    while let Some(start_idx) = remain.find('%') {
        result.push_str(&remain[..start_idx]);
        let tail = &remain[start_idx + 1..];
        if let Some(end_rel_idx) = tail.find('%') {
            let var_name = tail[..end_rel_idx].trim();
            if var_name.is_empty() {
                result.push('%');
            } else if let Ok(value) = std::env::var(var_name) {
                result.push_str(&value);
            } else {
                result.push('%');
                result.push_str(var_name);
                result.push('%');
            }
            remain = &tail[end_rel_idx + 1..];
        } else {
            result.push_str(&remain[start_idx..]);
            remain = "";
            break;
        }
    }

    result.push_str(remain);
    result
}

#[cfg(not(windows))]
fn expand_windows_env_vars(input: &str) -> String {
    input.to_string()
}

fn find_icon_from_install_location(install_location: Option<&str>) -> Option<String> {
    let location = install_location?.trim();
    if location.is_empty() {
        return None;
    }

    let root = Path::new(location);
    if !root.exists() || !root.is_dir() {
        return None;
    }

    let mut scanned = 0usize;
    let mut fallback: Option<PathBuf> = None;
    let entries = std::fs::read_dir(root).ok()?;

    for entry in entries.flatten() {
        if scanned >= ICON_SCAN_MAX_ENTRIES {
            break;
        }
        scanned += 1;

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        match extension.as_deref() {
            Some("ico") => return Some(path.to_string_lossy().to_string()),
            Some("exe") => {
                if fallback.is_none() {
                    fallback = Some(path);
                }
            }
            Some("dll") => {
                if fallback.is_none() {
                    fallback = Some(path);
                }
            }
            _ => {}
        }
    }

    fallback.map(|path| path.to_string_lossy().to_string())
}

#[derive(Debug, Clone)]
struct IconAssetBundle {
    icon_cache_path_32: Option<String>,
    icon_cache_path_48: Option<String>,
}

fn build_icon_assets_from_path(icon_path: &str) -> Option<IconAssetBundle> {
    let (resolved_path, icon_index) = extract_icon_path_candidate_with_index(icon_path)
        .unwrap_or_else(|| (icon_path.trim_matches('"').to_string(), 0));
    let source_path = Path::new(&resolved_path);
    if !source_path.exists() || !source_path.is_file() {
        return None;
    }

    let (icon_32_path, icon_48_path) = resolve_icon_cache_paths(source_path, icon_index)?;
    if !icon_32_path.exists() || !icon_48_path.exists() {
        generate_icon_cache_files(source_path, icon_index, &icon_32_path, &icon_48_path)?;
    }

    if !icon_32_path.exists() && !icon_48_path.exists() {
        return None;
    }

    Some(IconAssetBundle {
        icon_cache_path_32: icon_32_path
            .exists()
            .then(|| icon_32_path.to_string_lossy().to_string()),
        icon_cache_path_48: icon_48_path
            .exists()
            .then(|| icon_48_path.to_string_lossy().to_string()),
    })
}

fn resolve_icon_cache_paths(source_path: &Path, icon_index: i32) -> Option<(PathBuf, PathBuf)> {
    let cache_root = storage::get_icon_cache_dir().ok()?;
    let cache_key = build_icon_cache_key(source_path, icon_index);
    let cache_32_dir = cache_root.join(ICON_SIZE_SMALL.to_string());
    let cache_48_dir = cache_root.join(ICON_SIZE_LARGE.to_string());
    std::fs::create_dir_all(&cache_32_dir).ok()?;
    std::fs::create_dir_all(&cache_48_dir).ok()?;
    let icon_32_path = cache_32_dir.join(format!("{}.png", cache_key));
    let icon_48_path = cache_48_dir.join(format!("{}.png", cache_key));
    Some((icon_32_path, icon_48_path))
}

fn build_icon_cache_key(source_path: &Path, icon_index: i32) -> String {
    let mut hasher = DefaultHasher::new();
    ICON_CACHE_KEY_VERSION.hash(&mut hasher);
    source_path
        .to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    icon_index.hash(&mut hasher);

    if let Ok(metadata) = std::fs::metadata(source_path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                duration.as_secs().hash(&mut hasher);
                duration.subsec_nanos().hash(&mut hasher);
            }
        }
    }

    format!("{:016x}", hasher.finish())
}

fn generate_icon_cache_files(
    source_path: &Path,
    icon_index: i32,
    icon_32_path: &Path,
    icon_48_path: &Path,
) -> Option<()> {
    #[cfg(not(windows))]
    {
        let _ = icon_index;
        let extension = source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())?;
        if !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico"
        ) {
            return None;
        }

        let bytes = std::fs::read(source_path).ok()?;
        if let Some(parent_dir) = icon_32_path.parent() {
            std::fs::create_dir_all(parent_dir).ok()?;
        }
        if let Some(parent_dir) = icon_48_path.parent() {
            std::fs::create_dir_all(parent_dir).ok()?;
        }

        std::fs::write(icon_32_path, &bytes).ok()?;
        std::fs::write(icon_48_path, &bytes).ok()?;
        Some(())
    }

    #[cfg(windows)]
    {
        let source_abs_path = source_path.to_string_lossy().to_string();
        let icon_index_value = icon_index.to_string();
        let icon_32_abs_path = icon_32_path.to_string_lossy().to_string();
        let icon_48_abs_path = icon_48_path.to_string_lossy().to_string();

        let script = r##"
$ErrorActionPreference = "Stop"
$sourcePath = $env:RUST_YU_ICON_SOURCE
$iconIndex = 0
[void][int]::TryParse($env:RUST_YU_ICON_INDEX, [ref]$iconIndex)
$target32 = $env:RUST_YU_ICON_32
$target48 = $env:RUST_YU_ICON_48

if ([string]::IsNullOrWhiteSpace($sourcePath)) { exit 2 }
if ([string]::IsNullOrWhiteSpace($target32)) { exit 2 }
if ([string]::IsNullOrWhiteSpace($target48)) { exit 2 }

function Save-IconAsPng {
    param(
        [Parameter(Mandatory = $true)][IntPtr]$IconHandle,
        [Parameter(Mandatory = $true)][string]$TargetPath
    )

    if ($IconHandle -eq [IntPtr]::Zero) { return }

    $dir = [System.IO.Path]::GetDirectoryName($TargetPath)
    if (-not [string]::IsNullOrWhiteSpace($dir) -and -not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }

    $icon = [System.Drawing.Icon]::FromHandle($IconHandle)
    $bitmap = $icon.ToBitmap()
    $bitmap.Save($TargetPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    $icon.Dispose()
}

function Save-ResizedImageAsPng {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Image]$Image,
        [Parameter(Mandatory = $true)][int]$Size,
        [Parameter(Mandatory = $true)][string]$TargetPath
    )

    $dir = [System.IO.Path]::GetDirectoryName($TargetPath)
    if (-not [string]::IsNullOrWhiteSpace($dir) -and -not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }

    $canvas = New-Object System.Drawing.Bitmap($Size, $Size)
    $graphics = [System.Drawing.Graphics]::FromImage($canvas)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    $ratio = [Math]::Min($Size / [double]$Image.Width, $Size / [double]$Image.Height)
    $drawWidth = [int][Math]::Max(1, [Math]::Round($Image.Width * $ratio))
    $drawHeight = [int][Math]::Max(1, [Math]::Round($Image.Height * $ratio))
    $offsetX = [int][Math]::Floor(($Size - $drawWidth) / 2)
    $offsetY = [int][Math]::Floor(($Size - $drawHeight) / 2)
    $graphics.DrawImage($Image, $offsetX, $offsetY, $drawWidth, $drawHeight)
    $canvas.Save($TargetPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $canvas.Dispose()
}

$icon32 = [IntPtr]::Zero
$icon48 = [IntPtr]::Zero

try {
    $actualPath = [Environment]::ExpandEnvironmentVariables($sourcePath.Trim('"'))
    if (-not (Test-Path -LiteralPath $actualPath)) { exit 3 }

    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
public struct SHFILEINFO {
    public IntPtr hIcon;
    public int iIcon;
    public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
}

public static class ShellIconBridge {
    private const uint SHGFI_ICON = 0x100;
    private const uint SHGFI_LARGEICON = 0x0;

    [DllImport("shell32.dll", CharSet = CharSet.Auto)]
    private static extern IntPtr SHGetFileInfo(string pszPath, uint dwFileAttributes, ref SHFILEINFO psfi, uint cbFileInfo, uint uFlags);

    [DllImport("shell32.dll", CharSet = CharSet.Auto)]
    private static extern uint ExtractIconEx(string szFileName, int nIconIndex, IntPtr[] phiconLarge, IntPtr[] phiconSmall, uint nIcons);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern uint PrivateExtractIcons(
        string szFileName,
        int nIconIndex,
        int cxIcon,
        int cyIcon,
        IntPtr[] phicon,
        uint[] piconid,
        uint nIcons,
        uint flags
    );

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool DestroyIcon(IntPtr hIcon);

    private static IntPtr ExtractBySize(string path, int index, int size) {
        try {
            IntPtr[] icons = new IntPtr[1];
            uint[] ids = new uint[1];
            if (PrivateExtractIcons(path, index, size, size, icons, ids, 1, 0) > 0) {
                if (icons[0] != IntPtr.Zero) {
                    return icons[0];
                }
            }
        } catch { }
        return IntPtr.Zero;
    }

    public static IntPtr Extract32(string path, int index) {
        IntPtr bySize = ExtractBySize(path, index, 32);
        if (bySize != IntPtr.Zero) {
            return bySize;
        }

        try {
            if (index != 0) {
                IntPtr[] large = new IntPtr[1];
                if (ExtractIconEx(path, index, large, null, 1) > 0) {
                    return large[0];
                }
            }

            SHFILEINFO shfi = new SHFILEINFO();
            if (SHGetFileInfo(path, 0, ref shfi, (uint)Marshal.SizeOf(shfi), SHGFI_ICON | SHGFI_LARGEICON) != IntPtr.Zero) {
                return shfi.hIcon;
            }
        } catch { }
        return IntPtr.Zero;
    }

    public static IntPtr Extract48(string path, int index) {
        return ExtractBySize(path, index, 48);
    }
}
"@ | Out-Null

    $extension = [System.IO.Path]::GetExtension($actualPath).ToLowerInvariant()

    if ($extension -ne ".exe" -and $extension -ne ".dll") {
        $img = [System.Drawing.Image]::FromFile($actualPath)
        Save-ResizedImageAsPng -Image $img -Size 32 -TargetPath $target32
        Save-ResizedImageAsPng -Image $img -Size 48 -TargetPath $target48
        $img.Dispose()
        if ((Test-Path -LiteralPath $target32) -and (Test-Path -LiteralPath $target48)) {
            exit 0
        }
        exit 4
    }

    $icon32 = [ShellIconBridge]::Extract32($actualPath, $iconIndex)
    $icon48 = [ShellIconBridge]::Extract48($actualPath, $iconIndex)

    if ($icon48 -eq [IntPtr]::Zero) {
        $icon48 = $icon32
    }
    if ($icon32 -eq [IntPtr]::Zero) {
        $icon32 = $icon48
    }
    if ($icon32 -eq [IntPtr]::Zero -or $icon48 -eq [IntPtr]::Zero) {
        exit 5
    }

    Save-IconAsPng -IconHandle $icon32 -TargetPath $target32
    Save-IconAsPng -IconHandle $icon48 -TargetPath $target48

    if (-not (Test-Path -LiteralPath $target32) -or -not (Test-Path -LiteralPath $target48)) {
        exit 6
    }
    exit 0
} catch {
    Write-Error ("icon extract failed: " + $_.Exception.Message)
    exit 1
} finally {
    if ($icon32 -ne [IntPtr]::Zero) { [void][ShellIconBridge]::DestroyIcon($icon32) }
    if ($icon48 -ne [IntPtr]::Zero -and $icon48 -ne $icon32) { [void][ShellIconBridge]::DestroyIcon($icon48) }
}
"##;

        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &build_powershell_script(script),
            ])
            .env("RUST_YU_ICON_SOURCE", &source_abs_path)
            .env("RUST_YU_ICON_INDEX", &icon_index_value)
            .env("RUST_YU_ICON_32", &icon_32_abs_path)
            .env("RUST_YU_ICON_48", &icon_48_abs_path)
            .output()
            .ok()?;

        if !output.status.success() {
            tracing::debug!(
                "提取图标失败: source={}, index={}, status={:?}, stderr={}, stdout={}",
                source_abs_path,
                icon_index,
                output.status.code(),
                decode_windows_output(&output.stderr),
                decode_windows_output(&output.stdout)
            );
            return None;
        }
        if !icon_32_path.exists() || !icon_48_path.exists() {
            tracing::debug!(
                "图标提取命令执行成功但输出文件缺失: source={}, index={}, path32={}, path48={}",
                source_abs_path,
                icon_index,
                icon_32_abs_path,
                icon_48_abs_path
            );
            return None;
        }

        Some(())
    }
}

fn resolve_cached_program_size(
    program: &InstalledProgram,
) -> (Option<u64>, MetadataSource, MetadataConfidence) {
    if let Some(estimated) = program.estimated_size {
        return (
            Some(estimated),
            MetadataSource::Registry,
            MetadataConfidence::High,
        );
    }

    (None, MetadataSource::Unknown, MetadataConfidence::Low)
}

pub fn warmup_program_metadata(
    program: &mut InstalledProgram,
    kind: MetadataWarmupKind,
) -> (MetadataWarmupItemStatus, Option<String>) {
    let result = match kind {
        MetadataWarmupKind::Icons => warmup_program_icon_assets(program),
        MetadataWarmupKind::Sizes => warmup_program_size(program),
    };
    refresh_metadata_confidence(program);
    result
}

pub fn is_program_metadata_warmup_eligible(
    program: &InstalledProgram,
    kind: MetadataWarmupKind,
) -> bool {
    match kind {
        MetadataWarmupKind::Icons => {
            program
                .icon_path
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
                && !has_ready_icon_cache(program)
        }
        MetadataWarmupKind::Sizes => {
            !collect_size_scan_locations(program).is_empty() || program.estimated_size.is_some()
        }
    }
}

fn has_ready_icon_cache(program: &InstalledProgram) -> bool {
    let cache_32_ready = program
        .icon_cache_path_32
        .as_deref()
        .map(Path::new)
        .map(Path::exists)
        .unwrap_or(false);
    let cache_48_ready = program
        .icon_cache_path_48
        .as_deref()
        .map(Path::new)
        .map(Path::exists)
        .unwrap_or(false);

    cache_32_ready && cache_48_ready
}

fn warmup_program_icon_assets(
    program: &mut InstalledProgram,
) -> (MetadataWarmupItemStatus, Option<String>) {
    let slow_info = load_slow_app_info(program);
    apply_slow_app_info(program, &slow_info);
    let Some(icon_extract_source) = program.icon_path.as_deref() else {
        program.icon_cache_path_32 = None;
        program.icon_cache_path_48 = None;
        return (
            MetadataWarmupItemStatus::Skipped,
            Some("icon_source_missing".to_string()),
        );
    };

    match build_icon_assets_from_path(icon_extract_source) {
        Some(icon_assets) => {
            let changed = program.icon_cache_path_32 != icon_assets.icon_cache_path_32
                || program.icon_cache_path_48 != icon_assets.icon_cache_path_48;
            program.icon_cache_path_32 = icon_assets.icon_cache_path_32;
            program.icon_cache_path_48 = icon_assets.icon_cache_path_48;
            program.icon_data_url = None;
            program.icon_data_url_32 = None;
            program.icon_data_url_48 = None;
            if changed {
                (MetadataWarmupItemStatus::Updated, None)
            } else {
                (
                    MetadataWarmupItemStatus::Skipped,
                    Some("icon_cache_ready".to_string()),
                )
            }
        }
        None => {
            program.icon_cache_path_32 = None;
            program.icon_cache_path_48 = None;
            (
                MetadataWarmupItemStatus::Failed,
                Some("icon_extract_failed".to_string()),
            )
        }
    }
}

fn warmup_program_size(
    program: &mut InstalledProgram,
) -> (MetadataWarmupItemStatus, Option<String>) {
    let slow_info = load_slow_app_info(program);
    apply_slow_app_info(program, &slow_info);
    let candidate_dirs = collect_size_scan_locations(program);
    if candidate_dirs.is_empty() {
        if let Some(estimated) = program.estimated_size {
            program.size = Some(estimated);
            program.size_source = MetadataSource::Registry;
            program.size_confidence = MetadataConfidence::High;
            program.size_last_updated_at = Some(Utc::now().to_rfc3339());
            return (
                MetadataWarmupItemStatus::Skipped,
                Some("estimated_size_only".to_string()),
            );
        }

        program.size = None;
        program.size_source = MetadataSource::Unknown;
        program.size_confidence = MetadataConfidence::Low;
        return (
            MetadataWarmupItemStatus::Skipped,
            Some("size_scan_location_missing".to_string()),
        );
    }

    // 这里显式把安装目录和保守匹配到的 AppData 目录一起纳入统计，
    // 让 agent / GUI 触发的 size 预热可以反映运行期写入的数据变化。
    match calculate_directory_sizes_limited(
        &candidate_dirs,
        SIZE_SCAN_TIMEOUT,
        SIZE_SCAN_TOTAL_TIMEOUT,
        SIZE_SCAN_MAX_ENTRIES,
        SIZE_SCAN_MAX_TOTAL_ENTRIES,
    ) {
        Some(size) if size > 0 => {
            let changed = program.size != Some(size)
                || program.size_source != MetadataSource::Filesystem
                || program.size_confidence != MetadataConfidence::Medium;
            program.size = Some(size);
            program.size_source = MetadataSource::Filesystem;
            program.size_confidence = MetadataConfidence::Medium;
            program.size_last_updated_at = Some(Utc::now().to_rfc3339());
            if changed {
                (MetadataWarmupItemStatus::Updated, None)
            } else {
                (
                    MetadataWarmupItemStatus::Skipped,
                    Some("filesystem_size_ready".to_string()),
                )
            }
        }
        Some(_) => {
            program.size = None;
            program.size_source = MetadataSource::Unknown;
            program.size_confidence = MetadataConfidence::Low;
            (
                MetadataWarmupItemStatus::Skipped,
                Some("filesystem_size_empty".to_string()),
            )
        }
        None => {
            program.size = program.estimated_size;
            if program.size.is_some() {
                program.size_source = MetadataSource::Registry;
                program.size_confidence = MetadataConfidence::Medium;
            } else {
                program.size_source = MetadataSource::Unknown;
                program.size_confidence = MetadataConfidence::Low;
            }
            (
                MetadataWarmupItemStatus::Failed,
                Some("filesystem_size_timeout".to_string()),
            )
        }
    }
}

fn calculate_directory_sizes_limited(
    directories: &[PathBuf],
    per_directory_timeout: Duration,
    total_timeout: Duration,
    per_directory_max_entries: usize,
    total_max_entries: usize,
) -> Option<u64> {
    let started_at = Instant::now();
    let mut total_size = 0u64;
    let mut total_entries = 0usize;

    for directory in directories {
        if started_at.elapsed() > total_timeout {
            return None;
        }

        let directory_started = Instant::now();
        for entry in WalkDir::new(directory).into_iter().filter_map(Result::ok) {
            if directory_started.elapsed() > per_directory_timeout
                || started_at.elapsed() > total_timeout
            {
                return None;
            }

            total_entries += 1;
            if total_entries > total_max_entries
                || total_entries > directories.len() * per_directory_max_entries
            {
                return None;
            }

            if !entry.file_type().is_file() {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                total_size = total_size.saturating_add(metadata.len());
            }
        }
    }

    Some(total_size)
}

fn collect_size_scan_locations(program: &InstalledProgram) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(install_location) = program.install_location.as_deref() {
        let location = PathBuf::from(install_location.trim());
        if location.is_dir() {
            insert_candidate_dir(&mut candidates, &mut seen, location);
        }
    }

    // 这里仅在 AppData 的浅层目录中按应用名/发布者保守匹配，
    // 避免把不相关的大目录误计入程序大小。
    for root in collect_appdata_roots() {
        for candidate in discover_matching_appdata_dirs(&root, program) {
            insert_candidate_dir(&mut candidates, &mut seen, candidate);
            if candidates.len() >= SIZE_SCAN_MAX_CANDIDATE_DIRS {
                return candidates;
            }
        }
    }

    candidates
}

fn insert_candidate_dir(
    candidates: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<String>,
    path: PathBuf,
) {
    let normalized = path.to_string_lossy().to_lowercase();
    if seen.insert(normalized) {
        candidates.push(path);
    }
}

fn collect_appdata_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        let path = PathBuf::from(appdata);
        if path.is_dir() {
            roots.push(path);
        }
    }

    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let local_path = PathBuf::from(local_appdata);
        if local_path.is_dir() {
            roots.push(local_path.clone());
            if let Some(parent) = local_path.parent() {
                let local_low = parent.join("LocalLow");
                if local_low.is_dir() {
                    roots.push(local_low);
                }
            }
        }
    }

    roots
}

fn discover_matching_appdata_dirs(root: &Path, program: &InstalledProgram) -> Vec<PathBuf> {
    let app_keys = build_app_match_keys(&program.name);
    let publisher_keys = program
        .publisher
        .as_deref()
        .map(build_app_match_keys)
        .unwrap_or_default();
    let mut matches = Vec::new();

    let Ok(entries) = std::fs::read_dir(root) else {
        return matches;
    };

    for entry in entries.flatten().take(128) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let directory_name = path
            .file_name()
            .map(|value| normalize_match_key(&value.to_string_lossy()))
            .unwrap_or_default();

        if directory_matches_keys(&directory_name, &app_keys) {
            matches.push(path);
            if matches.len() >= SIZE_SCAN_MAX_CANDIDATE_DIRS {
                return matches;
            }
            continue;
        }

        if directory_matches_keys(&directory_name, &publisher_keys) {
            let Ok(nested_entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for nested_entry in nested_entries.flatten().take(32) {
                let nested_path = nested_entry.path();
                if !nested_path.is_dir() {
                    continue;
                }

                let nested_name = nested_path
                    .file_name()
                    .map(|value| normalize_match_key(&value.to_string_lossy()))
                    .unwrap_or_default();
                if directory_matches_keys(&nested_name, &app_keys) {
                    matches.push(nested_path);
                    if matches.len() >= SIZE_SCAN_MAX_CANDIDATE_DIRS {
                        return matches;
                    }
                }
            }
        }
    }

    matches
}

fn build_app_match_keys(value: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let normalized = normalize_match_key(value);
    if normalized.len() >= 2 {
        keys.push(normalized);
    }

    for segment in value.split(|ch: char| !ch.is_alphanumeric() && !is_cjk(ch)) {
        let normalized_segment = normalize_match_key(segment);
        if normalized_segment.len() >= 2 && !keys.contains(&normalized_segment) {
            keys.push(normalized_segment);
        }
    }

    keys
}

fn normalize_match_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric() || is_cjk(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn directory_matches_keys(directory_name: &str, keys: &[String]) -> bool {
    keys.iter().filter(|key| key.len() >= 2).any(|key| {
        directory_name == key || directory_name.starts_with(key) || directory_name.contains(key)
    })
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::modules::lister::models::InstallSource;

    const STORAGE_DIR_ENV: &str = "RUST_YU_STORAGE_DIR";

    fn with_storage_root(test_name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-icon-storage-test-{}-{}",
            test_name,
            uuid::Uuid::new_v4()
        ));
        let _ = fs::create_dir_all(&root);
        std::env::set_var(STORAGE_DIR_ENV, &root);
        root
    }

    fn cleanup_storage_root(root: &Path) {
        std::env::remove_var(STORAGE_DIR_ENV);
        let _ = fs::remove_dir_all(root);
    }

    fn set_appdata_env(root: &Path) {
        let roaming = root.join("Roaming");
        let local = root.join("Local");
        let local_low = root.join("LocalLow");
        let _ = fs::create_dir_all(&roaming);
        let _ = fs::create_dir_all(&local);
        let _ = fs::create_dir_all(&local_low);
        std::env::set_var("APPDATA", &roaming);
        std::env::set_var("LOCALAPPDATA", &local);
    }

    fn clear_appdata_env() {
        std::env::remove_var("APPDATA");
        std::env::remove_var("LOCALAPPDATA");
    }

    fn write_minimal_png(path: &Path) -> bool {
        // 1x1 透明 PNG
        let bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE5, 0x27, 0xD4, 0xA2, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        fs::write(path, bytes).is_ok()
    }

    #[test]
    fn normalize_install_date_supports_registry_format() {
        assert_eq!(
            normalize_install_date("20240115"),
            Some("2024-01-15".to_string())
        );
        assert_eq!(
            normalize_install_date("2024/01/15"),
            Some("2024-01-15".to_string())
        );
    }

    #[test]
    fn normalize_install_date_returns_none_for_invalid_input() {
        assert_eq!(normalize_install_date("not-a-date"), None);
        assert_eq!(normalize_install_date(""), None);
    }

    #[test]
    fn sanitize_icon_path_strips_index_and_validates_existence() {
        let temp_root = std::env::temp_dir().join(format!("rust-yu-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&temp_root).is_ok());

        let icon_path = temp_root.join("app.exe");
        assert!(fs::write(&icon_path, b"binary").is_ok());

        let raw = format!("\"{}\",0", icon_path.to_string_lossy());
        assert_eq!(
            sanitize_icon_path(&raw),
            Some(icon_path.to_string_lossy().to_string())
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn extract_icon_path_candidate_keeps_quoted_index() {
        let parsed =
            extract_icon_path_candidate_with_index(r#""C:\Program Files\Demo\demo.exe",5"#);
        assert_eq!(
            parsed,
            Some((r"C:\Program Files\Demo\demo.exe".to_string(), 5))
        );
    }

    #[test]
    fn build_icon_assets_generates_cache_paths_for_image_file() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = with_storage_root("data-url");
        let temp_root = std::env::temp_dir().join(format!("rust-yu-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&temp_root).is_ok());

        let icon_path = temp_root.join("icon.png");
        assert!(write_minimal_png(&icon_path));

        let assets = build_icon_assets_from_path(&icon_path.to_string_lossy())
            .unwrap_or_else(|| panic!("failed to build icon assets"));
        assert!(assets.icon_cache_path_32.is_some());
        assert!(assets.icon_cache_path_48.is_some());

        cleanup_storage_root(&storage_root);
        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_icon_assets_generates_32_and_48_cache_files() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = with_storage_root("dual-size");
        let source_root =
            std::env::temp_dir().join(format!("rust-yu-icon-source-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&source_root).is_ok());

        let icon_source_path = source_root.join("source.png");
        assert!(write_minimal_png(&icon_source_path));

        let assets = build_icon_assets_from_path(&icon_source_path.to_string_lossy())
            .unwrap_or_else(|| panic!("failed to build icon assets"));

        let cache_path_32 = assets.icon_cache_path_32.unwrap_or_default();
        let cache_path_48 = assets.icon_cache_path_48.unwrap_or_default();
        assert!(cache_path_32.contains("\\32\\"));
        assert!(cache_path_48.contains("\\48\\"));
        assert!(Path::new(&cache_path_32).exists());
        assert!(Path::new(&cache_path_48).exists());

        cleanup_storage_root(&storage_root);
        let _ = fs::remove_dir_all(&source_root);
    }

    #[test]
    fn build_icon_assets_uses_icon_index_for_cache_key() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = with_storage_root("icon-index-key");
        let source_root =
            std::env::temp_dir().join(format!("rust-yu-icon-index-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&source_root).is_ok());

        let icon_source_path = source_root.join("source.png");
        assert!(write_minimal_png(&icon_source_path));

        let icon_with_index_0 = format!("\"{}\",0", icon_source_path.to_string_lossy());
        let icon_with_index_1 = format!("\"{}\",1", icon_source_path.to_string_lossy());

        let assets_idx0 = build_icon_assets_from_path(&icon_with_index_0)
            .unwrap_or_else(|| panic!("failed to build icon assets idx0"));
        let assets_idx1 = build_icon_assets_from_path(&icon_with_index_1)
            .unwrap_or_else(|| panic!("failed to build icon assets idx1"));

        assert_ne!(
            assets_idx0.icon_cache_path_32,
            assets_idx1.icon_cache_path_32
        );
        assert_ne!(
            assets_idx0.icon_cache_path_48,
            assets_idx1.icon_cache_path_48
        );

        cleanup_storage_root(&storage_root);
        let _ = fs::remove_dir_all(&source_root);
    }

    #[cfg(windows)]
    #[test]
    fn build_icon_assets_extracts_native_32_and_48_icons_from_exe() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = with_storage_root("exe-native-icons");
        let windows_dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let notepad_path = PathBuf::from(windows_dir)
            .join("System32")
            .join("notepad.exe");
        assert!(notepad_path.exists());

        let raw_icon = format!("\"{}\",0", notepad_path.to_string_lossy());
        let assets = build_icon_assets_from_path(&raw_icon)
            .unwrap_or_else(|| panic!("failed to build icon assets for exe"));

        let cache_path_32 = assets.icon_cache_path_32.unwrap_or_default();
        let cache_path_48 = assets.icon_cache_path_48.unwrap_or_default();
        assert!(cache_path_32.contains("\\32\\"));
        assert!(cache_path_48.contains("\\48\\"));
        assert!(Path::new(&cache_path_32).exists());
        assert!(Path::new(&cache_path_48).exists());
        assert!(
            fs::metadata(&cache_path_32)
                .map(|meta| meta.len())
                .unwrap_or(0)
                > 0
        );
        assert!(
            fs::metadata(&cache_path_48)
                .map(|meta| meta.len())
                .unwrap_or(0)
                > 0
        );

        cleanup_storage_root(&storage_root);
    }

    #[test]
    fn resolve_size_prefers_estimated_size() {
        let mut program = InstalledProgram::new("TestApp".to_string(), InstallSource::Registry);
        program.estimated_size = Some(1024);
        enrich_program(&mut program);

        assert_eq!(program.size, Some(1024));
        assert_eq!(program.size_source, MetadataSource::Registry);
        assert_eq!(program.size_confidence, MetadataConfidence::High);
    }

    #[test]
    fn enrich_program_keeps_size_empty_when_estimated_missing() {
        let temp_root = std::env::temp_dir().join(format!("rust-yu-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&temp_root).is_ok());
        let test_file = temp_root.join("data.bin");
        assert!(fs::write(&test_file, vec![1u8; 2048]).is_ok());

        let mut program = InstalledProgram::new("FsFallback".to_string(), InstallSource::Registry);
        program.install_location = Some(temp_root.to_string_lossy().to_string());
        enrich_program(&mut program);

        assert_eq!(program.size, None);
        assert_eq!(program.size_source, MetadataSource::Unknown);

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn icon_warmup_is_incremental_when_cache_files_exist() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = with_storage_root("icon-ready-check");
        let cache_32 = storage_root.join("icon-cache").join("32").join("ready.png");
        let cache_48 = storage_root.join("icon-cache").join("48").join("ready.png");
        assert!(cache_32
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .is_ok());
        assert!(cache_48
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .is_ok());
        assert!(write_minimal_png(&cache_32));
        assert!(write_minimal_png(&cache_48));

        let mut program = InstalledProgram::new("ReadyIcon".to_string(), InstallSource::Registry);
        program.icon_path = Some(r"C:\Program Files\ReadyIcon\app.exe".to_string());
        program.icon_cache_path_32 = Some(cache_32.to_string_lossy().to_string());
        program.icon_cache_path_48 = Some(cache_48.to_string_lossy().to_string());

        assert!(!is_program_metadata_warmup_eligible(
            &program,
            MetadataWarmupKind::Icons
        ));

        cleanup_storage_root(&storage_root);
    }

    #[test]
    fn size_warmup_scans_install_location_and_matching_appdata() {
        let _guard = super::storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let appdata_root = std::env::temp_dir().join(format!(
            "rust-yu-appdata-size-test-{}",
            uuid::Uuid::new_v4()
        ));
        let install_root = std::env::temp_dir().join(format!(
            "rust-yu-install-size-test-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(fs::create_dir_all(&install_root).is_ok());
        assert!(fs::write(install_root.join("install.bin"), vec![1u8; 2048]).is_ok());
        set_appdata_env(&appdata_root);

        let app_data_dir = appdata_root
            .join("Roaming")
            .join("VendorCo")
            .join("SizeApp");
        assert!(fs::create_dir_all(&app_data_dir).is_ok());
        assert!(fs::write(app_data_dir.join("user.dat"), vec![2u8; 1024]).is_ok());

        let mut program = InstalledProgram::new("SizeApp".to_string(), InstallSource::Registry);
        program.publisher = Some("VendorCo".to_string());
        program.install_location = Some(install_root.to_string_lossy().to_string());

        let (status, message) = warmup_program_metadata(&mut program, MetadataWarmupKind::Sizes);

        assert_eq!(status, MetadataWarmupItemStatus::Updated);
        assert_eq!(message, None);
        assert!(program.size.unwrap_or(0) >= 3072);
        assert_eq!(program.size_source, MetadataSource::Filesystem);

        clear_appdata_env();
        let _ = fs::remove_dir_all(&appdata_root);
        let _ = fs::remove_dir_all(&install_root);
    }
}
