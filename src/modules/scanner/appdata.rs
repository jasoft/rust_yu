use super::models::{ConfidenceReason, Trace, TraceType};
use super::scope::{is_protected_appdata_path, is_user_program_install_path, ScanIdentity};
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::lister::models::{InstallSource, InstalledProgram};
use std::path::{Path, PathBuf};
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

/// 扫描普通 AppData 候选，并精确补充 Store/MSIX 包族对应的用户数据目录。
pub fn scan_appdata_traces_for_program(
    program: &InstalledProgram,
) -> Result<Vec<Trace>, UninstallerError> {
    let identity = ScanIdentity::from_program(program);
    let mut traces = scan_appdata_traces_with_identity(&identity)?;
    if program.install_source != InstallSource::Store {
        return Ok(traces);
    }

    let Some(package_family) = program.id.strip_prefix("store:") else {
        return Ok(traces);
    };
    if !is_safe_package_family_name(package_family) {
        tracing::warn!(package_family, "Store 包族名格式异常，跳过用户数据目录分析");
        return Ok(traces);
    }

    let Some(local_appdata) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Ok(traces);
    };
    let package_root = local_appdata.join("Packages").join(package_family);
    if package_root.is_dir() {
        append_store_package_files(&program.name, &package_root, &mut traces);
    }
    Ok(traces)
}

fn is_safe_package_family_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.contains(['\\', '/', ':'])
        && value != "."
        && value != ".."
}

fn append_store_package_files(program_name: &str, root: &Path, traces: &mut Vec<Trace>) {
    const MAX_STORE_PACKAGE_DEPTH: usize = 32;
    const MAX_STORE_PACKAGE_ITEMS: usize = 10_000;

    let mut item_count = 0usize;
    for entry in WalkDir::new(root)
        .min_depth(1)
        .max_depth(MAX_STORE_PACKAGE_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        if item_count >= MAX_STORE_PACKAGE_ITEMS {
            tracing::warn!(
                package_data = %root.display(),
                limit = MAX_STORE_PACKAGE_ITEMS,
                "Store 用户数据数量超过安全展示上限，已截断"
            );
            break;
        }

        let size = entry.metadata().ok().map(|metadata| metadata.len());
        let description = size
            .map(|value| format!("Store 用户数据文件，大小: {}", utils::format_size(value)))
            .unwrap_or_else(|| "Store 用户数据文件".to_string());
        let mut trace = Trace::new(
            program_name.to_string(),
            TraceType::AppData,
            entry.path().to_string_lossy().into_owned(),
        )
        .with_description(description)
        .with_confidence_evidence(90, ConfidenceReason::StorePackageUserData);
        trace.size = size;
        // Packages 目录由 AppX/MSIX 运行时管理，只允许分析展示，禁止通用清理器直接删除。
        trace.is_critical = true;
        traces.push(trace);
        item_count += 1;
    }
}

/// 扫描 AppData 目录
fn scan_appdata_dir(dir: &Path, identity: &ScanIdentity, traces: &mut Vec<Trace>) {
    let walker = WalkDir::new(dir)
        .max_depth(4) // AppData 目录可能比较深
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == dir
                || (!is_protected_appdata_path(entry.path())
                    && !is_user_program_install_path(entry.path()))
        });

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
            let size = if path.is_file() {
                path.metadata().ok().map(|m| m.len())
            } else {
                // 计算目录大小
                calculate_size(path)
            };

            let description = if is_dir {
                "用户数据目录".to_string()
            } else {
                size.map(|s| format!("文件大小: {}", utils::format_size(s)))
                    .unwrap_or_else(|| "用户数据文件".to_string())
            };

            let mut trace = Trace::new(
                identity.display_name().to_string(),
                TraceType::AppData,
                path.to_string_lossy().to_string(),
            )
            .with_description(description)
            // 名称匹配只说明候选目录与程序标识一致；高置信度
            // 只来自卸载前记录的明确安装目录。
            .with_confidence_evidence(60, ConfidenceReason::ExactNameUserData);

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
        .with_confidence_evidence(60, ConfidenceReason::ExactNameUserData);
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
    use super::{
        append_store_package_files, is_safe_package_family_name, scan_appdata_dir, ScanIdentity,
    };
    use crate::modules::scanner::models::ConfidenceReason;
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

    #[test]
    fn store_package_family_user_data_is_visible_but_protected() {
        let root =
            std::env::temp_dir().join(format!("rust-yu-store-data-test-{}", uuid::Uuid::new_v4()));
        let file = root.join("LocalState").join("session.json");
        fs::create_dir_all(file.parent().unwrap_or(&root))
            .unwrap_or_else(|error| panic!("create Store data fixture: {error}"));
        fs::write(&file, b"fixture")
            .unwrap_or_else(|error| panic!("write Store data fixture: {error}"));

        let mut traces = Vec::new();
        append_store_package_files("Demo Store App", &root, &mut traces);

        assert_eq!(traces.len(), 1);
        assert!(traces[0].is_critical);
        assert_eq!(traces[0].confidence_score, 90);
        assert_eq!(
            traces[0].confidence_reason,
            ConfidenceReason::StorePackageUserData
        );
        assert!(is_safe_package_family_name("Demo.Store_abcd1234"));
        assert!(!is_safe_package_family_name(r"..\OtherPackage"));

        let _ = fs::remove_dir_all(root);
    }
}
