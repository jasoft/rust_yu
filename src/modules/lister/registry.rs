use super::models::{InstallSource, InstalledProgram, MetadataConfidence, MetadataSource};
use crate::modules::common::error::UninstallerError;
use winreg::enums::*;
use winreg::RegKey;

/// 从注册表读取已安装程序
pub fn list_registry_programs() -> Result<Vec<InstalledProgram>, UninstallerError> {
    let mut programs = Vec::new();

    // 注册表路径列表
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
        match RegKey::predef(*hkey).open_subkey(path) {
            Ok(key) => {
                for name in key.enum_keys().filter_map(|k| k.ok()) {
                    if let Ok(subkey) = key.open_subkey(&name) {
                        // Windows 的 ARP 列表使用这些注册表标记区分“安装的应用”和
                        // 系统组件/子组件。名称或 Publisher 不是可靠的判据：例如
                        // Windows Desktop Runtime、Visual C++ Runtime 仍然应该显示。
                        if !is_hidden_arp_entry(&subkey, &name) {
                            if let Some(program) = parse_registry_entry(*hkey, path, &name, &subkey)
                            {
                                programs.push(program);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!("无法打开注册表路径 {}: {}", path, e);
            }
        }
    }

    Ok(programs)
}

/// 检查程序是否仍存在于卸载注册表中。
pub fn registry_program_exists(name: &str) -> Result<bool, UninstallerError> {
    let target = name.to_lowercase();

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
            for subkey_name in key.enum_keys().filter_map(|item| item.ok()) {
                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    if let Ok(display_name) = subkey.get_value::<String, _>("DisplayName") {
                        if display_name.to_lowercase() == target {
                            return Ok(true);
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

/// 解析注册表项
fn parse_registry_entry(
    hkey: winreg::HKEY,
    parent_path: &str,
    subkey_name: &str,
    subkey: &RegKey,
) -> Option<InstalledProgram> {
    // 必须有 DisplayName
    let name: String = subkey.get_value("DisplayName").ok()?;

    // 跳过以 KB 开头的补丁
    if name.starts_with("KB") || name.to_lowercase().contains("security update") {
        return None;
    }

    let install_source = if is_msi_entry(subkey) {
        InstallSource::Msi
    } else {
        InstallSource::Registry
    };
    let mut program = InstalledProgram::new(name, install_source);
    // 注册表扫描本身没有稳定的顺序，不能把随机 UUID 作为卸载目标 ID。
    // 使用完整注册表项路径，使列表刷新、卸载计划和执行阶段始终指向同一个程序。
    let registry_key_path = build_registry_key_path(hkey, parent_path, subkey_name);
    program.id = format!("registry:{registry_key_path}");

    // 提取可选字段
    program.publisher = subkey.get_value("Publisher").ok();
    program.version = subkey.get_value("DisplayVersion").ok();
    program.install_date = subkey.get_value("InstallDate").ok();
    program.install_location = subkey.get_value("InstallLocation").ok();
    program.uninstall_string = subkey.get_value("UninstallString").ok();
    program.quiet_uninstall_string = subkey.get_value("QuietUninstallString").ok();
    program.uninstall_registry_key_path = Some(registry_key_path);
    program.icon_path = subkey.get_value("DisplayIcon").ok();
    program.url_info_about = subkey.get_value("URLInfoAbout").ok();
    program.help_link = subkey.get_value("HelpLink").ok();

    if program.install_date.is_some() {
        program.install_date_source = MetadataSource::Registry;
        program.install_date_confidence = MetadataConfidence::Medium;
    }

    if program.icon_path.is_some() {
        program.icon_source = MetadataSource::Registry;
        program.icon_confidence = MetadataConfidence::Medium;
    }

    // 估算大小 (KB)
    if let Ok(size) = subkey.get_value::<u32, _>("EstimatedSize") {
        program.estimated_size = Some(size as u64 * 1024); // 转换为字节
        program.size = program.estimated_size;
        program.size_source = MetadataSource::Registry;
        program.size_confidence = MetadataConfidence::High;
    }

    Some(program)
}

fn is_msi_entry(subkey: &RegKey) -> bool {
    subkey
        .get_value::<u32, _>("WindowsInstaller")
        .map(|value| value == 1)
        .unwrap_or_else(|_| {
            subkey
                .get_value::<String, _>("WindowsInstaller")
                .map(|value| value == "1")
                .unwrap_or(false)
        })
}

fn format_hkey(hkey: winreg::HKEY) -> &'static str {
    match hkey {
        HKEY_LOCAL_MACHINE => "HKLM",
        HKEY_CURRENT_USER => "HKCU",
        HKEY_CLASSES_ROOT => "HKCR",
        HKEY_USERS => "HKU",
        HKEY_CURRENT_CONFIG => "HKCC",
        _ => "UNKNOWN",
    }
}

/// 检查是否为系统组件
fn build_registry_key_path(hkey: winreg::HKEY, parent_path: &str, subkey_name: &str) -> String {
    format!("{}\\{}\\{}", format_hkey(hkey), parent_path, subkey_name)
}

fn is_hidden_arp_entry(subkey: &RegKey, subkey_name: &str) -> bool {
    // ARPSYSTEMCOMPONENT=1 是 Windows Installer/ARP 的权威系统组件标记。
    if registry_flag_is_one(subkey, "SystemComponent") {
        return true;
    }

    // ParentKeyName/ParentDisplayName 表示安装器注册的子组件；设置中的主列表
    // 不会把这些独立列出。只要字段存在且非空就排除，不依赖父项是否仍可打开。
    if registry_value_is_non_empty(subkey, "ParentKeyName")
        || registry_value_is_non_empty(subkey, "ParentDisplayName")
    {
        return true;
    }

    // 补丁/安全更新没有稳定的 DisplayName 约定，ReleaseType 是剩余的 ARP 语义标记。
    if let Ok(release_type) = subkey.get_value::<String, _>("ReleaseType") {
        let normalized = release_type.trim().to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "update" | "security update" | "hotfix" | "patch" | "service pack"
        ) {
            return true;
        }
    }

    // 仅保留 KB 名称检查作为没有 ARP 元数据的旧补丁兜底；不能按 Microsoft/Windows
    // 名称过滤，否则会误删 Windows Runtime、VC Runtime 等设置中可见应用。
    let name = subkey
        .get_value::<String, _>("DisplayName")
        .unwrap_or_else(|_| subkey_name.to_string());
    name.trim_start().to_ascii_lowercase().starts_with("kb")
        || name.to_ascii_lowercase().contains("security update")
}

fn registry_flag_is_one(subkey: &RegKey, value_name: &str) -> bool {
    subkey
        .get_value::<u32, _>(value_name)
        .map(|value| value == 1)
        .or_else(|_| {
            subkey
                .get_value::<String, _>(value_name)
                .map(|value| value.trim() == "1")
        })
        .unwrap_or(false)
}

fn registry_value_is_non_empty(subkey: &RegKey, value_name: &str) -> bool {
    subkey
        .get_value::<String, _>(value_name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{build_registry_key_path, format_hkey, is_hidden_arp_entry, HKEY_LOCAL_MACHINE};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    #[test]
    fn registry_program_id_is_derived_from_full_registry_key_path() {
        let path = build_registry_key_path(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            "demo",
        );

        assert_eq!(format_hkey(HKEY_LOCAL_MACHINE), "HKLM");
        assert_eq!(
            path,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\demo"
        );
        assert_eq!(
            format!("registry:{path}"),
            r"registry:HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\demo"
        );
    }

    #[test]
    fn name_based_runtime_entries_are_not_hidden_without_arp_markers() {
        let key = RegKey::predef(HKEY_CURRENT_USER);
        let temp = key
            .create_subkey(format!("Software\\RustYuTest\\{}", uuid::Uuid::new_v4()))
            .expect("测试注册表项应可创建");
        temp.0
            .set_value("DisplayName", &"Microsoft Windows Desktop Runtime 8")
            .expect("测试名称应可写入");

        assert!(!is_hidden_arp_entry(&temp.0, "runtime"));
        let _ = key.delete_subkey_all("Software\\RustYuTest");
    }
}

/// 获取特定程序的详细信息
#[allow(dead_code)]
pub fn get_program_info(name: &str) -> Result<Option<InstalledProgram>, UninstallerError> {
    let programs = list_registry_programs()?;

    Ok(programs
        .into_iter()
        .find(|p| p.name.to_lowercase().contains(&name.to_lowercase())))
}
