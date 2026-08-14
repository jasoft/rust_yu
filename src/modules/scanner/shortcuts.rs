use super::models::{Confidence, Trace, TraceType};
use super::scope::{is_protected_shared_path, ScanIdentity};
use crate::modules::common::error::UninstallerError;
use std::path::Path;
use walkdir::WalkDir;

/// 扫描快捷方式痕迹
pub fn scan_shortcut_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    scan_shortcut_traces_with_identity(&ScanIdentity::from_name(program_name))
}

pub fn scan_shortcut_traces_with_identity(
    identity: &ScanIdentity,
) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();

    // 扫描位置
    let dirs_to_scan = get_shortcut_dirs();

    for dir in dirs_to_scan {
        if !dir.exists() {
            continue;
        }

        scan_shortcuts_in_dir(&dir, identity, &mut traces);
    }

    Ok(traces)
}

/// 获取快捷方式扫描目录
fn get_shortcut_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    // 开始菜单 - 当前用户
    if let Some(home) = dirs::home_dir() {
        let start_menu = home
            .join("AppData")
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
fn scan_shortcuts_in_dir(dir: &Path, identity: &ScanIdentity, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(3)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == dir || !is_protected_shared_path(entry.path()));

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();

        // 只处理 .lnk 文件
        if path.extension().map(|e| e == "lnk").unwrap_or(false) {
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            // 快捷方式名称也必须完整匹配；桌面和固定任务栏等共享区
            // 不进入自动清理候选。
            if identity.matches_component(&name) {
                let description = get_shortcut_description(path);

                let trace = Trace::new(
                    identity.display_name().to_string(),
                    TraceType::Shortcut,
                    path.to_string_lossy().to_string(),
                )
                .with_description(description)
                .with_confidence(Confidence::Medium);

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

#[cfg(test)]
mod tests {
    use super::scan_shortcuts_in_dir;
    use crate::modules::scanner::scope::ScanIdentity;
    use std::fs;

    #[test]
    fn shortcut_scan_rejects_substring_and_shell_area_matches() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-shortcut-scope-test-{}",
            uuid::Uuid::new_v4()
        ));
        let shell = root
            .join("Microsoft")
            .join("Internet Explorer")
            .join("Quick Launch")
            .join("User Pinned")
            .join("TaskBar");
        fs::create_dir_all(&shell).unwrap_or_else(|error| panic!("create shell fixture: {error}"));
        fs::write(shell.join("Google Chrome.lnk"), b"shortcut")
            .unwrap_or_else(|error| panic!("write shell fixture: {error}"));

        let mut traces = Vec::new();
        scan_shortcuts_in_dir(&root, &ScanIdentity::from_name("Xplorer"), &mut traces);
        assert!(traces.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
