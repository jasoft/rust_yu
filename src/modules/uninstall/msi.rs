use std::path::PathBuf;

use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::lister;
use crate::modules::lister::models::InstalledProgram;

use super::ProgramRemovalStatus;

pub fn resolve_uninstall_command(program: &InstalledProgram) -> Result<String, UninstallerError> {
    let uninstall_string = program.preferred_uninstall_string().ok_or_else(|| {
        UninstallerError::NotFound(format!("未找到 {} 的 MSI 卸载命令", program.name))
    })?;

    Ok(utils::normalize_uninstall_command(uninstall_string))
}

pub fn check_removal(program: &InstalledProgram) -> Result<ProgramRemovalStatus, UninstallerError> {
    let expected_name = program.name.to_lowercase();
    let install_location = program
        .install_location
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let still_registered = lister::list_all_programs(Some(program.install_source), None)?
        .into_iter()
        .any(|candidate| candidate.name.to_lowercase() == expected_name);
    let install_dir_exists = install_location
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);

    Ok(ProgramRemovalStatus {
        // MSI 更保守：只要产品条目消失，就视为主体卸载完成。
        removed: !still_registered,
        still_registered,
        install_dir_exists,
        store_package_present: false,
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_uninstall_command;
    use crate::modules::lister::models::{InstallSource, InstalledProgram};

    #[test]
    fn resolve_uninstall_command_normalizes_msi_maintenance_mode() {
        let mut program = InstalledProgram::new("Demo MSI".to_string(), InstallSource::Msi);
        program.uninstall_string = Some("msiexec /I{ABC-123}".to_string());

        let command = resolve_uninstall_command(&program)
            .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert_eq!(command, "msiexec /X{ABC-123} /quiet /norestart");
    }
}
