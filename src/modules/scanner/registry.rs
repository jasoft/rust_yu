use super::models::{Confidence, Trace, TraceType};
use super::scope::ScanIdentity;
use crate::modules::common::error::UninstallerError;
use winreg::enums::*;
use winreg::RegKey;

const MAX_DEPTH: u32 = 5;

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
        let confidence = if path.to_ascii_lowercase().contains("uninstall") {
            Confidence::High
        } else {
            Confidence::Medium
        };

        traces.push(
            Trace::new(
                identity.display_name().to_string(),
                TraceType::RegistryKey,
                full_path,
            )
            .with_description(description)
            .with_confidence(confidence),
        );
    }

    for name in key.enum_keys().filter_map(|key| key.ok()) {
        let subpath = format!("{}\\{}", path, name);
        let _ = scan_registry_key(hkey, &subpath, identity, traces, depth + 1);
    }

    Ok(())
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
                            full_path,
                        )
                        .with_description(format!(
                            "卸载信息: {} ({})",
                            display_name.unwrap_or_default(),
                            install_location.unwrap_or_default()
                        ))
                        .with_confidence(Confidence::High),
                    );
                }
            }
        }
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
    use super::{is_protected_registry_path, uninstall_entry_matches};
    use crate::modules::scanner::scope::ScanIdentity;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

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
}
