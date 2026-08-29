use super::error::{UninstallError, UninstallErrorCode};
use super::ports::{CleanedTrace, RemovalVerification, UninstallPort, UninstallerExecution};
use crate::application::target::build_target_search_query;
use crate::modules::lister::models::{InstallSourceSelector, InstalledProgram, ListProgramsQuery};
use crate::modules::scanner::models::Trace;
use crate::modules::{cleaner, common::process, common::utils, lister, scanner, uninstall};
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Default, Clone)]
pub struct ProductionUninstallPort {
    cancellation: Option<Arc<AtomicBool>>,
}

impl ProductionUninstallPort {
    pub fn with_cancellation(cancellation: Arc<AtomicBool>) -> Self {
        Self {
            cancellation: Some(cancellation),
        }
    }

    fn cancellation_requested(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::SeqCst))
    }

    async fn wait_for_cancellation(&self) {
        while !self.cancellation_requested() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

fn core_error(code: UninstallErrorCode, error: impl std::fmt::Display) -> UninstallError {
    UninstallError::new(code, error.to_string())
}

/// 返回解析 program_id 时依次尝试的来源选择器。
///
/// `registry:` 稳定 ID 同时覆盖 Registry 与 Msi 两类 ARP 条目；Registry 选择器的
/// 视图只包含非 MSI 分类，如果只查 Registry 视图，MSI 程序会在规划阶段被误报
/// “未找到指定程序”。两个视图都查不到时才允许回退到 Store 选择器（仅限
/// Store 包 ID），避免规划卸载时逐包读取 Microsoft Store 清单阻塞 GUI。
fn resolve_program_selectors(program_id: &str) -> &'static [InstallSourceSelector] {
    if program_id
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("registry:")
    {
        &[InstallSourceSelector::Registry, InstallSourceSelector::Msi]
    } else {
        &[InstallSourceSelector::Store]
    }
}

#[async_trait]
impl UninstallPort for ProductionUninstallPort {
    async fn resolve_program_by_id(
        &self,
        program_id: &str,
    ) -> Result<InstalledProgram, UninstallError> {
        for selector in resolve_program_selectors(program_id) {
            let mut query = build_target_search_query();
            query.source = *selector;
            let programs = lister::list_programs_with_cache(query)
                .map_err(|error| core_error(UninstallErrorCode::JobNotFound, error))?
                .programs;
            if let Some(program) = programs
                .into_iter()
                .find(|program| program.id.eq_ignore_ascii_case(program_id))
            {
                return Ok(program);
            }
        }
        Err(UninstallError::new(UninstallErrorCode::JobNotFound, "未找到指定程序"))
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
        let scan = scanner::scan_all_traces_for_program(program, None);
        tokio::pin!(scan);
        let traces = if self.cancellation.is_some() {
            tokio::select! {
                result = &mut scan => result
                    .map_err(|error| core_error(UninstallErrorCode::ResidueScanFailed, error))?,
                () = self.wait_for_cancellation() => {
                    return Err(UninstallError::new(
                        UninstallErrorCode::UninstallerCancelled,
                        "用户已取消残留扫描",
                    ));
                }
            }
        } else {
            scan.await
                .map_err(|error| core_error(UninstallErrorCode::ResidueScanFailed, error))?
        };
        Ok(traces.into_iter().filter(|trace| trace.exists).collect())
    }

    async fn clean_traces(&self, traces: &[Trace]) -> Result<Vec<CleanedTrace>, UninstallError> {
        let results = cleaner::clean_reviewed_traces(traces.to_vec(), true)
            .await
            .map_err(|error| core_error(UninstallErrorCode::CleanupFailed, error))?;
        Ok(results
            .into_iter()
            .map(|result| CleanedTrace {
                trace_id_hash: result.trace_id.bytes().fold(0u64, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(u64::from(byte))
                }),
                success: result.success,
                error: result.error,
                bytes_freed: result.bytes_freed,
                backup_id: result.backup_id,
            })
            .collect())
    }

    async fn invalidate_cache(&self, program_id: &str) -> Result<(), UninstallError> {
        lister::storage::invalidate_scan_cache_for_program(program_id)
            .map_err(|error| core_error(UninstallErrorCode::CleanupFailed, error))
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_program_selectors;
    use crate::modules::lister::models::InstallSourceSelector;

    #[test]
    fn registry_stable_ids_try_registry_then_msi_views_without_store() {
        let selectors = resolve_program_selectors(
            r"registry:HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Demo",
        );
        assert_eq!(
            selectors,
            &[InstallSourceSelector::Registry, InstallSourceSelector::Msi]
        );
    }

    #[test]
    fn store_package_ids_use_store_enumeration() {
        let selectors =
            resolve_program_selectors("Publisher.Demo_1.0.0.0_x64__example");
        assert_eq!(selectors, &[InstallSourceSelector::Store]);
    }
}
