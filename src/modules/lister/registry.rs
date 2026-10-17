use super::models::{
    InstallSource, InstalledProgram, MetadataConfidence, MetadataSource, RegistryValueDetail,
};
use crate::modules::common::error::UninstallerError;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

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
    Ok(!find_uninstall_registry_key_paths(name).is_empty())
}

/// 按 DisplayName 精确匹配定位 Uninstall 注册表项的完整路径（HKLM/HKCU 均覆盖）。
///
/// 强制卸载时使用：即使反安装程序丢失、卸载信息损坏，或子键名是 GUID
/// （不包含程序名），也能定位到安装程序列表中的条目以便清除。
pub fn find_uninstall_registry_key_paths(name: &str) -> Vec<String> {
    let target = name.to_lowercase();
    let mut paths_found = Vec::new();

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
                            paths_found.push(format!(
                                "{}\\{}\\{}",
                                format_hkey(*hkey),
                                path,
                                subkey_name
                            ));
                        }
                    }
                }
            }
        }
    }

    paths_found
}

/// 只读获取某个 ARP/Uninstall 键的全部值。
///
/// 前端传入的路径不可信，因此这里只接受 HKLM/HKCU 下三个标准 Uninstall
/// 分支的直接子键，避免详情功能演变成任意注册表读取接口。
pub fn read_uninstall_registry_values(
    registry_key_path: &str,
) -> Result<Vec<RegistryValueDetail>, UninstallerError> {
    let (hkey, key_path) = parse_uninstall_registry_path(registry_key_path)?;
    let key = RegKey::predef(hkey)
        .open_subkey_with_flags(&key_path, KEY_READ)
        .map_err(|error| UninstallerError::Registry(error.to_string()))?;

    let mut values = key
        .enum_values()
        .map(|result| {
            let (name, raw) =
                result.map_err(|error| UninstallerError::Registry(error.to_string()))?;
            Ok(RegistryValueDetail {
                name,
                value_type: format!("{:?}", raw.vtype),
                value: format_registry_value(&raw),
                byte_length: raw.bytes.len() as u64,
            })
        })
        .collect::<Result<Vec<_>, UninstallerError>>()?;
    values.sort_by_key(|value| value.name.to_ascii_lowercase());
    Ok(values)
}

fn parse_uninstall_registry_path(
    registry_key_path: &str,
) -> Result<(winreg::HKEY, String), UninstallerError> {
    let trimmed = registry_key_path.trim();
    let normalized = trimmed
        .strip_prefix("registry:")
        .unwrap_or(trimmed)
        .replace('/', "\\");
    let (hive, key_path) = normalized
        .split_once('\\')
        .ok_or_else(|| UninstallerError::Registry("卸载注册表路径格式无效".to_string()))?;
    let hkey = if hive.eq_ignore_ascii_case("HKLM")
        || hive.eq_ignore_ascii_case("HKEY_LOCAL_MACHINE")
    {
        HKEY_LOCAL_MACHINE
    } else if hive.eq_ignore_ascii_case("HKCU") || hive.eq_ignore_ascii_case("HKEY_CURRENT_USER") {
        HKEY_CURRENT_USER
    } else {
        return Err(UninstallerError::Registry(
            "只允许读取 HKLM 或 HKCU 的卸载信息".to_string(),
        ));
    };

    let lower = key_path.to_ascii_lowercase();
    let allowed = if hkey == HKEY_LOCAL_MACHINE {
        lower.starts_with(r"software\microsoft\windows\currentversion\uninstall\")
            || lower
                .starts_with(r"software\wow6432node\microsoft\windows\currentversion\uninstall\")
    } else {
        lower.starts_with(r"software\microsoft\windows\currentversion\uninstall\")
    };
    if !allowed || key_path.ends_with('\\') {
        return Err(UninstallerError::Registry(
            "路径不属于受支持的卸载注册表项".to_string(),
        ));
    }
    Ok((hkey, key_path.to_string()))
}

pub(crate) fn format_registry_value(value: &RegValue) -> String {
    match value.vtype {
        REG_SZ | REG_EXPAND_SZ => decode_utf16_strings(&value.bytes)
            .into_iter()
            .next()
            .unwrap_or_default(),
        REG_MULTI_SZ => decode_utf16_strings(&value.bytes).join("\n"),
        REG_DWORD if value.bytes.len() >= 4 => {
            u32::from_le_bytes(value.bytes[..4].try_into().unwrap_or([0; 4])).to_string()
        }
        REG_DWORD_BIG_ENDIAN if value.bytes.len() >= 4 => {
            u32::from_be_bytes(value.bytes[..4].try_into().unwrap_or([0; 4])).to_string()
        }
        REG_QWORD if value.bytes.len() >= 8 => {
            u64::from_le_bytes(value.bytes[..8].try_into().unwrap_or([0; 8])).to_string()
        }
        _ => value
            .bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn decode_utf16_strings(bytes: &[u8]) -> Vec<String> {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    units
        .split(|unit| *unit == 0)
        .filter(|segment| !segment.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
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
    use super::{
        build_registry_key_path, format_hkey, format_registry_value, is_hidden_arp_entry,
        parse_uninstall_registry_path, HKEY_LOCAL_MACHINE,
    };
    use winreg::enums::{HKEY_CURRENT_USER, REG_DWORD, REG_SZ};
    use winreg::{RegKey, RegValue};

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

    #[test]
    fn uninstall_registry_reader_rejects_arbitrary_registry_paths() {
        assert!(parse_uninstall_registry_path(
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Demo"
        )
        .is_ok());
        assert!(parse_uninstall_registry_path(r"HKLM\SYSTEM\CurrentControlSet\Services").is_err());
        assert!(parse_uninstall_registry_path(
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"
        )
        .is_err());
    }

    #[test]
    fn registry_value_formatter_preserves_strings_and_numbers() {
        let text_bytes = "ChatGPT\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            format_registry_value(&RegValue {
                bytes: text_bytes,
                vtype: REG_SZ,
            }),
            "ChatGPT"
        );
        assert_eq!(
            format_registry_value(&RegValue {
                bytes: 42_u32.to_le_bytes().to_vec(),
                vtype: REG_DWORD,
            }),
            "42"
        );
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
