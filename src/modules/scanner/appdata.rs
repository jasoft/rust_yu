use super::models::{Confidence, Trace, TraceType};
use super::scope::{is_protected_appdata_path, ScanIdentity};
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use std::path::Path;
use walkdir::WalkDir;

/// 扫描 AppData 痕迹
pub fn scan_appdata_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    scan_appdata_traces_with_identity(&ScanIdentity::from_name(program_name))
}

pub fn scan_appdata_traces_with_identity(
    identity: &ScanIdentity,
) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();

    // 扫描用户 AppData 目录
    if let Some(home) = dirs::home_dir() {
        // Roaming
        let roaming = home.join("AppData").join("Roaming");
        if roaming.exists() {
            scan_appdata_dir(&roaming, identity, &mut traces);
        }

        // Local
        let local = home.join("AppData").join("Local");
        if local.exists() {
            scan_appdata_dir(&local, identity, &mut traces);
        }

        // LocalLow
        let local_low = home.join("AppData").join("LocalLow");
        if local_low.exists() {
            scan_appdata_dir(&local_low, identity, &mut traces);
        }
    }

    Ok(traces)
}

/// 扫描 AppData 目录
fn scan_appdata_dir(dir: &Path, identity: &ScanIdentity, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(4) // AppData 目录可能比较深
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == dir || !is_protected_appdata_path(entry.path()));

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        if identity.matches_component(&name) {
            let is_dir = path.is_dir();
            if is_dir && append_matched_directory_files(path, identity, traces) {
                // 已逐文件展示目录内容，避免同时生成父目录目标造成重叠删除。
                continue;
            }
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

            let mut trace = Trace::new(
                identity.display_name().to_string(),
                trace_type,
                path.to_string_lossy().to_string(),
            )
            .with_description(description)
            // 名称匹配只说明候选目录与程序标识一致；高置信度
            // 只来自卸载前记录的明确安装目录。
            .with_confidence(Confidence::Medium);

            if let Some(s) = size {
                trace.size = Some(s);
            }

            traces.push(trace);
        }
    }
}

fn append_matched_directory_files(
    path: &Path,
    identity: &ScanIdentity,
    traces: &mut Vec<Trace>,
) -> bool {
    const MAX_MATCHED_DIRECTORY_DEPTH: usize = 16;
    const MAX_MATCHED_DIRECTORY_ITEMS: usize = 10_000;

    let mut matched = false;
    for entry in WalkDir::new(path)
        .min_depth(1)
        .max_depth(MAX_MATCHED_DIRECTORY_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && !entry.file_type().is_symlink()
                && !is_protected_appdata_path(entry.path())
        })
        .take(MAX_MATCHED_DIRECTORY_ITEMS)
    {
        let size = entry.metadata().ok().map(|metadata| metadata.len());
        let description = size
            .map(|value| format!("用户数据文件，大小: {}", utils::format_size(value)))
            .unwrap_or_else(|| "用户数据文件".to_string());
        let mut trace = Trace::new(
            identity.display_name().to_string(),
            TraceType::AppData,
            entry.path().to_string_lossy().into_owned(),
        )
        .with_description(description)
        .with_confidence(Confidence::Medium);
        trace.size = size;
        traces.push(trace);
        matched = true;
    }
    matched
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

#[cfg(test)]
mod tests {
    use super::{scan_appdata_dir, ScanIdentity};
    use std::fs;

    #[test]
    fn appdata_matching_accepts_compact_legacy_directory_name() {
        let identity = ScanIdentity::from_name("RustYu Legacy Test App");

        assert!(identity.matches_component("rustyulegacytest"));
        assert!(!identity.matches_component("rusty"));
    }

    #[test]
    fn matched_appdata_directory_surfaces_nested_leftover_file() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-appdata-residue-test-{}",
            uuid::Uuid::new_v4()
        ));
        let appdata = root.join("RustYuLegacyTest");
        let leftover = appdata.join("Data").join("leftover-user-profile.json");
        fs::create_dir_all(leftover.parent().unwrap_or(&appdata))
            .unwrap_or_else(|error| panic!("create AppData fixture: {error}"));
        fs::write(&leftover, b"leftover")
            .unwrap_or_else(|error| panic!("write AppData fixture: {error}"));

        let mut traces = Vec::new();
        scan_appdata_dir(
            &root,
            &ScanIdentity::from_name("RustYu Legacy Test App"),
            &mut traces,
        );

        assert!(traces
            .iter()
            .any(|trace| trace.path.ends_with("leftover-user-profile.json")));
        assert!(!traces.iter().any(|trace| trace
            .path
            .eq_ignore_ascii_case(appdata.to_string_lossy().as_ref())));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appdata_matching_rejects_internet_explorer_for_xplorer() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-appdata-scope-test-{}",
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
        scan_appdata_dir(&root, &ScanIdentity::from_name("Xplorer"), &mut traces);
        assert!(traces.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
