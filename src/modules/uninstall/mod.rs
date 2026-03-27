pub mod legacy;
pub mod msi;
pub mod store;

use std::time::{Duration, Instant};

use crate::modules::common::error::UninstallerError;
use crate::modules::lister::models::{InstalledProgram, UninstallKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramRemovalStatus {
    pub removed: bool,
    pub still_registered: bool,
    pub install_dir_exists: bool,
    pub store_package_present: bool,
}

pub fn route_name(kind: UninstallKind) -> &'static str {
    match kind {
        UninstallKind::Legacy => "legacy",
        UninstallKind::Msi => "msi",
        UninstallKind::Store => "store",
    }
}

pub fn resolve_uninstall_command(program: &InstalledProgram) -> Result<String, UninstallerError> {
    match program.uninstall_kind {
        UninstallKind::Legacy => legacy::resolve_uninstall_command(program),
        UninstallKind::Msi => msi::resolve_uninstall_command(program),
        UninstallKind::Store => store::resolve_uninstall_command(program),
    }
}

pub async fn wait_for_program_removal(
    program: &InstalledProgram,
    timeout_secs: u64,
) -> Result<ProgramRemovalStatus, UninstallerError> {
    let started_at = Instant::now();
    loop {
        let status = match program.uninstall_kind {
            UninstallKind::Legacy => legacy::check_removal(program)?,
            UninstallKind::Msi => msi::check_removal(program)?,
            UninstallKind::Store => store::check_removal(program)?,
        };

        tracing::info!(
            "等待卸载完成, name={}, kind={}, removed={}, still_registered={}, install_dir_exists={}, store_package_present={}",
            program.name,
            route_name(program.uninstall_kind),
            status.removed,
            status.still_registered,
            status.install_dir_exists,
            status.store_package_present
        );

        if status.removed {
            return Ok(status);
        }

        if started_at.elapsed() >= Duration::from_secs(timeout_secs) {
            return Ok(status);
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
