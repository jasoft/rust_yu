use crate::modules::common::error::UninstallerError;
use super::models::{Trace, TraceType, Confidence};
use std::path::Path;
use walkdir::WalkDir;

/// 扫描快捷方式痕迹
pub fn scan_shortcut_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();
    let search_pattern = program_name.to_lowercase();

    // 扫描位置
    let dirs_to_scan = get_shortcut_dirs();

    for dir in dirs_to_scan {
        if !dir.exists() {
            continue;
        }

        scan_shortcuts_in_dir(&dir, &search_pattern, &mut traces);
    }

    Ok(traces)
}

/// 获取快捷方式扫描目录
fn get_shortcut_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    // 用户桌面
    if let Some(home) = dirs::home_dir() {
        let desktop = home.join("Desktop");
        if desktop.exists() {
            dirs.push(desktop);
        }
    }

    // 公共桌面
    if let Ok(public) = std::env::var("Public") {
        let desktop = Path::new(&public).join("Desktop");
        if desktop.exists() {
            dirs.push(desktop);
        }
    }

    // 开始菜单 - 当前用户
    if let Some(home) = dirs::home_dir() {
        let start_menu = home.join("AppData")
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");

        if start_menu.exists() {
            dirs.push(start_menu);
        }
    }

    // 开始菜单 - 所有用户
    if let Ok(program_data) = std::env::var("ProgramData") {
        let start_menu = Path::new(&program_data)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");

        if start_menu.exists() {
            dirs.push(start_menu);
        }
    }

    dirs
}

/// 在目录中扫描快捷方式
fn scan_shortcuts_in_dir(dir: &Path, pattern: &str, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(3)
        .follow_links(false);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // 只处理 .lnk 文件
        if path.extension().map(|e| e == "lnk").unwrap_or(false) {
            let name = path.file_stem()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            // 检查名称是否包含搜索模式
            if name.contains(pattern) {
                let description = get_shortcut_description(path);

                let confidence = if name.starts_with(pattern) {
                    Confidence::High
                } else {
                    Confidence::Medium
                };

                let trace = Trace::new(
                    pattern.to_string(),
                    TraceType::Shortcut,
                    path.to_string_lossy().to_string(),
                )
                .with_description(description)
                .with_confidence(confidence);

                traces.push(trace);
            }
        }
    }
}

/// 获取快捷方式描述
fn get_shortcut_description(path: &Path) -> String {
    // 简单返回文件名
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "快捷方式".to_string())
}
