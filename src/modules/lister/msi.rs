use super::models::{InstallSource, InstalledProgram};
use crate::modules::common::error::UninstallerError;

/// 从卸载注册表快速列出 MSI 产品。
pub fn list_msi_products() -> Result<Vec<InstalledProgram>, UninstallerError> {
    #[cfg(windows)]
    {
        let programs = super::registry::list_registry_programs()?;
        return Ok(programs
            .into_iter()
            .filter(|program| program.install_source == InstallSource::Msi)
            .collect());
    }

    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}
