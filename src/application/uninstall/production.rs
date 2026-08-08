use super::error::{UninstallError, UninstallErrorCode};
use super::ports::{CleanedTrace, RemovalVerification, UninstallPort, UninstallerExecution};
use crate::application::target::build_target_search_query;
use crate::modules::lister::models::InstalledProgram;
use crate::modules::scanner::models::Trace;
use crate::modules::{cleaner, common::process, common::utils, lister, scanner, uninstall};
use async_trait::async_trait;

#[derive(Debug, Default, Clone, Copy)]
pub struct ProductionUninstallPort;

fn core_error(code: UninstallErrorCode, error: impl std::fmt::Display) -> UninstallError {
    UninstallError::new(code, error.to_string())
}

#[async_trait]
impl UninstallPort for ProductionUninstallPort {
    async fn resolve_program_by_id(
        &self,
        program_id: &str,
    ) -> Result<InstalledProgram, UninstallError> {
        let programs = lister::list_programs_with_cache(build_target_search_query())
            .map_err(|error| core_error(UninstallErrorCode::JobNotFound, error))?
            .programs;
        programs
            .into_iter()
            .find(|program| program.id.eq_ignore_ascii_case(program_id))
            .ok_or_else(|| UninstallError::new(UninstallErrorCode::JobNotFound, "未找到指定程序"))
    }

    async fn save_snapshot(&self, program: &InstalledProgram) -> Result<(), UninstallError> {
        lister::storage::save_program_snapshot(std::slice::from_ref(program))
            .map_err(|error| core_error(UninstallErrorCode::UninstallerFailed, error))
    }

    async fn ensure_administrator(&self) -> Result<(), UninstallError> {
        utils::ensure_running_as_administrator().map_err(|error| {
            UninstallError::new(UninstallErrorCode::AdminRequired, error.to_string())
        })
    }

    async fn run_uninstaller(
        &self,
        program: &InstalledProgram,
        timeout_secs: u64,
    ) -> Result<UninstallerExecution, UninstallError> {
        let command = match program.preferred_uninstall_string() {
            Some(command) => command.to_string(),
            None => uninstall::resolve_uninstall_command(program)
                .map_err(|error| core_error(UninstallErrorCode::UninstallerFailed, error))?,
        };
        let normalized = utils::normalize_uninstall_command(&command);
        let result = process::run_uninstall_command(&normalized, timeout_secs)
            .await
            .map_err(|error| core_error(UninstallErrorCode::UninstallerFailed, error))?;
        Ok(UninstallerExecution {
            successful: result.classification.successful,
            exit_code: result.exit_code,
            reboot_required: result.classification.reboot_required,
            user_cancelled: result.classification.user_cancelled
                || matches!(
                    result.completion_status,
                    process::UninstallCompletionStatus::InterruptedByUser
                ),
            used_job_object: result.used_job_object,
        })
    }

    async fn verify_removal(
        &self,
        program: &InstalledProgram,
        timeout_secs: u64,
    ) -> Result<RemovalVerification, UninstallError> {
        let result = uninstall::wait_for_program_removal(program, timeout_secs)
            .await
            .map_err(|error| core_error(UninstallErrorCode::RemovalNotConfirmed, error))?;
        Ok(RemovalVerification {
            removed: result.removed,
            still_registered: result.still_registered,
            install_dir_exists: result.install_dir_exists,
            store_package_present: result.store_package_present,
        })
    }

    async fn scan_residues(
        &self,
        program: &InstalledProgram,
    ) -> Result<Vec<Trace>, UninstallError> {
        scanner::scan_all_traces(&program.name, None)
            .await
            .map(|traces| traces.into_iter().filter(|trace| trace.exists).collect())
            .map_err(|error| core_error(UninstallErrorCode::ResidueScanFailed, error))
    }

    async fn clean_traces(&self, traces: &[Trace]) -> Result<Vec<CleanedTrace>, UninstallError> {
        let results = cleaner::clean_traces(traces.to_vec(), true)
            .await
            .map_err(|error| core_error(UninstallErrorCode::CleanupFailed, error))?;
        Ok(results
            .into_iter()
            .map(|result| CleanedTrace {
                trace_id_hash: result.trace_id.bytes().fold(0u64, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(u64::from(byte))
                }),
                success: result.success,
                bytes_freed: result.bytes_freed,
            })
            .collect())
    }

    async fn invalidate_cache(&self, program_id: &str) -> Result<(), UninstallError> {
        lister::storage::invalidate_scan_cache_for_program(program_id)
            .map_err(|error| core_error(UninstallErrorCode::CleanupFailed, error))
    }
}
