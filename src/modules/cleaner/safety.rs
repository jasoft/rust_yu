use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::{Trace, TraceType};

/// 关键系统路径黑名单
const CRITICAL_PATHS: &[&str] = &[
    r"C:\Windows",
    r"C:\Windows\System32",
    r"C:\Windows\SysWOW64",
    r"C:\Windows\WinSxS",
    r"C:\Windows\INF",
    r"C:\Windows\DriverStore",
    r"C:\Windows\System32\DriverStore",
];

/// 关键注册表路径黑名单
const CRITICAL_REGISTRY_PATHS: &[&str] = &[
    r"HKLM\SYSTEM",
    r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
    r"HKCR\*",
    r"HKLM\BOOT",
    r"HKLM\SAM",
    r"HKLM\SECURITY",
];

/// 删除前检查
pub fn pre_delete_check(trace: &Trace) -> Result<(), UninstallerError> {
    // 检查是否标记为关键项
    if trace.is_critical {
        return Err(UninstallerError::CriticalSystemItem(
            format!("该项被标记为关键系统项: {}", trace.path),
        ));
    }

    // 根据类型进行特定检查
    match trace.trace_type {
        TraceType::RegistryKey
        | TraceType::RegistryValue => {
            if is_critical_registry(&trace.path) {
                return Err(UninstallerError::CriticalSystemItem(
                    "不能删除关键系统注册表项".to_string(),
                ));
            }
        }
        TraceType::File
        | TraceType::AppData
        | TraceType::Shortcut => {
            if is_critical_path(&trace.path) {
                return Err(UninstallerError::CriticalSystemItem(
                    "不能删除关键系统目录".to_string(),
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

/// 检查是否为关键系统路径
fn is_critical_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    for critical in CRITICAL_PATHS {
        if path_upper.starts_with(&critical.to_uppercase()) {
            return true;
        }
    }

    false
}

/// 检查是否为关键注册表路径
fn is_critical_registry(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    for critical in CRITICAL_REGISTRY_PATHS {
        if path_upper.starts_with(critical) {
            return true;
        }
    }

    false
}

/// 列出所有关键路径（用于显示）
#[allow(dead_code)]
pub fn get_critical_paths() -> &'static [&'static str] {
    CRITICAL_PATHS
}

/// 列出所有关键注册表路径（用于显示）
#[allow(dead_code)]
pub fn get_critical_registry_paths() -> &'static [&'static str] {
    CRITICAL_REGISTRY_PATHS
}
