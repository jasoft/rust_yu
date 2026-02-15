use super::models::{Confidence, Trace, TraceType};
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use std::path::Path;
use walkdir::WalkDir;

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
