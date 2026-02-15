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
                        if let Some(program) = parse_registry_entry(&subkey) {
                            // 跳过系统组件和更新
                            if !is_system_component(&program) {
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

/// 解析注册表项
fn parse_registry_entry(subkey: &RegKey) -> Option<InstalledProgram> {
    // 必须有 DisplayName
    let name: String = subkey.get_value("DisplayName").ok()?;

    // 跳过以 KB 开头的补丁
    if name.starts_with("KB") || name.to_lowercase().contains("security update") {
        return None;
    }

    let mut program = InstalledProgram::new(name, InstallSource::Registry);

    // 提取可选字段
    program.publisher = subkey.get_value("Publisher").ok();
    program.version = subkey.get_value("DisplayVersion").ok();
    program.install_date = subkey.get_value("InstallDate").ok();
    program.install_location = subkey.get_value("InstallLocation").ok();
    program.uninstall_string = subkey.get_value("UninstallString").ok();
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

/// 检查是否为系统组件
fn is_system_component(program: &InstalledProgram) -> bool {
    let system_components = [
        "Windows",
        "Microsoft Visual C++",
        "Microsoft Visual Studio",
        "Microsoft .NET",
        "Windows Defender",
        "Windows Security",
    ];

    // 检查名称
    for component in &system_components {
        if program
            .name
            .to_lowercase()
            .contains(&component.to_lowercase())
        {
            // 但不是所有 Windows 开头的都是系统组件
            if program.name.to_lowercase().contains("windows") {
                // 检查是否是第三方程序
                if let Some(publisher) = &program.publisher {
                    if publisher.to_lowercase().contains("microsoft") {
                        return true;
                    }
                }
            }
        }
    }

    // 检查 ParentKeyName (通常是系统组件)
    false
}

/// 获取特定程序的详细信息
#[allow(dead_code)]
pub fn get_program_info(name: &str) -> Result<Option<InstalledProgram>, UninstallerError> {
    let programs = list_registry_programs()?;

    Ok(programs
        .into_iter()
        .find(|p| p.name.to_lowercase().contains(&name.to_lowercase())))
}
