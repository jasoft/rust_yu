use std::process::Command;

use crate::modules::common::error::UninstallerError;
use crate::modules::common::text::{build_powershell_script, decode_windows_output};
use crate::modules::lister::models::InstalledProgram;

use super::ProgramRemovalStatus;

pub fn resolve_uninstall_command(program: &InstalledProgram) -> Result<String, UninstallerError> {
    let package_full_name = package_full_name(program)?;
    let script = build_powershell_script(&format!(
        "Remove-AppxPackage -Package '{}' -ErrorAction Stop",
        package_full_name
    ));

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
    if !program.id.trim().is_empty() && program.id.contains('_') {
        return Ok(program.id.trim().to_string());
    }

    if let Some(uninstall_string) = program.uninstall_string.as_deref() {
        if let Some(package_name) = extract_package_name_from_uninstall_string(uninstall_string) {
            return Ok(package_name);
        }
    }

    Err(UninstallerError::StoreApp(format!(
        "无法确定 {} 的 Store 包标识",
        program.name
    )))
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
            &build_powershell_script(&format!(
                "$exists = @(Get-AppxPackage | Where-Object {{ $_.PackageFullName -eq '{}' }}).Count -gt 0\nif ($exists) {{ 'true' }} else {{ 'false' }}",
                package_full_name
            )),
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

#[cfg(test)]
mod tests {
    use super::{
        extract_package_name_from_uninstall_string, package_full_name, resolve_uninstall_command,
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
}
