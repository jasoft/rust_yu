use super::models::{Confidence, Trace, TraceType};
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::lister::models::InstalledProgram;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAX_INSTALL_LOCATION_DEPTH: usize = 32;
const MAX_INSTALL_LOCATION_ITEMS: usize = 10_000;

/// 扫描文件系统痕迹
pub fn scan_filesystem_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();
    let search_pattern = program_name.to_lowercase();

    // 扫描目录
    let dirs_to_scan = get_scan_dirs();

    for dir in dirs_to_scan {
        if !dir.exists() {
            continue;
        }

        let dir_str = dir.to_string_lossy().to_string();
        tracing::debug!("扫描目录: {}", dir_str);

        // 扫描目录
        scan_directory(&dir, &search_pattern, &mut traces);
    }

    Ok(traces)
}

/// 扫描普通文件系统位置，并补充注册表明确记录的安装目录内容。
///
/// 通用名称扫描只能命中目录本身；卸载器删掉主程序后，剩余日志、配置等文件
/// 往往不再包含产品名。安装位置来自卸载前快照，因此可以在严格限制范围后逐项展示，
/// 但仍不自动选择，最终删除继续由确认与备份流程保护。
pub fn scan_filesystem_traces_for_program(
    program: &InstalledProgram,
) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = scan_filesystem_traces(&program.name)?;
    let Some(raw_install_location) = program.install_location.as_deref() else {
        return Ok(traces);
    };
    let Some(install_location) = safe_install_location(raw_install_location) else {
        tracing::warn!(
            install_location = raw_install_location,
            "安装位置范围过大或属于系统目录，跳过目录内残留扫描"
        );
        return Ok(traces);
    };

    // 避免同时展示安装根目录和其中的文件；两者被一起勾选会形成重叠删除目标。
    traces.retain(|trace| !same_path(Path::new(&trace.path), &install_location));
    append_install_location_contents(&program.name, &install_location, &mut traces);
    Ok(traces)
}

fn safe_install_location(raw_install_location: &str) -> Option<PathBuf> {
    let trimmed = raw_install_location.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    let path = PathBuf::from(trimmed);
    let safety_path = strip_extended_prefix(&path);
    if !path.is_absolute()
        || !path.is_dir()
        || path.parent().is_none()
        || utils::is_system_critical_path(&safety_path.to_string_lossy())
        || is_shared_system_root(&safety_path)
    {
        return None;
    }

    let forbidden_names = [
        "windows",
        "system32",
        "syswow64",
        "winsxs",
        "program files",
        "program files (x86)",
        "programdata",
        "users",
        "public",
        "appdata",
        "local",
        "locallow",
        "roaming",
        "windowsapps",
        "common files",
    ];
    let leaf = path
        .file_name()
        .map(|name| name.to_string_lossy().trim().to_lowercase())
        .unwrap_or_default();
    if forbidden_names.contains(&leaf.as_str()) {
        return None;
    }
    if dirs::home_dir().is_some_and(|home| same_path(&home, &safety_path)) {
        return None;
    }

    Some(path)
}

fn strip_extended_prefix(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if value.len() >= 8 && value[..8].eq_ignore_ascii_case("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{}", &value[8..]));
    }
    if value.len() >= 4 && value[..4].eq_ignore_ascii_case("\\\\?\\") {
        return PathBuf::from(&value[4..]);
    }
    path.to_path_buf()
}

fn is_shared_system_root(path: &Path) -> bool {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "Public",
        "CommonProgramFiles",
        "CommonProgramFiles(x86)",
    ]
    .into_iter()
    .filter_map(|name| std::env::var(name).ok())
    .map(PathBuf::from)
    .any(|root| {
        same_path(path, &root)
            || (name_is_common_files_root(&root)
                && normalize_path(path).starts_with(&format!("{}\\", normalize_path(&root))))
    })
}

fn name_is_common_files_root(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("Common Files"))
        .unwrap_or(false)
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn append_install_location_contents(program_name: &str, root: &Path, traces: &mut Vec<Trace>) {
    let mut item_count = 0usize;
    for entry in WalkDir::new(root)
        .min_depth(1)
        .max_depth(MAX_INSTALL_LOCATION_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        if item_count >= MAX_INSTALL_LOCATION_ITEMS {
            tracing::warn!(
                install_location = %root.display(),
                limit = MAX_INSTALL_LOCATION_ITEMS,
                "安装目录残留数量超过安全展示上限，已截断"
            );
            break;
        }

        let path = entry.path();
        let size = entry.metadata().ok().map(|metadata| metadata.len());
        let description = size
            .map(|value| format!("安装目录残留文件，大小: {}", utils::format_size(value)))
            .unwrap_or_else(|| "安装目录残留文件".to_string());
        let mut trace = Trace::new(
            program_name.to_string(),
            TraceType::File,
            path.to_string_lossy().into_owned(),
        )
        .with_description(description)
        .with_confidence(Confidence::High);
        trace.size = size;
        traces.push(trace);
        item_count += 1;
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

/// 获取需要扫描的目录
fn get_scan_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    // Program Files
    if let Ok(pf) = std::env::var("ProgramFiles") {
        dirs.push(Path::new(&pf).to_path_buf());
    }

    // Program Files (x86)
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(Path::new(&pf86).to_path_buf());
    }

    // 公共文档
    if let Ok(public) = std::env::var("Public") {
        let docs = Path::new(&public).join("Documents");
        if docs.exists() {
            dirs.push(docs);
        }
    }

    // 用户桌面
    if let Some(home) = dirs::home_dir() {
        let desktop = home.join("Desktop");
        if desktop.exists() {
            dirs.push(desktop);
        }
    }

    // ProgramData
    if let Ok(program_data) = std::env::var("ProgramData") {
        dirs.push(Path::new(&program_data).to_path_buf());
    }

    dirs
}

/// 扫描目录
fn scan_directory(dir: &Path, pattern: &str, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(3) // 限制深度
        .follow_links(false);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // 检查名称是否包含搜索模式
        if name.contains(pattern) {
            // 跳过系统目录
            if is_system_dir(path) {
                continue;
            }

            let trace_type = if path.is_dir() {
                TraceType::File
            } else if path.extension().map(|e| e == "lnk").unwrap_or(false) {
                TraceType::Shortcut
            } else {
                TraceType::File
            };

            let size = if path.is_file() {
                path.metadata().ok().map(|m| m.len())
            } else {
                None
            };

            let description = if path.is_dir() {
                format!(
                    "目录: {} 个项目",
                    entry.metadata().ok().map(|m| m.len()).unwrap_or(0)
                )
            } else {
                size.map(|s| format!("文件大小: {}", utils::format_size(s)))
                    .unwrap_or_default()
            };

            let confidence = if name.starts_with(pattern) || name == pattern {
                Confidence::High
            } else {
                Confidence::Medium
            };

            let trace = Trace::new(
                pattern.to_string(),
                trace_type,
                path.to_string_lossy().to_string(),
            )
            .with_description(description)
            .with_confidence(confidence);

            // 如果是文件，设置大小
            if let Some(s) = size {
                traces.push(trace.with_size(s));
            } else {
                traces.push(trace);
            }
        }
    }
}

/// 检查是否为系统目录
fn is_system_dir(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_uppercase();

    let system_dirs = [
        "WINDOWS", "SYSTEM32", "SYSWOW64", "WINSXS", "INF", "DRIVERS",
    ];

    for sys_dir in &system_dirs {
        if path_str.contains(sys_dir) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{append_install_location_contents, safe_install_location, strip_extended_prefix};
    use std::fs;

    #[test]
    fn program_scan_surfaces_nested_files_from_recorded_install_location() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-install-residue-test-{}",
            uuid::Uuid::new_v4()
        ));
        let install_location = root.join("Demo App");
        let leftover = install_location.join("logs").join("leftover.log");
        fs::create_dir_all(leftover.parent().unwrap_or(&install_location))
            .unwrap_or_else(|error| panic!("create residue fixture: {error}"));
        fs::write(&leftover, b"leftover")
            .unwrap_or_else(|error| panic!("write residue fixture: {error}"));

        let mut traces = Vec::new();
        append_install_location_contents("Demo App", &install_location, &mut traces);

        assert!(traces
            .iter()
            .any(|trace| trace.path.ends_with("leftover.log")));
        assert!(!traces.iter().any(|trace| trace
            .path
            .eq_ignore_ascii_case(install_location.to_string_lossy().as_ref())));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn broad_shared_directory_is_not_a_safe_install_location() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-install-scope-test-{}",
            uuid::Uuid::new_v4()
        ));
        let broad = root.join("Program Files");
        fs::create_dir_all(&broad).unwrap_or_else(|error| panic!("create scope fixture: {error}"));

        assert!(safe_install_location(&broad.to_string_lossy()).is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extended_windows_prefix_is_removed_before_safety_checks() {
        assert_eq!(
            strip_extended_prefix(std::path::Path::new(r"\\?\C:\Windows\System32")),
            std::path::PathBuf::from(r"C:\Windows\System32")
        );
        assert_eq!(
            strip_extended_prefix(std::path::Path::new(r"\\?\UNC\server\share\Demo")),
            std::path::PathBuf::from(r"\\server\share\Demo")
        );
    }
}
