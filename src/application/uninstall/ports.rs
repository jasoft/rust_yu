use super::error::UninstallError;
use crate::modules::lister::models::InstalledProgram;
use crate::modules::scanner::models::Trace;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UninstallerExecution {
    pub successful: bool,
    pub exit_code: Option<u32>,
    pub reboot_required: bool,
    pub user_cancelled: bool,
    pub used_job_object: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovalVerification {
    pub removed: bool,
    pub still_registered: bool,
    pub install_dir_exists: bool,
    pub store_package_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanedTrace {
    pub trace_id_hash: u64,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_freed: u64,
    pub backup_id: Option<String>,
}

#[async_trait]
pub trait UninstallPort: Send + Sync {
    async fn resolve_program_by_id(
        &self,
        program_id: &str,
    ) -> Result<InstalledProgram, UninstallError>;

    async fn save_snapshot(&self, program: &InstalledProgram) -> Result<(), UninstallError>;

    async fn ensure_administrator(&self) -> Result<(), UninstallError>;

    async fn run_uninstaller(
        &self,
        program: &InstalledProgram,
        timeout_secs: u64,
    ) -> Result<UninstallerExecution, UninstallError>;

    async fn verify_removal(
        &self,
        program: &InstalledProgram,
        timeout_secs: u64,
    ) -> Result<RemovalVerification, UninstallError>;

    async fn scan_residues(&self, program: &InstalledProgram)
        -> Result<Vec<Trace>, UninstallError>;

    async fn clean_traces(&self, traces: &[Trace]) -> Result<Vec<CleanedTrace>, UninstallError>;

    async fn invalidate_cache(&self, program_id: &str) -> Result<(), UninstallError>;
}
