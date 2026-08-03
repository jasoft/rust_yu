use std::path::PathBuf;

use crate::modules::common::error::UninstallerError;
use crate::modules::lister;
use crate::modules::lister::models::InstalledProgram;

use super::ProgramRemovalStatus;

pub fn resolve_uninstall_command(program: &InstalledProgram) -> Result<String, UninstallerError> {
    program
        .preferred_uninstall_string()
        .map(str::to_string)
        .ok_or_else(|| UninstallerError::NotFound(format!("未找到 {} 的卸载命令", program.name)))
}

pub fn check_removal(program: &InstalledProgram) -> Result<ProgramRemovalStatus, UninstallerError> {
    let install_location = program
        .install_location
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let still_registered = lister::registry::registry_program_exists(&program.name)?;
    let install_dir_exists = install_location
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);

    // Legacy 卸载器经常会主动保留日志、配置等残留文件；注册表卸载项消失
    // 后即可进入残留扫描，不能把整个安装目录仍存在误判为卸载失败。
    Ok(ProgramRemovalStatus {
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
    fn resolve_uninstall_command_prefers_quiet_variant_for_legacy_program() {
        let mut program = InstalledProgram::new("LegacyApp".to_string(), InstallSource::Registry);
        program.uninstall_string = Some(r#""C:\Legacy\uninstall.exe""#.to_string());
        program.quiet_uninstall_string = Some(r#""C:\Legacy\uninstall.exe" /S"#.to_string());

        let command = resolve_uninstall_command(&program)
            .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert_eq!(command, r#""C:\Legacy\uninstall.exe" /S"#);
    }
}
