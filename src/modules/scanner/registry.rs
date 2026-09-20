use super::filesystem::safe_install_location;
use super::models::{ConfidenceReason, Trace, TraceType};
use super::scope::ScanIdentity;
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils::split_command_for_spawn;
use crate::modules::lister::models::InstalledProgram;
use crate::modules::lister::registry::format_registry_value;
use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const MAX_DEPTH: u32 = 5;
const MAX_REFERENCE_KEYS_PER_ROOT: usize = 50_000;
const MAX_EXACT_CLASS_KEYS: usize = 5_000;

pub struct RegistryScanReport {
    pub traces: Vec<Trace>,
    pub truncated: bool,
}

/// 扫描注册表痕迹。
///
/// 保留这个名称兼容强制卸载入口，但实际匹配已经改为组件级精确匹配。
pub fn scan_registry_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    scan_registry_traces_for_identity(&ScanIdentity::from_name(program_name))
}

pub fn scan_registry_traces_for_identity(
    identity: &ScanIdentity,
) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();

    // 不再递归 HKCR 全树；Classes/Explorer/Shell Folders 等键是共享系统
    // 状态，名称相似绝不能成为删除依据。
    let search_paths: Vec<(winreg::HKEY, &str)> = vec![
        (HKEY_LOCAL_MACHINE, r"SOFTWARE"),
        (HKEY_CURRENT_USER, r"SOFTWARE"),
    ];

    for (hkey, path) in &search_paths {
        if let Err(error) = scan_registry_key(*hkey, path, identity, &mut traces, 0) {
            tracing::debug!("扫描注册表路径 {} 失败: {}", path, error);
        }
    }

    scan_uninstall_keys(identity, &mut traces);
    Ok(traces)
}

/// 在身份精确匹配之外，补充“注册表值明确引用安装目录”的证据。
/// 这些共享分支只做只读分析，不根据键名猜测归属。
pub fn scan_registry_traces_for_program(
    program: &InstalledProgram,
) -> Result<Vec<Trace>, UninstallerError> {
    Ok(scan_registry_traces_report_for_program(program)?.traces)
}

pub fn scan_registry_traces_report_for_program(
    program: &InstalledProgram,
) -> Result<RegistryScanReport, UninstallerError> {
    let identity = ScanIdentity::from_program(program);
    let mut traces = scan_registry_traces_for_identity(&identity)?;
    let roots = reference_roots(program);
    if roots.is_empty() {
        return Ok(RegistryScanReport {
            traces,
            truncated: false,
        });
    }

    scan_exact_class_roots(&identity, &roots, &mut traces);
    let mut truncated = false;
    for (hkey, path, max_depth, protected) in registry_reference_locations() {
        let mut visited = 0usize;
        scan_registry_reference_values(
            hkey,
            path,
            &roots,
            &identity,
            &mut traces,
            0,
            max_depth,
            protected,
            &mut visited,
        );
        if visited >= MAX_REFERENCE_KEYS_PER_ROOT {
            truncated = true;
            tracing::warn!(
                registry_path = path,
                "注册表引用扫描达到安全上限，结果可能被截断"
            );
        }
    }
    Ok(RegistryScanReport { traces, truncated })
}

fn reference_roots(program: &InstalledProgram) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(location) = program.install_location.as_deref() {
        if let Some(path) = safe_install_location(location) {
            candidates.push(path.to_string_lossy().into_owned());
        }
    }
    if let Some(command) = program.preferred_uninstall_string() {
        if let Ok((executable, _)) = split_command_for_spawn(command) {
            if let Some(parent) = Path::new(&executable).parent() {
                if let Some(path) = safe_install_location(&parent.to_string_lossy()) {
                    candidates.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }
    candidates.sort_by_key(|value| std::cmp::Reverse(value.len()));
    candidates.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    candidates
}

fn scan_exact_class_roots(identity: &ScanIdentity, roots: &[String], traces: &mut Vec<Trace>) {
    let locations = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Classes"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Classes"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Classes"),
    ];
    for (hkey, base_path) in locations {
        let Ok(base) = RegKey::predef(hkey).open_subkey(base_path) else {
            continue;
        };
        for name in base.enum_keys().filter_map(Result::ok) {
            if !identity.matches_component(&name) {
                continue;
            }
            let mut visited = 0usize;
            scan_owned_class_tree(
                hkey,
                &format!("{base_path}\\{name}"),
                roots,
                identity,
                traces,
                &mut visited,
            );
        }
    }
}

fn scan_owned_class_tree(
    hkey: winreg::HKEY,
    path: &str,
    roots: &[String],
    identity: &ScanIdentity,
    traces: &mut Vec<Trace>,
    visited: &mut usize,
) {
    if *visited >= MAX_EXACT_CLASS_KEYS {
        tracing::warn!(
            registry_path = path,
            "精确 Classes 应用树达到安全上限，已截断"
        );
        return;
    }
    let Ok(key) = RegKey::predef(hkey).open_subkey(path) else {
        return;
    };
    *visited += 1;
    let full_key_path = format!("{}\\{}", format_hkey(hkey), path);
    let mut key_trace = Trace::new(
        identity.display_name().to_string(),
        TraceType::RegistryKey,
        full_key_path.clone(),
    )
    .with_description("与程序身份完全一致的 Classes 注册表项".to_string())
    .with_confidence_evidence(60, ConfidenceReason::ExactNameRegistry);
    key_trace.is_critical = true;
    traces.push(key_trace);

    for result in key.enum_values() {
        let Ok((value_name, raw)) = result else {
            continue;
        };
        let value = format_registry_value(&raw);
        let related_root = roots.iter().find(|root| {
            crate::modules::system_integration::command_references_related_path(&value, root)
                || crate::modules::system_integration::command_references_related_path(
                    &value_name,
                    root,
                )
        });
        let display_name = if value_name.is_empty() {
            "(Default)"
        } else {
            &value_name
        };
        let preview = if value.chars().count() > 240 {
            format!("{}…", value.chars().take(240).collect::<String>())
        } else {
            value
        };
        let mut trace = Trace::new(
            identity.display_name().to_string(),
            TraceType::RegistryValue,
            format!("{full_key_path}\\{display_name}"),
        )
        .with_description(format!("{display_name} [{:?}] = {preview}", raw.vtype))
        .with_size(raw.bytes.len() as u64)
        .with_confidence_evidence(
            if related_root.is_some() { 90 } else { 60 },
            if related_root.is_some() {
                ConfidenceReason::RegistryValueReferencesInstallLocation
            } else {
                ConfidenceReason::ExactNameRegistry
            },
        );
        trace.related_path = related_root.cloned();
        trace.is_critical = true;
        traces.push(trace);
    }

    for name in key.enum_keys().filter_map(Result::ok) {
        if *visited >= MAX_EXACT_CLASS_KEYS {
            return;
        }
        scan_owned_class_tree(
            hkey,
            &format!("{path}\\{name}"),
            roots,
            identity,
            traces,
            visited,
        );
    }
}

fn registry_reference_locations() -> Vec<(winreg::HKEY, &'static str, u32, bool)> {
    vec![
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
            2,
            true,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths",
            2,
            true,
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
            2,
            false,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            0,
            false,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
            0,
            false,
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            0,
            false,
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
            0,
            false,
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Classes", 6, true),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Classes", 6, true),
        (HKEY_CURRENT_USER, r"SOFTWARE\Classes", 6, true),
        (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\FirewallRules",
            0,
            true,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            0,
            true,
        ),
        (HKEY_CURRENT_USER, r"Environment", 0, false),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs",
            0,
            true,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\Folders",
            0,
            true,
        ),
    ]
}

/// 判断注册表路径是否为 Explorer 的 MuiCache 键。
///
/// 该键保存每应用的 FriendlyAppName / ApplicationCompany 显示名缓存，
/// 值名是可执行文件完整路径；引用目标已确认为程序自身安装根时可以安全删除。
fn is_mui_cache_path(path: &str) -> bool {
    path.to_ascii_lowercase()
        .ends_with(r"shell\muicache")
}

#[allow(clippy::too_many_arguments)]
fn scan_registry_reference_values(
    hkey: winreg::HKEY,
    path: &str,
    roots: &[String],
    identity: &ScanIdentity,
    traces: &mut Vec<Trace>,
    depth: u32,
    max_depth: u32,
    protected: bool,
    visited: &mut usize,
) {
    if depth > max_depth || *visited >= MAX_REFERENCE_KEYS_PER_ROOT {
        return;
    }
    let Ok(key) = RegKey::predef(hkey).open_subkey(path) else {
        return;
    };
    *visited += 1;

    for result in key.enum_values() {
        let Ok((value_name, raw)) = result else {
            continue;
        };
        let value = format_registry_value(&raw);
        let Some((related_root, matched_value_name)) = roots.iter().find_map(|root| {
            if crate::modules::system_integration::command_references_related_path(&value, root) {
                Some((root, false))
            } else if crate::modules::system_integration::command_references_related_path(
                &value_name,
                root,
            ) {
                Some((root, true))
            } else {
                None
            }
        }) else {
            continue;
        };
        let display_name = if value_name.is_empty() {
            "(Default)"
        } else {
            &value_name
        };
        let preview = if value.chars().count() > 240 {
            format!("{}…", value.chars().take(240).collect::<String>())
        } else {
            value
        };
        let mut trace = Trace::new(
            identity.display_name().to_string(),
            TraceType::RegistryValue,
            format!("{}\\{}\\{}", format_hkey(hkey), path, display_name),
        )
        .with_description(format!(
            "注册表值明确引用安装目录: {display_name} [{:?}] = {preview}",
            raw.vtype
        ))
        .with_size(raw.bytes.len() as u64)
        .with_confidence_evidence(90, ConfidenceReason::RegistryValueReferencesInstallLocation)
        .with_related_path(related_root.clone());
        // Classes、机器级 App Paths、防火墙规则和默认值都属于共享契约；
        // 深度分析可以展示，但通用残留清理不得直接修改。
        trace.is_critical = protected || value_name.is_empty() || matched_value_name;
        // MuiCache 的值名就是可执行文件完整路径，FriendlyAppName /
        // ApplicationCompany 是每应用的显示名缓存；值已明确引用该程序自身的
        // 安装根，属于应用私有残留而不是共享契约，应当允许用户勾选清理，
        // 否则失效安装的这类残留永远无法从列表中移除。
        if is_mui_cache_path(path) && !value_name.is_empty() {
            trace.is_critical = false;
        }
        traces.push(trace);
    }

    if depth == max_depth {
        return;
    }
    for name in key.enum_keys().filter_map(Result::ok) {
        if *visited >= MAX_REFERENCE_KEYS_PER_ROOT {
            return;
        }
        scan_registry_reference_values(
            hkey,
            &format!("{path}\\{name}"),
            roots,
            identity,
            traces,
            depth + 1,
            max_depth,
            protected,
            visited,
        );
    }
}

/// 递归扫描注册表键。当前键名必须是完整身份组件，禁止任意子串命中。
fn scan_registry_key(
    hkey: winreg::HKEY,
    path: &str,
    identity: &ScanIdentity,
    traces: &mut Vec<Trace>,
    depth: u32,
) -> Result<(), UninstallerError> {
    if depth > MAX_DEPTH || is_protected_registry_path(hkey, path) {
        return Ok(());
    }

    let key = match RegKey::predef(hkey).open_subkey(path) {
        Ok(key) => key,
        Err(_) => return Ok(()),
    };

    let key_name = path.split('\\').last().unwrap_or("");
    if identity.matches_component(key_name) {
        let full_path = format!("{}\\{}", format_hkey(hkey), path);
        let description = if path.to_ascii_lowercase().contains("uninstall") {
            format!("卸载残留: {}", key_name)
        } else {
            format!("注册表项: {}", key_name)
        };
        let (score, reason) = if path.to_ascii_lowercase().contains("uninstall") {
            (100, ConfidenceReason::UninstallRegistry)
        } else {
            (60, ConfidenceReason::ExactNameRegistry)
        };

        traces.push(
            Trace::new(
                identity.display_name().to_string(),
                TraceType::RegistryKey,
                full_path.clone(),
            )
            .with_description(description)
            .with_confidence_evidence(score, reason),
        );
        append_matched_key_values(identity, &key, &full_path, traces, score, reason);
    }

    for name in key.enum_keys().filter_map(|key| key.ok()) {
        let subpath = format!("{}\\{}", path, name);
        let _ = scan_registry_key(hkey, &subpath, identity, traces, depth + 1);
    }

    Ok(())
}

fn append_matched_key_values(
    identity: &ScanIdentity,
    key: &RegKey,
    full_key_path: &str,
    traces: &mut Vec<Trace>,
    score: u8,
    reason: ConfidenceReason,
) {
    for result in key.enum_values() {
        let Ok((value_name, raw)) = result else {
            continue;
        };
        let display_name = if value_name.is_empty() {
            "(Default)"
        } else {
            &value_name
        };
        let value = format_registry_value(&raw);
        let preview = if value.chars().count() > 240 {
            format!("{}…", value.chars().take(240).collect::<String>())
        } else {
            value
        };
        let mut trace = Trace::new(
            identity.display_name().to_string(),
            TraceType::RegistryValue,
            format!("{full_key_path}\\{display_name}"),
        )
        .with_description(format!("{display_name} [{:?}] = {preview}", raw.vtype))
        .with_size(raw.bytes.len() as u64)
        .with_confidence_evidence(score, reason);
        trace.is_critical = value_name.is_empty() || value_name.contains('\\');
        traces.push(trace);
    }
}

/// 只接受卸载项键名或 DisplayName 的完整身份匹配。
fn scan_uninstall_keys(identity: &ScanIdentity, traces: &mut Vec<Trace>) {
    let paths = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    for (hkey, path) in &paths {
        if let Ok(key) = RegKey::predef(*hkey).open_subkey(path) {
            for name in key.enum_keys().filter_map(|key| key.ok()) {
                if let Ok(subkey) = key.open_subkey(&name) {
                    let display_name: Option<String> = subkey.get_value("DisplayName").ok();
                    if !uninstall_entry_matches(identity, &name, display_name.as_deref()) {
                        continue;
                    }

                    let full_path = format!("{}\\{}\\{}", format_hkey(*hkey), path, name);
                    let install_location: Option<String> = subkey.get_value("InstallLocation").ok();
                    traces.push(
                        Trace::new(
                            identity.display_name().to_string(),
                            TraceType::RegistryKey,
                            full_path.clone(),
                        )
                        .with_description(format!(
                            "卸载信息: {} ({})",
                            display_name.unwrap_or_default(),
                            install_location.unwrap_or_default()
                        ))
                        .with_confidence_evidence(100, ConfidenceReason::UninstallRegistry),
                    );
                    append_uninstall_values(identity, &subkey, &full_path, traces);
                }
            }
        }
    }
}

fn append_uninstall_values(
    identity: &ScanIdentity,
    subkey: &RegKey,
    full_key_path: &str,
    traces: &mut Vec<Trace>,
) {
    for result in subkey.enum_values() {
        let Ok((value_name, raw)) = result else {
            continue;
        };
        // 默认值没有可安全编码到现有“键路径\值名”删除契约中的名称；详情页仍会
        // 完整展示它，但残留列表只生成具有明确值名的独立项目。
        if value_name.is_empty() || value_name.contains('\\') {
            continue;
        }
        let value = format_registry_value(&raw);
        let preview = if value.chars().count() > 240 {
            format!("{}…", value.chars().take(240).collect::<String>())
        } else {
            value
        };
        traces.push(
            Trace::new(
                identity.display_name().to_string(),
                TraceType::RegistryValue,
                format!("{full_key_path}\\{value_name}"),
            )
            .with_description(format!("{value_name} [{:?}] = {preview}", raw.vtype))
            .with_size(raw.bytes.len() as u64)
            .with_confidence_evidence(100, ConfidenceReason::UninstallRegistry),
        );
    }
}

fn uninstall_entry_matches(
    identity: &ScanIdentity,
    key_name: &str,
    display_name: Option<&str>,
) -> bool {
    identity.matches_component(key_name)
        || display_name.is_some_and(|value| identity.matches_display_name(value))
}

fn is_protected_registry_path(hkey: winreg::HKEY, path: &str) -> bool {
    if hkey == HKEY_CLASSES_ROOT {
        return true;
    }

    let normalized = path
        .replace('/', "\\")
        .split('\\')
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    normalized.windows(2).any(|window| {
        matches!(
            (window[0].as_str(), window[1].as_str()),
            ("software", "classes")
                | ("microsoft", "windows")
                | ("microsoft", "windows nt")
                | ("currentversion", "explorer")
                | ("currentversion", "shell folders")
                | ("currentversion", "user shell folders")
                | ("currentversion", "run")
                | ("currentversion", "runonce")
        )
    })
}

/// 格式化 HKEY 为字符串
fn format_hkey(hkey: winreg::HKEY) -> String {
    match hkey {
        HKEY_LOCAL_MACHINE => "HKLM".to_string(),
        HKEY_CURRENT_USER => "HKCU".to_string(),
        HKEY_CLASSES_ROOT => "HKCR".to_string(),
        HKEY_USERS => "HKU".to_string(),
        HKEY_CURRENT_CONFIG => "HKCC".to_string(),
        _ => format!("{:?}", hkey),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_matched_key_values, append_uninstall_values, is_protected_registry_path,
        scan_registry_reference_values, uninstall_entry_matches,
    };
    use crate::modules::scanner::models::{ConfidenceReason, TraceType};
    use crate::modules::scanner::scope::ScanIdentity;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    #[test]
    fn uninstall_matching_does_not_use_substrings() {
        let identity = ScanIdentity::from_name("Xplorer");
        assert!(uninstall_entry_matches(
            &identity,
            "Xplorer",
            Some("Xplorer 0.3.1")
        ));
        assert!(!uninstall_entry_matches(
            &identity,
            "InternetExplorer",
            Some("Internet Explorer")
        ));
    }

    #[test]
    fn protected_registry_branches_are_not_traversed() {
        assert!(is_protected_registry_path(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer"
        ));
        assert!(is_protected_registry_path(
            HKEY_CURRENT_USER,
            r"Software\Classes\CLSID"
        ));
        assert!(!is_protected_registry_path(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\DemoVendor\Xplorer"
        ));
    }

    #[test]
    fn uninstall_values_are_emitted_as_individual_scan_results() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\RustYuTest\{}", uuid::Uuid::new_v4());
        let (key, _) = root
            .create_subkey(&path)
            .unwrap_or_else(|error| panic!("create test key: {error}"));
        key.set_value("DisplayName", &"ChatGPT")
            .unwrap_or_else(|error| panic!("write DisplayName: {error}"));
        key.set_value("Publisher", &"OpenAI")
            .unwrap_or_else(|error| panic!("write Publisher: {error}"));

        let mut traces = Vec::new();
        append_uninstall_values(
            &ScanIdentity::from_name("ChatGPT"),
            &key,
            r"HKCU\Software\RustYuTest\ChatGPT",
            &mut traces,
        );

        assert_eq!(traces.len(), 2);
        assert!(traces
            .iter()
            .all(|trace| trace.trace_type == TraceType::RegistryValue));
        assert!(traces
            .iter()
            .any(|trace| trace.path.ends_with(r"\Publisher")));
        drop(key);
        let _ = root.delete_subkey_all(path);
    }

    #[test]
    fn registry_values_that_reference_install_root_receive_explainable_score() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\RustYuTest\{}", uuid::Uuid::new_v4());
        let (key, _) = root
            .create_subkey(&path)
            .unwrap_or_else(|error| panic!("create reference test key: {error}"));
        key.set_value(
            "DemoRun",
            &r#""C:\Program Files\RustYu Deep Scan\fixture.exe" --background"#,
        )
        .unwrap_or_else(|error| panic!("write reference value: {error}"));
        key.set_value("Unrelated", &r"C:\Program Files\Other App\other.exe")
            .unwrap_or_else(|error| panic!("write unrelated value: {error}"));
        drop(key);

        let mut traces = Vec::new();
        let mut visited = 0;
        scan_registry_reference_values(
            HKEY_CURRENT_USER,
            &path,
            &[r"C:\Program Files\RustYu Deep Scan".to_string()],
            &ScanIdentity::from_name("RustYu Deep Scan"),
            &mut traces,
            0,
            0,
            false,
            &mut visited,
        );

        assert_eq!(traces.len(), 1);
        assert!(traces[0].path.ends_with(r"\DemoRun"));
        assert_eq!(traces[0].confidence_score, 90);
        assert_eq!(
            traces[0].confidence_reason,
            ConfidenceReason::RegistryValueReferencesInstallLocation
        );
        assert_eq!(
            traces[0].related_path.as_deref(),
            Some(r"C:\Program Files\RustYu Deep Scan")
        );
        let _ = root.delete_subkey_all(path);
    }

    #[test]
    fn muicache_values_referencing_install_root_are_selectable() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\RustYuTest\{}\Shell\MuiCache", uuid::Uuid::new_v4());
        let (key, _) = root
            .create_subkey(&path)
            .unwrap_or_else(|error| panic!("create muicache test key: {error}"));
        key.set_value(
            r"C:\eastmoney\Choice\ChoiceUpd.exe.FriendlyAppName",
            &"Choice 应用名",
        )
        .unwrap_or_else(|error| panic!("write muicache value: {error}"));
        drop(key);

        let mut traces = Vec::new();
        let mut visited = 0;
        scan_registry_reference_values(
            HKEY_CURRENT_USER,
            &path,
            &[r"C:\eastmoney\Choice".to_string()],
            &ScanIdentity::from_name("Choice 9.0.0.1"),
            &mut traces,
            0,
            0,
            true,
            &mut visited,
        );

        assert_eq!(traces.len(), 1);
        // 值名引用安装根本会命中 protected 分支的受保护规则；MuiCache 是
        // 每应用显示名缓存，必须允许用户勾选清理。
        assert!(!traces[0].is_critical);
        assert_eq!(traces[0].confidence_score, 90);
        let _ = root.delete_subkey_all(path);
    }

    #[test]
    fn protected_non_muicache_references_stay_critical() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\RustYuTest\{}\App Paths", uuid::Uuid::new_v4());
        let (key, _) = root
            .create_subkey(&path)
            .unwrap_or_else(|error| panic!("create app paths test key: {error}"));
        key.set_value(
            r"C:\eastmoney\Choice\Choice.exe",
            &r#"C:\eastmoney\Choice\Choice.exe"#,
        )
        .unwrap_or_else(|error| panic!("write app path value: {error}"));
        drop(key);

        let mut traces = Vec::new();
        let mut visited = 0;
        scan_registry_reference_values(
            HKEY_CURRENT_USER,
            &path,
            &[r"C:\eastmoney\Choice".to_string()],
            &ScanIdentity::from_name("Choice 9.0.0.1"),
            &mut traces,
            0,
            0,
            true,
            &mut visited,
        );

        assert_eq!(traces.len(), 1);
        assert!(traces[0].is_critical);
        let _ = root.delete_subkey_all(path);
    }

    #[test]
    fn exact_application_key_surfaces_its_named_values() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\RustYuTest\{}", uuid::Uuid::new_v4());
        let (key, _) = root
            .create_subkey(&path)
            .unwrap_or_else(|error| panic!("create application key: {error}"));
        key.set_value("InstallRoot", &r"C:\Program Files\RustYu Deep Scan")
            .unwrap_or_else(|error| panic!("write application value: {error}"));

        let mut traces = Vec::new();
        append_matched_key_values(
            &ScanIdentity::from_name("RustYu Deep Scan"),
            &key,
            r"HKCU\Software\RustYuTest\RustYuDeepScan",
            &mut traces,
            60,
            ConfidenceReason::ExactNameRegistry,
        );

        assert_eq!(traces.len(), 1);
        assert!(traces[0].path.ends_with(r"\InstallRoot"));
        assert_eq!(traces[0].confidence_score, 60);
        drop(key);
        let _ = root.delete_subkey_all(path);
    }
}
