use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use winreg::enums::*;
use winreg::HKEY;

#[cfg(windows)]
use windows::Win32::Security::{
    AllocateAndInitializeSid, CheckTokenMembership, FreeSid, PSID, SID_IDENTIFIER_AUTHORITY,
};

/// 规范化路径（处理大小写、斜杠等）
#[allow(dead_code)]
pub fn normalize_path(path: &str) -> String {
    let path = path.replace('/', "\\");

    // 处理连续的反斜杠
    while path.contains("\\\\") {
        let path = path.replace("\\\\", "\\");
        return normalize_path(&path);
    }

    path
}

/// 计算目录大小
pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut size = 0u64;

    if path.is_file() {
        return path.metadata().map(|m| m.len());
    }

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                size += metadata.len();
            }
        }
    }

    Ok(size)
}

/// 模糊匹配字符串
pub fn fuzzy_match(text: &str, pattern: &str) -> bool {
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(text, pattern).is_some()
}

/// 获取模糊匹配分数
#[allow(dead_code)]
pub fn fuzzy_score(text: &str, pattern: &str) -> i64 {
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(text, pattern).unwrap_or(0)
}

/// 格式化文件大小
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 规范化卸载命令，优先把 MSI 的维护模式切换为真正的卸载模式。
pub fn normalize_uninstall_command(uninstall_string: &str) -> String {
    let trimmed = uninstall_string.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("msiexec") {
        return trimmed.to_string();
    }

    let mut normalized = trimmed.to_string();
    if let Some(msi_arg_start) = normalized.find(['/', '-']) {
        let candidate = normalized[msi_arg_start..].to_string();
        let candidate_lower = candidate.to_lowercase();
        if candidate_lower.starts_with("/i")
            || candidate_lower.starts_with("-i")
        {
            normalized.replace_range(msi_arg_start + 1..msi_arg_start + 2, "X");
        }
    }
    let normalized_lower = normalized.to_lowercase();

    if !normalized_lower.contains("/quiet") {
        normalized.push_str(" /quiet");
    }
    if !normalized_lower.contains("/norestart") {
        normalized.push_str(" /norestart");
    }

    normalized
}

/// 将卸载命令拆分为可执行文件和参数，便于直接 spawn，避免再套一层 cmd。
pub fn split_command_for_spawn(
    command: &str,
) -> Result<(String, Vec<String>), crate::modules::common::error::UninstallerError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(crate::modules::common::error::UninstallerError::Other(
            "卸载命令为空".to_string(),
        ));
    }

    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in trimmed.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err(crate::modules::common::error::UninstallerError::Other(
            "卸载命令引号不匹配".to_string(),
        ));
    }

    if !current.is_empty() {
        args.push(current);
    }

    let mut parts = args.into_iter();
    let executable = parts.next().ok_or_else(|| {
        crate::modules::common::error::UninstallerError::Other("卸载命令缺少可执行文件".to_string())
    })?;

    Ok((executable, parts.collect()))
}

/// 为 GUI/传统 EXE 卸载命令创建临时批处理包装器。
///
/// 某些 Windows 卸载器通过批处理上下文调用时更稳定，尤其是需要
/// `cmd.exe` 参与处理引号或内部启动逻辑的旧式卸载程序。
pub fn create_command_wrapper_script(
    command: &str,
) -> Result<std::path::PathBuf, crate::modules::common::error::UninstallerError> {
    let script_path = std::env::temp_dir().join(format!("rust-yu-uninstall-{}.cmd", uuid::Uuid::new_v4()));
    let script_content = format!("@echo off\r\n{} >nul 2>&1\r\n", command.trim());
    std::fs::write(&script_path, script_content)?;
    Ok(script_path)
}

/// 检查路径是否为系统关键路径
pub fn is_system_critical_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    let critical_paths = [
        r"C:\WINDOWS",
        r"C:\WINDOWS\SYSTEM32",
        r"C:\WINDOWS\SYSWOW64",
        r"C:\WINDOWS\INF",
        r"C:\WINDOWS\WINSXS",
        r"C:\PROGRAM FILES\WINDOWS",
    ];

    critical_paths
        .iter()
        .any(|p| path_upper.starts_with(&p.to_uppercase()))
}

/// 检查注册表路径是否为关键路径
pub fn is_critical_registry_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    let critical_paths = [
        r"HKLM\SYSTEM",
        r"HKLM\SOFTWARE\Microsoft\Windows NT",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RUN",
        r"HKCR\*",
        r"HKLM\BOOT",
    ];

    critical_paths.iter().any(|p| path_upper.starts_with(p))
}

/// 生成唯一 ID
#[allow(dead_code)]
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 检查当前进程是否具备管理员权限。
///
/// 卸载系统级程序前必须先做权限校验，避免卸载器被启动后又因为 UAC
/// 或权限不足静默失败，导致命令行错误地显示“已结束”。
#[cfg(windows)]
pub fn ensure_running_as_administrator() -> Result<(), crate::modules::common::error::UninstallerError>
{
    const SECURITY_BUILTIN_DOMAIN_RID: u32 = 0x0000_0020;
    const DOMAIN_ALIAS_RID_ADMINS: u32 = 0x0000_0220;

    let nt_authority = SID_IDENTIFIER_AUTHORITY {
        Value: [0, 0, 0, 0, 0, 5],
    };
    let mut administrators_group = PSID::default();

    unsafe {
        AllocateAndInitializeSid(
            &nt_authority,
            2,
            SECURITY_BUILTIN_DOMAIN_RID as u32,
            DOMAIN_ALIAS_RID_ADMINS as u32,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut administrators_group,
        )
        .map_err(|error| {
            crate::modules::common::error::UninstallerError::PermissionDenied(format!(
                "无法初始化管理员 SID: {error}"
            ))
        })?;

        let mut is_member = windows::Win32::Foundation::FALSE;
        let membership_result = CheckTokenMembership(None, administrators_group, &mut is_member);
        FreeSid(administrators_group);

        membership_result.map_err(|error| {
            crate::modules::common::error::UninstallerError::PermissionDenied(format!(
                "无法检查管理员权限: {error}"
            ))
        })?;

        if is_member.as_bool() {
            Ok(())
        } else {
            Err(crate::modules::common::error::UninstallerError::PermissionDenied(
                "当前进程不是管理员，请以管理员身份重新运行后再卸载".to_string(),
            ))
        }
    }
}

#[cfg(not(windows))]
pub fn ensure_running_as_administrator() -> Result<(), crate::modules::common::error::UninstallerError>
{
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_uninstall_command;

    #[test]
    fn normalize_uninstall_command_keeps_non_msi_command() {
        assert_eq!(
            normalize_uninstall_command(r#""C:\Program Files\App\uninstall.exe" /S"#),
            r#""C:\Program Files\App\uninstall.exe" /S"#
        );
    }

    #[test]
    fn normalize_uninstall_command_converts_msi_install_mode_to_uninstall() {
        assert_eq!(
            normalize_uninstall_command("MsiExec.exe /I{23170F69-40C1-2702-2600-000001000000}"),
            "MsiExec.exe /X{23170F69-40C1-2702-2600-000001000000} /quiet /norestart"
        );
    }

    #[test]
    fn normalize_uninstall_command_preserves_existing_msi_uninstall_switch() {
        assert_eq!(
            normalize_uninstall_command("msiexec /x {GUID}"),
            "msiexec /x {GUID} /quiet /norestart"
        );
    }

    #[test]
    fn split_command_for_spawn_handles_quoted_executable() {
        let (executable, args) =
            super::split_command_for_spawn(r#""C:\Program Files\7-Zip\Uninstall.exe" /S"#)
                .expect("应能解析带引号的可执行文件");

        assert_eq!(executable, r#"C:\Program Files\7-Zip\Uninstall.exe"#);
        assert_eq!(args, vec!["/S".to_string()]);
    }

    #[test]
    fn split_command_for_spawn_handles_msi_arguments() {
        let (executable, args) =
            super::split_command_for_spawn("MsiExec.exe /X{GUID} /quiet /norestart")
                .expect("应能解析 MSI 卸载命令");

        assert_eq!(executable, "MsiExec.exe");
        assert_eq!(
            args,
            vec![
                "/X{GUID}".to_string(),
                "/quiet".to_string(),
                "/norestart".to_string()
            ]
        );
    }

    #[test]
    fn create_command_wrapper_script_writes_expected_command() {
        let script_path = super::create_command_wrapper_script(r#""C:\Program Files\App\uninstall.exe" /S"#)
            .expect("应能创建包装脚本");
        let content = std::fs::read_to_string(&script_path).expect("应能读取包装脚本");
        let _ = std::fs::remove_file(&script_path);

        assert!(content.contains(r#""C:\Program Files\App\uninstall.exe" /S"#));
    }
}

/// 解析注册表路径
pub fn parse_registry_path(path: &str) -> Option<(HKEY, &str)> {
    let path = path.trim();

    if path.starts_with("HKLM\\") || path.starts_with("HKEY_LOCAL_MACHINE\\") {
        Some((HKEY_LOCAL_MACHINE, &path[5..]))
    } else if path.starts_with("HKCU\\") || path.starts_with("HKEY_CURRENT_USER\\") {
        Some((HKEY_CURRENT_USER, &path[5..]))
    } else if path.starts_with("HKCR\\") || path.starts_with("HKEY_CLASSES_ROOT\\") {
        Some((HKEY_CLASSES_ROOT, &path[5..]))
    } else if path.starts_with("HKU\\") || path.starts_with("HKEY_USERS\\") {
        Some((HKEY_USERS, &path[4..]))
    } else {
        None
    }
}

/// 获取 Windows 系统目录
#[allow(dead_code)]
pub fn get_system_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(program_files) = std::env::var("ProgramFiles") {
        dirs.push(std::path::PathBuf::from(program_files));
    }

    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(std::path::PathBuf::from(program_files_x86));
    }

    if let Ok(program_w6432) = std::env::var("ProgramW6432") {
        dirs.push(std::path::PathBuf::from(program_w6432));
    }

    if let Ok(system_root) = std::env::var("SystemRoot") {
        let system_root = std::path::PathBuf::from(system_root);
        dirs.push(system_root.join("System32"));
        dirs.push(system_root.join("SysWOW64"));
    }

    // 公共目录
    if let Ok(public) = std::env::var("Public") {
        let public = std::path::PathBuf::from(public);
        dirs.push(public.join("Documents"));
        dirs.push(public.join("Desktop"));
    }

    // 用户目录
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("AppData").join("Roaming"));
        dirs.push(home.join("AppData").join("Local"));
        dirs.push(home.join("Desktop"));
    }

    // 开始菜单
    if let Ok(program_data) = std::env::var("ProgramData") {
        dirs.push(
            std::path::PathBuf::from(program_data).join("Microsoft\\Windows\\Start Menu\\Programs"),
        );
    }

    dirs
}
