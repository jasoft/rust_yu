use std::process::Command;

use crate::modules::common::error::UninstallerError;
use crate::modules::common::text::{build_powershell_script, decode_windows_output};
use crate::modules::lister::models::InstalledProgram;
use std::path::Path;

use super::ProgramRemovalStatus;

pub fn resolve_uninstall_command(program: &InstalledProgram) -> Result<String, UninstallerError> {
    let package_full_name = package_full_name(program)?;
    let script = build_powershell_script(&build_remove_package_script(&package_full_name));

    Ok(format!(r#"powershell -NoProfile -Command "{}""#, script))
}

pub fn check_removal(program: &InstalledProgram) -> Result<ProgramRemovalStatus, UninstallerError> {
    let package_full_name = package_full_name(program)?;
    let package_present = is_package_present(&package_full_name)?;

    Ok(ProgramRemovalStatus {
        removed: !package_present,
        still_registered: package_present,
        install_dir_exists: false,
        store_package_present: package_present,
    })
}

fn package_full_name(program: &InstalledProgram) -> Result<String, UninstallerError> {
    let stored_id = program.id.trim();
    let package_id = stored_id.strip_prefix("store:").unwrap_or(stored_id);
    if !package_id.is_empty() && looks_like_package_full_name(package_id) {
        return Ok(package_id.to_string());
    }

    if let Some(package_full_name) = extract_package_name_from_uninstall_string(
        program.uninstall_string.as_deref().unwrap_or_default(),
    ) {
        return Ok(package_full_name);
    }

    if let Some(package_full_name) = program
        .install_location
        .as_deref()
        .and_then(|location| Path::new(location.trim()).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|candidate| looks_like_package_full_name(candidate))
    {
        return Ok(package_full_name);
    }

    if looks_like_package_family_name(package_id) {
        return package_full_name_from_family(package_id);
    }

    Err(UninstallerError::StoreApp(format!(
        "无法确定 {} 的 Store 包标识",
        program.name
    )))
}

fn looks_like_package_family_name(value: &str) -> bool {
    is_safe_package_identity(value) && value.matches('_').count() == 1
}

fn looks_like_package_full_name(value: &str) -> bool {
    // AppX 完整名至少包含 Name、Version、Architecture、ResourceId 和 PublisherHash，
    // 因此普通包族名（只有一个下划线）不会误入这个分支。
    is_safe_package_identity(value) && value.matches('_').count() >= 4
}

fn is_safe_package_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '\'' | '"' | '\\' | '/' | '<' | '>' | '|' | ':' | '*' | '?'
                )
        })
}

fn package_full_name_from_family(package_family_name: &str) -> Result<String, UninstallerError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &build_powershell_script(&build_resolve_package_full_name_script(package_family_name)),
        ])
        .output()
        .map_err(UninstallerError::FileSystem)?;

    if !output.status.success() {
        return Err(UninstallerError::StoreApp(format!(
            "解析 Store 包完整名失败: {}",
            decode_windows_output(&output.stderr)
        )));
    }

    let package_full_name = decode_windows_output(&output.stdout).trim().to_string();
    if looks_like_package_full_name(&package_full_name) {
        Ok(package_full_name)
    } else {
        Err(UninstallerError::StoreApp(format!(
            "找不到 Store 包族对应的完整包名: {package_family_name}"
        )))
    }
}

fn extract_package_name_from_uninstall_string(command: &str) -> Option<String> {
    let marker = "-Package '";
    let start = command.find(marker)? + marker.len();
    let tail = &command[start..];
    let end = tail.find('\'')?;
    Some(tail[..end].to_string())
}

fn is_package_present(package_full_name: &str) -> Result<bool, UninstallerError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &build_powershell_script(&build_package_presence_script(package_full_name)),
        ])
        .output()
        .map_err(UninstallerError::FileSystem)?;

    if !output.status.success() {
        return Err(UninstallerError::StoreApp(format!(
            "检查 Store 包状态失败: {}",
            decode_windows_output(&output.stderr)
        )));
    }

    let stdout = decode_windows_output(&output.stdout);
    Ok(stdout.trim().eq_ignore_ascii_case("true"))
}

fn build_remove_package_script(package_full_name: &str) -> String {
    format!(
        "$userSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value\nRemove-AppxPackage -Package '{}' -User $userSid -ErrorAction Stop",
        escape_single_quoted(package_full_name)
    )
}

fn build_package_presence_script(package_full_name: &str) -> String {
    format!(
        "$userSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value\n$exists = @(Get-AppxPackage -User $userSid | Where-Object {{ $_.PackageFullName -eq '{}' }}).Count -gt 0\nif ($exists) {{ 'true' }} else {{ 'false' }}",
        escape_single_quoted(package_full_name)
    )
}

fn build_resolve_package_full_name_script(package_family_name: &str) -> String {
    format!(
        "$familyName = '{}'\n$userSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value\n$package = @(Get-AppxPackage -User $userSid | Where-Object {{ $_.PackageFamilyName -eq $familyName }}) | Select-Object -First 1\nif ($null -ne $package -and $package.PackageFullName) {{ \"$($package.PackageFullName)\" }}",
        escape_single_quoted(package_family_name)
    )
}

fn escape_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::{
        build_package_presence_script, build_remove_package_script,
        build_resolve_package_full_name_script, extract_package_name_from_uninstall_string,
        looks_like_package_family_name, looks_like_package_full_name, package_full_name,
        resolve_uninstall_command,
    };
    use crate::modules::lister::models::{InstallSource, InstalledProgram};

    #[test]
    fn resolve_uninstall_command_uses_program_id_for_store_package() {
        let mut program =
            InstalledProgram::new("OpenAI.ChatGPT-Desktop".to_string(), InstallSource::Store);
        program.id = "OpenAI.ChatGPT-Desktop_1.2026.43.0_x64__2p2nqsd0c76g0".to_string();

        let command = resolve_uninstall_command(&program)
            .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert!(command.contains("Remove-AppxPackage"));
        assert!(command.contains("OpenAI.ChatGPT-Desktop_1.2026.43.0_x64__2p2nqsd0c76g0"));
        assert!(command.contains("-User $userSid"));
    }

    #[test]
    fn family_store_id_resolves_full_name_from_install_location() {
        let mut program = InstalledProgram::new("ChatGPT".to_string(), InstallSource::Store);
        program.id = "store:OpenAI.Codex_2p2nqsd0c76g0".to_string();
        program.install_location = Some(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.818.3698.0_arm64__2p2nqsd0c76g0"
                .to_string(),
        );

        assert_eq!(
            package_full_name(&program).unwrap(),
            "OpenAI.Codex_26.818.3698.0_arm64__2p2nqsd0c76g0"
        );
        assert!(looks_like_package_family_name("OpenAI.Codex_2p2nqsd0c76g0"));
        assert!(looks_like_package_full_name(
            "OpenAI.Codex_26.818.3698.0_arm64__2p2nqsd0c76g0"
        ));
    }

    #[test]
    fn resolve_script_maps_a_family_to_the_current_full_name() {
        let script = build_resolve_package_full_name_script("OpenAI.Codex_2p2nqsd0c76g0");

        assert!(script.contains("PackageFamilyName -eq $familyName"));
        assert!(script.contains("OpenAI.Codex_2p2nqsd0c76g0"));
        assert!(script.contains("PackageFullName"));
    }

    #[test]
    fn extract_package_name_from_uninstall_string_reads_appx_package_name() {
        let package_name = extract_package_name_from_uninstall_string(
            r#"powershell -Command "Remove-AppxPackage -Package 'Demo.Store_1.0.0_x64__abc'""#,
        );

        assert_eq!(package_name, Some("Demo.Store_1.0.0_x64__abc".to_string()));
    }

    #[test]
    fn package_full_name_falls_back_to_uninstall_string() {
        let mut program = InstalledProgram::new("Demo.Store".to_string(), InstallSource::Store);
        program.id = "demo-store".to_string();
        program.uninstall_string = Some(
            r#"powershell -Command "Remove-AppxPackage -Package 'Demo.Store_1.0.0_x64__abc'""#
                .to_string(),
        );

        let package_name =
            package_full_name(&program).unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert_eq!(package_name, "Demo.Store_1.0.0_x64__abc");
    }

    #[test]
    fn build_remove_package_script_targets_current_user_sid() {
        let script = build_remove_package_script("Demo.Store_1.0.0_x64__abc");

        assert!(script.contains("WindowsIdentity"));
        assert!(script.contains("-User $userSid"));
        assert!(script.contains("Demo.Store_1.0.0_x64__abc"));
    }

    #[test]
    fn build_package_presence_script_checks_current_user_sid() {
        let script = build_package_presence_script("Demo.Store_1.0.0_x64__abc");

        assert!(script.contains("Get-AppxPackage -User $userSid"));
        assert!(script.contains("Demo.Store_1.0.0_x64__abc"));
    }
}
