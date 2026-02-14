use crate::modules::common::error::UninstallerError;
use winreg::enums::*;
use winreg::RegKey;
use super::models::{Trace, TraceType, Confidence};

const MAX_DEPTH: u32 = 5;

/// 扫描注册表痕迹
pub fn scan_registry_traces(program_name: &str) -> Result<Vec<Trace>, UninstallerError> {
    let mut traces = Vec::new();
    let search_pattern = program_name.to_lowercase();

    // 主要搜索路径
    let search_paths: Vec<(winreg::HKEY, &str)> = vec![
        (HKEY_LOCAL_MACHINE, r"SOFTWARE"),
        (HKEY_CURRENT_USER, r"SOFTWARE"),
        (HKEY_CLASSES_ROOT, r""),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
    ];

    for (hkey, path) in &search_paths {
        if let Err(e) = scan_registry_key(*hkey, path, &search_pattern, &mut traces, 0) {
            tracing::debug!("扫描注册表路径 {} 失败: {}", path, e);
        }
    }

    // 检查 Uninstall 键中的残留
    scan_uninstall_keys(program_name, &mut traces);

    Ok(traces)
}

/// 递归扫描注册表键
fn scan_registry_key(
    hkey: winreg::HKEY,
    path: &str,
    pattern: &str,
    traces: &mut Vec<Trace>,
    depth: u32,
) -> Result<(), UninstallerError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }

    let key = match RegKey::predef(hkey).open_subkey(path) {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };

    // 检查当前键名是否匹配
    let key_name = path.split('\\').last().unwrap_or("");
    if key_name.to_lowercase().contains(pattern) {
        let full_path = format!("{}\\{}", format_hkey(hkey), path);

        // 检查是否为 Uninstall 相关键
        let description = if path.to_lowercase().contains("uninstall") {
            format!("卸载残留: {}", key_name)
        } else {
            format!("注册表项: {}", key_name)
        };

        let confidence = if path.to_lowercase().contains("uninstall") || path.to_lowercase().contains("app paths") {
            Confidence::High
        } else {
            Confidence::Medium
        };

        let trace = Trace::new(
            pattern.to_string(),
            TraceType::RegistryKey,
            full_path,
        )
        .with_description(description)
        .with_confidence(confidence);

        traces.push(trace);
    }

    // 枚举子键
    for name in key.enum_keys().filter_map(|k| k.ok()) {
        let subpath = format!("{}\\{}", path, name);

        // 递归扫描子键
        let _ = scan_registry_key(hkey, &subpath, pattern, traces, depth + 1);
    }

    Ok(())
}

/// 扫描 Uninstall 相关键
fn scan_uninstall_keys(program_name: &str, traces: &mut Vec<Trace>) {
    let search_pattern = program_name.to_lowercase();

    let paths = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];

    for (hkey, path) in &paths {
        if let Ok(key) = RegKey::predef(*hkey).open_subkey(path) {
            for name in key.enum_keys().filter_map(|k| k.ok()) {
                if name.to_lowercase().contains(&search_pattern) {
                    if let Ok(subkey) = key.open_subkey(&name) {
                        let full_path = format!("{}\\{}\\{}", format_hkey(*hkey), path, name);

                        // 获取安装位置
                        let install_location: Option<String> = subkey.get_value("InstallLocation").ok();
                        let display_name: Option<String> = subkey.get_value("DisplayName").ok();

                        let trace = Trace::new(
                            program_name.to_string(),
                            TraceType::RegistryKey,
                            full_path,
                        )
                        .with_description(format!(
                            "卸载信息: {} ({})",
                            display_name.unwrap_or_default(),
                            install_location.unwrap_or_default()
                        ))
                        .with_confidence(Confidence::High);

                        traces.push(trace);
                    }
                }
            }
        }
    }
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
