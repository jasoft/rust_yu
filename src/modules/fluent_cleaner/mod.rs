mod models;
mod parser;

pub use models::{
    CleanSelection, CleanerCatalog, CleanerCleanItemResult, CleanerCleanResult,
    CleanerEntrySummary, CleanerScanResult, CleanerTarget, CleanerTargetKind,
};

use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use glob::{glob_with, MatchOptions, Pattern};
use models::{CleanerEntry, ExclusionKind, FileKeyFlag};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;
use winreg::enums::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, KEY_WRITE};
use winreg::RegKey;

const DATABASE: &str = include_str!("../../../resources/fluent-cleaner/Winapp2.ini");
const MAX_SCAN_TARGETS: usize = 30_000;
const PROTECTED_SEGMENTS: &[&str] = &[r"\IndexedDB\chrome-extension_"];
static ENTRIES: OnceLock<Vec<CleanerEntry>> = OnceLock::new();

pub fn list_detected_entries() -> Result<CleanerCatalog, UninstallerError> {
    let all_entries = entries();
    let detected = all_entries
        .iter()
        .filter(|entry| is_entry_detected(entry))
        .map(|entry| CleanerEntrySummary {
            id: entry.id.clone(),
            name: entry.name.clone(),
            category: entry.category.clone(),
            warning: entry.warning.clone(),
            default_enabled: entry.default_enabled,
            file_rule_count: entry.file_keys.len(),
            registry_rule_count: entry.registry_keys.len(),
        })
        .collect::<Vec<_>>();
    Ok(CleanerCatalog {
        detected_rule_count: detected.len(),
        total_rule_count: all_entries.len(),
        database_version: parser::database_version(DATABASE),
        entries: detected,
    })
}

pub fn analyze_entries(entry_ids: &[String]) -> Result<CleanerScanResult, UninstallerError> {
    let requested = entry_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    if requested.is_empty() {
        return Ok(CleanerScanResult {
            targets: Vec::new(),
            total_bytes: 0,
            truncated: false,
        });
    }
    let selected = entries()
        .iter()
        .filter(|entry| requested.contains(entry.id.as_str()))
        .collect::<Vec<_>>();
    if selected.len() != requested.len() {
        return Err(UninstallerError::NotFound(
            "清理规则已变化，请刷新后重试".to_string(),
        ));
    }
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;
    for entry in selected {
        analyze_entry(entry, &mut targets, &mut seen, &mut truncated);
        if truncated {
            break;
        }
    }
    let total_bytes = targets.iter().map(|target| target.size).sum();
    Ok(CleanerScanResult {
        targets,
        total_bytes,
        truncated,
    })
}

pub fn clean_selection_with_progress<F>(
    selection: CleanSelection,
    mut on_item: F,
) -> Result<CleanerCleanResult, UninstallerError>
where
    F: FnMut(&CleanerCleanItemResult),
{
    if !selection.dry_run && !selection.confirm {
        return Err(UninstallerError::PermissionDenied(
            "执行系统清理前必须明确确认".to_string(),
        ));
    }

    // 删除前重新运行只读分析，不信任前端回传的路径，避免目标被篡改或预览结果过期。
    let fresh_scan = analyze_entries(&selection.entry_ids)?;
    let requested = selection
        .target_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let selected = fresh_scan
        .targets
        .into_iter()
        .filter(|target| requested.contains(target.id.as_str()))
        .collect::<Vec<_>>();
    if selected.len() != requested.len() {
        return Err(UninstallerError::NotFound(
            "部分目标已变化，请重新分析并确认".to_string(),
        ));
    }
    if selected
        .iter()
        .any(|target| target.blocked_reason.is_some())
    {
        return Err(UninstallerError::CriticalSystemItem(
            "选择中包含受保护目标".to_string(),
        ));
    }
    if !selection.dry_run && selected.iter().any(|target| target.requires_admin) {
        utils::ensure_running_as_administrator().map_err(|_| {
            UninstallerError::PermissionDenied(
                "所选目标需要管理员权限，请以管理员身份重新运行".to_string(),
            )
        })?;
    }

    let mut items = Vec::with_capacity(selected.len());
    let mut bytes_freed = 0_u64;
    for target in selected {
        let outcome = if selection.dry_run {
            Ok(target.size)
        } else {
            delete_target(&target)
        };
        let item = match outcome {
            Ok(bytes) => {
                if !selection.dry_run {
                    bytes_freed = bytes_freed.saturating_add(bytes);
                }
                CleanerCleanItemResult {
                    target_id: target.id,
                    path: target.path,
                    success: true,
                    error: None,
                    bytes_freed: if selection.dry_run { 0 } else { bytes },
                    dry_run: selection.dry_run,
                }
            }
            Err(error) => CleanerCleanItemResult {
                target_id: target.id,
                path: target.path,
                success: false,
                error: Some(error.to_string()),
                bytes_freed: 0,
                dry_run: selection.dry_run,
            },
        };
        on_item(&item);
        items.push(item);
    }
    Ok(CleanerCleanResult { items, bytes_freed })
}

fn entries() -> &'static [CleanerEntry] {
    ENTRIES.get_or_init(|| parser::parse_database(DATABASE))
}

fn is_entry_detected(entry: &CleanerEntry) -> bool {
    entry.detect_keys.iter().any(|path| registry_exists(path))
        || entry
            .detect_files
            .iter()
            .any(|path| expanded_path_exists(path))
}

fn expanded_path_exists(raw_path: &str) -> bool {
    let expanded = expand_variables(raw_path);
    if contains_wildcard(&expanded) {
        resolve_paths(raw_path).iter().any(|path| path.exists())
    } else {
        Path::new(&expanded).exists()
    }
}

fn analyze_entry(
    entry: &CleanerEntry,
    targets: &mut Vec<CleanerTarget>,
    seen: &mut HashSet<String>,
    truncated: &mut bool,
) {
    let exclusions = build_exclusions(entry);
    for file_key in &entry.file_keys {
        for directory in resolve_paths(&file_key.path) {
            if !directory.is_dir() {
                continue;
            }
            let candidates: Box<dyn Iterator<Item = PathBuf>> =
                if file_key.flag == FileKeyFlag::None {
                    match std::fs::read_dir(&directory) {
                        Ok(items) => Box::new(items.filter_map(Result::ok).map(|item| item.path())),
                        Err(_) => Box::new(std::iter::empty()),
                    }
                } else {
                    Box::new(
                        WalkDir::new(&directory)
                            .follow_links(false)
                            .into_iter()
                            .filter_map(Result::ok)
                            .filter(|item| item.file_type().is_file())
                            .map(|item| item.into_path()),
                    )
                };
            for path in candidates.filter(|path| path.is_file()) {
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !file_key
                    .patterns
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, file_name))
                    || is_excluded(&path, &exclusions)
                {
                    continue;
                }
                let path_text = path.to_string_lossy().into_owned();
                if !seen.insert(format!("file:{}", path_text.to_ascii_lowercase())) {
                    continue;
                }
                let size = path.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                push_target(
                    targets,
                    CleanerTarget {
                        id: target_id(CleanerTargetKind::File, &path_text, None),
                        entry_id: entry.id.clone(),
                        entry_name: entry.name.clone(),
                        kind: CleanerTargetKind::File,
                        path: path_text.clone(),
                        value_name: None,
                        size,
                        requires_admin: file_requires_admin(&path),
                        blocked_reason: blocked_file_reason(&path_text),
                    },
                    truncated,
                );
                if *truncated {
                    return;
                }
            }
        }
    }
    for registry_key in &entry.registry_keys {
        if !registry_item_exists(&registry_key.path, registry_key.value_name.as_deref()) {
            continue;
        }
        let kind = if registry_key.value_name.is_some() {
            CleanerTargetKind::RegistryValue
        } else {
            CleanerTargetKind::RegistryKey
        };
        let dedupe = format!(
            "registry:{}|{}",
            registry_key.path.to_ascii_lowercase(),
            registry_key
                .value_name
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
        );
        if !seen.insert(dedupe) {
            continue;
        }
        push_target(
            targets,
            CleanerTarget {
                id: target_id(kind, &registry_key.path, registry_key.value_name.as_deref()),
                entry_id: entry.id.clone(),
                entry_name: entry.name.clone(),
                kind,
                path: registry_key.path.clone(),
                value_name: registry_key.value_name.clone(),
                size: 0,
                requires_admin: !registry_key.path.to_ascii_uppercase().starts_with("HKCU\\"),
                blocked_reason: blocked_registry_reason(
                    &registry_key.path,
                    registry_key.value_name.as_deref(),
                ),
            },
            truncated,
        );
        if *truncated {
            return;
        }
    }
}

fn push_target(targets: &mut Vec<CleanerTarget>, target: CleanerTarget, truncated: &mut bool) {
    if targets.len() >= MAX_SCAN_TARGETS {
        *truncated = true;
    } else {
        targets.push(target);
    }
}

fn build_exclusions(entry: &CleanerEntry) -> Vec<(PathBuf, Option<String>)> {
    entry
        .exclusions
        .iter()
        .filter(|item| item.kind != ExclusionKind::Registry)
        .flat_map(|item| {
            resolve_paths(&item.path)
                .into_iter()
                .map(|path| (path, item.pattern.clone()))
        })
        .collect()
}

fn is_excluded(path: &Path, exclusions: &[(PathBuf, Option<String>)]) -> bool {
    exclusions.iter().any(|(directory, pattern)| {
        let Ok(relative) = path.strip_prefix(directory) else {
            return false;
        };
        match pattern {
            None => true,
            Some(pattern) if contains_wildcard(pattern) => path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| wildcard_matches(pattern, name)),
            Some(pattern) => {
                relative.components().count() == 1
                    && relative.to_string_lossy().eq_ignore_ascii_case(pattern)
            }
        }
    })
}

fn expand_variables(raw_path: &str) -> String {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let windows = std::env::var("SystemRoot")
        .or_else(|_| std::env::var("WINDIR"))
        .unwrap_or_default();
    let drive = Path::new(&windows)
        .components()
        .next()
        .map(|value| value.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| "C:".to_string());
    let values = HashMap::from([
        ("AppData", std::env::var("APPDATA").unwrap_or_default()),
        ("LocalAppData", local.clone()),
        (
            "LocalLowAppData",
            Path::new(&local)
                .join("..\\LocalLow")
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "ProgramFiles",
            std::env::var("ProgramW6432")
                .or_else(|_| std::env::var("ProgramFiles"))
                .unwrap_or_default(),
        ),
        (
            "ProgramFiles(x86)",
            std::env::var("ProgramFiles(x86)").unwrap_or_default(),
        ),
        (
            "ProgramFilesX86",
            std::env::var("ProgramFiles(x86)").unwrap_or_default(),
        ),
        (
            "CommonProgramFiles",
            std::env::var("CommonProgramFiles").unwrap_or_default(),
        ),
        (
            "ProgramData",
            std::env::var("ProgramData").unwrap_or_default(),
        ),
        (
            "CommonAppData",
            std::env::var("ProgramData").unwrap_or_default(),
        ),
        (
            "UserProfile",
            std::env::var("USERPROFILE").unwrap_or_default(),
        ),
        ("Public", std::env::var("PUBLIC").unwrap_or_default()),
        ("UserName", std::env::var("USERNAME").unwrap_or_default()),
        ("WinDir", windows.clone()),
        ("SystemRoot", windows),
        ("SystemDrive", drive),
        ("Temp", std::env::temp_dir().to_string_lossy().into_owned()),
    ]);
    let mut expanded = raw_path.to_string();
    for (name, value) in values {
        if !value.is_empty() {
            expanded = replace_ignore_ascii_case(&expanded, &format!("%{name}%"), &value);
        }
    }
    expanded
}

fn resolve_paths(raw_path: &str) -> Vec<PathBuf> {
    let mut variants = vec![raw_path.to_string()];
    if raw_path.to_ascii_lowercase().contains("%programfiles%") {
        variants.push(replace_ignore_ascii_case(
            raw_path,
            "%ProgramFiles%",
            "%ProgramFiles(x86)%",
        ));
    }
    let mut results = HashSet::new();
    for variant in variants {
        let expanded = expand_variables(&variant);
        if contains_wildcard(&expanded) {
            if let Ok(paths) = glob_with(&expanded, match_options()) {
                results.extend(paths.filter_map(Result::ok));
            }
        } else {
            results.insert(PathBuf::from(expanded));
        }
    }
    results.into_iter().collect()
}

fn match_options() -> MatchOptions {
    MatchOptions {
        case_sensitive: false,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    Pattern::new(if pattern == "*.*" { "*" } else { pattern })
        .is_ok_and(|compiled| compiled.matches_with(value, match_options()))
}

fn contains_wildcard(value: &str) -> bool {
    value.contains('*') || value.contains('?')
}

fn replace_ignore_ascii_case(source: &str, needle: &str, replacement: &str) -> String {
    let lower_source = source.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut output = String::new();
    let mut start = 0;
    while let Some(offset) = lower_source[start..].find(&lower_needle) {
        let index = start + offset;
        output.push_str(&source[start..index]);
        output.push_str(replacement);
        start = index + needle.len();
    }
    output.push_str(&source[start..]);
    output
}

fn registry_exists(raw_path: &str) -> bool {
    let (path, value) = raw_path
        .rsplit_once('|')
        .map_or((raw_path, None), |(path, value)| (path, Some(value)));
    registry_item_exists(path, value)
}

fn registry_item_exists(path: &str, value_name: Option<&str>) -> bool {
    let Some((root, subkey)) = parse_registry_path(path) else {
        return false;
    };
    [
        KEY_READ,
        KEY_READ | KEY_WOW64_64KEY,
        KEY_READ | KEY_WOW64_32KEY,
    ]
    .into_iter()
    .any(|view| {
        root.open_subkey_with_flags(subkey, view).is_ok_and(|key| {
            value_name.is_none() || value_name.is_some_and(|name| key.get_raw_value(name).is_ok())
        })
    })
}

fn parse_registry_path(path: &str) -> Option<(RegKey, &str)> {
    let (hive, subkey) = path.split_once('\\')?;
    let root = match hive.to_ascii_uppercase().as_str() {
        "HKCU" | "HKEY_CURRENT_USER" => RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
        "HKLM" | "HKEY_LOCAL_MACHINE" => RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        "HKCR" | "HKEY_CLASSES_ROOT" => RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT),
        "HKU" | "HKEY_USERS" => RegKey::predef(winreg::enums::HKEY_USERS),
        _ => return None,
    };
    Some((root, subkey))
}

fn target_id(kind: CleanerTargetKind, path: &str, value_name: Option<&str>) -> String {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    path.to_ascii_lowercase().hash(&mut hasher);
    value_name
        .unwrap_or_default()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    format!("target-{:016x}", hasher.finish())
}

fn blocked_file_reason(path: &str) -> Option<String> {
    if PROTECTED_SEGMENTS.iter().any(|segment| {
        path.to_ascii_lowercase()
            .contains(&segment.to_ascii_lowercase())
    }) {
        return Some("受保护的浏览器扩展数据".to_string());
    }
    let trace = Trace::new(
        "System Cleaner".to_string(),
        TraceType::File,
        path.to_string(),
    )
    .with_confidence(Confidence::High);
    crate::modules::cleaner::safety::pre_delete_check(&trace)
        .err()
        .map(|error| error.to_string())
}

fn blocked_registry_reason(path: &str, value_name: Option<&str>) -> Option<String> {
    let kind = if value_name.is_some() {
        TraceType::RegistryValue
    } else {
        TraceType::RegistryKey
    };
    let display = value_name.map_or_else(|| path.to_string(), |name| format!("{path}\\{name}"));
    let trace =
        Trace::new("System Cleaner".to_string(), kind, display).with_confidence(Confidence::High);
    crate::modules::cleaner::safety::pre_delete_check(&trace)
        .err()
        .map(|error| error.to_string())
}

fn file_requires_admin(path: &Path) -> bool {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "SystemRoot",
    ]
    .iter()
    .filter_map(|name| std::env::var(name).ok())
    .any(|root| {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .starts_with(&root.to_ascii_lowercase())
    })
}

fn delete_target(target: &CleanerTarget) -> Result<u64, UninstallerError> {
    match target.kind {
        CleanerTargetKind::File => {
            let path = Path::new(&target.path);
            if !path.exists() {
                return Ok(0);
            }
            if !path.is_file() {
                return Err(UninstallerError::CriticalSystemItem(
                    "仅允许删除预览中的单个文件".to_string(),
                ));
            }
            let size = path.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            // 只删除预览中重新验证过的文件，不递归删除目录，降低错误规则的破坏范围。
            std::fs::remove_file(path)?;
            Ok(size)
        }
        CleanerTargetKind::RegistryKey | CleanerTargetKind::RegistryValue => {
            delete_registry_target(target)?;
            Ok(0)
        }
    }
}

fn delete_registry_target(target: &CleanerTarget) -> Result<(), UninstallerError> {
    let Some((root, subkey)) = parse_registry_path(&target.path) else {
        return Err(UninstallerError::Registry("无效的注册表路径".to_string()));
    };
    if let Some(value_name) = target.value_name.as_deref() {
        let key = root
            .open_subkey_with_flags(subkey, KEY_WRITE)
            .map_err(|error| UninstallerError::Registry(error.to_string()))?;
        key.delete_value(value_name)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|error| UninstallerError::Registry(error.to_string()))
    } else {
        root.delete_subkey_all(subkey)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|error| UninstallerError::Registry(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{entries, parser, target_id, wildcard_matches, CleanerTargetKind, DATABASE};

    #[test]
    fn bundled_database_is_complete() {
        assert_eq!(parser::database_version(DATABASE), "260730");
        assert!(entries().len() >= 4_000);
    }

    #[test]
    fn target_ids_are_case_insensitive() {
        assert_eq!(
            target_id(CleanerTargetKind::File, r"C:\Temp\a.log", None),
            target_id(CleanerTargetKind::File, r"c:\temp\A.LOG", None)
        );
        assert!(wildcard_matches("*.*", "LOCK"));
    }
}
