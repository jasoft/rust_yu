use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, UNIX_EPOCH};

use chrono::NaiveDate;
use chrono::Utc;
use walkdir::WalkDir;

use super::models::{InstalledProgram, MetadataConfidence, MetadataSource};
use super::storage;

const SIZE_SCAN_TIMEOUT: Duration = Duration::from_millis(300);
const SIZE_SCAN_MAX_ENTRIES: usize = 20_000;
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

    // 图标：先清洗 DisplayIcon，再从安装目录回退
    let original_display_icon = program.icon_path.clone();
    let sanitized_icon = program
        .icon_path
        .as_deref()
        .and_then(sanitize_icon_path)
        .or_else(|| find_icon_from_install_location(program.install_location.as_deref()));

    if let Some(icon_path) = sanitized_icon {
        let from_registry = program
            .icon_path
            .as_deref()
            .and_then(sanitize_icon_path)
            .is_some();
        program.icon_path = Some(icon_path);
        let icon_extract_source = original_display_icon
            .as_deref()
            .unwrap_or_else(|| program.icon_path.as_deref().unwrap_or_default());
        if let Some(icon_assets) = build_icon_assets_from_path(icon_extract_source) {
            program.icon_cache_path_32 = icon_assets.icon_cache_path_32;
            program.icon_cache_path_48 = icon_assets.icon_cache_path_48;
            // 不再缓存/传输 base64 图标，仅保留磁盘缓存路径
            program.icon_data_url = None;
            program.icon_data_url_32 = None;
            program.icon_data_url_48 = None;
        } else {
            program.icon_data_url = None;
            program.icon_data_url_32 = None;
            program.icon_data_url_48 = None;
            program.icon_cache_path_32 = None;
            program.icon_cache_path_48 = None;
        }
        if from_registry {
            program.icon_source = MetadataSource::Registry;
            program.icon_confidence = MetadataConfidence::High;
        } else {
            program.icon_source = MetadataSource::Filesystem;
            program.icon_confidence = MetadataConfidence::Medium;
        }
    } else {
        program.icon_path = None;
        program.icon_data_url = None;
        program.icon_data_url_32 = None;
        program.icon_data_url_48 = None;
        program.icon_cache_path_32 = None;
        program.icon_cache_path_48 = None;
        program.icon_source = MetadataSource::Unknown;
        program.icon_confidence = MetadataConfidence::Low;
    }

    // 大小：优先 EstimatedSize，缺失时回退文件系统扫描
    let (resolved_size, size_source, size_confidence) = resolve_program_size(program);
    program.size = resolved_size;
    program.size_source = size_source;
    program.size_confidence = size_confidence;
    if program.size.is_some() {
        program.size_last_updated_at = Some(Utc::now().to_rfc3339());
    }

    program.metadata_confidence = MetadataConfidence::lowest(&[
        program.install_date_confidence,
        program.icon_confidence,
        program.size_confidence,
    ]);
}

/// 批量增强元数据
pub fn enrich_programs(programs: &mut [InstalledProgram]) {
    for program in programs {
        enrich_program(program);
    }
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
                script,
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
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
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

fn resolve_program_size(
    program: &InstalledProgram,
) -> (Option<u64>, MetadataSource, MetadataConfidence) {
    if let Some(estimated) = program.estimated_size {
        return (
            Some(estimated),
            MetadataSource::Registry,
            MetadataConfidence::High,
        );
    }

    let location = match program.install_location.as_deref() {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => {
            return (None, MetadataSource::Unknown, MetadataConfidence::Low);
        }
    };

    if !location.exists() || !location.is_dir() {
        return (None, MetadataSource::Unknown, MetadataConfidence::Low);
    }

    match calculate_directory_size_limited(&location, SIZE_SCAN_TIMEOUT, SIZE_SCAN_MAX_ENTRIES) {
        Some(size) if size > 0 => (
            Some(size),
            MetadataSource::Filesystem,
            MetadataConfidence::Medium,
        ),
        _ => (None, MetadataSource::Unknown, MetadataConfidence::Low),
    }
}

fn calculate_directory_size_limited(
    directory: &Path,
    timeout: Duration,
    max_entries: usize,
) -> Option<u64> {
    let started_at = Instant::now();
    let mut size = 0u64;
    let mut entries = 0usize;

    for entry in WalkDir::new(directory).into_iter().filter_map(Result::ok) {
        if started_at.elapsed() > timeout {
            return None;
        }
        entries += 1;
        if entries > max_entries {
            return None;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if let Ok(metadata) = entry.metadata() {
            size = size.saturating_add(metadata.len());
        }
    }

    Some(size)
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
    fn resolve_size_falls_back_to_filesystem_when_estimated_missing() {
        let temp_root = std::env::temp_dir().join(format!("rust-yu-test-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&temp_root).is_ok());
        let test_file = temp_root.join("data.bin");
        assert!(fs::write(&test_file, vec![1u8; 2048]).is_ok());

        let mut program = InstalledProgram::new("FsFallback".to_string(), InstallSource::Registry);
        program.install_location = Some(temp_root.to_string_lossy().to_string());
        enrich_program(&mut program);

        assert!(program.size.unwrap_or(0) >= 2048);
        assert_eq!(program.size_source, MetadataSource::Filesystem);

        let _ = fs::remove_dir_all(&temp_root);
    }
}
