use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use super::models::{Trace, TraceType, Confidence};
use std::path::Path;
use walkdir::WalkDir;

/// 扫描 AppData 痕迹
pub fn scan_appdata_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();
    let search_pattern = program_name.to_lowercase();

    // 扫描用户 AppData 目录
    if let Some(home) = dirs::home_dir() {
        // Roaming
        let roaming = home.join("AppData").join("Roaming");
        if roaming.exists() {
            scan_appdata_dir(&roaming, &search_pattern, &mut traces);
        }

        // Local
        let local = home.join("AppData").join("Local");
        if local.exists() {
            scan_appdata_dir(&local, &search_pattern, &mut traces);
        }

        // LocalLow
        let local_low = home.join("AppData").join("LocalLow");
        if local_low.exists() {
            scan_appdata_dir(&local_low, &search_pattern, &mut traces);
        }
    }

    Ok(traces)
}

/// 扫描 AppData 目录
fn scan_appdata_dir(dir: &Path, pattern: &str, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(4)  // AppData 目录可能比较深
        .follow_links(false);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // 检查名称是否包含搜索模式
        if name.contains(pattern) {
            // 跳过某些系统目录
            if is_system_appdata_dir(path) {
                continue;
            }

            let is_dir = path.is_dir();
            let trace_type = if is_dir {
                TraceType::AppData
            } else {
                TraceType::AppData
            };

            let size = if path.is_file() {
                path.metadata().ok().map(|m| m.len())
            } else {
                // 计算目录大小
                calculate_size(path)
            };

            let description = if is_dir {
                format!("用户数据目录")
            } else {
                size.map(|s| format!("文件大小: {}", utils::format_size(s)))
                    .unwrap_or_else(|| "用户数据文件".to_string())
            };

            let confidence = if name.starts_with(pattern) || name.to_lowercase() == pattern.to_lowercase() {
                Confidence::High
            } else {
                Confidence::Medium
            };

            let mut trace = Trace::new(
                pattern.to_string(),
                trace_type,
                path.to_string_lossy().to_string(),
            )
            .with_description(description)
            .with_confidence(confidence);

            if let Some(s) = size {
                trace.size = Some(s);
            }

            traces.push(trace);
        }
    }
}

/// 检查是否为系统 AppData 目录
fn is_system_appdata_dir(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();

    let _system_dirs = [
        "microsoft",
        "windows",
        "google\\chrome",  // 浏览器数据通常很大，但不一定是要清理的
    ];

    // 只跳过真正的系统目录
    if path_str.contains("microsoft\\windows\\explorer") {
        return true;
    }

    false
}

/// 计算目录大小
fn calculate_size(path: &Path) -> Option<u64> {
    if !path.is_dir() {
        return path.metadata().ok().map(|m| m.len());
    }

    let mut size = 0u64;

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                size += meta.len();
            }
        }
    }

    Some(size)
}
