use rust_yu_lib::application::uninstall::{
    clean_uninstall_residues as run_clean_uninstall_residues,
    execute_uninstall as run_execute_uninstall, plan_uninstall as run_plan_uninstall,
    CleanupSelection, ProductionUninstallPort, UninstallJob, UninstallPhase,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::{require_administrator, CommandError};
use crate::state::UninstallCoordinator;

pub const UNINSTALL_JOB_PROGRESS_EVENT: &str = "uninstall-job-progress";

#[derive(Debug, Clone, Deserialize)]
pub struct PlanUninstallRequest {
    pub program_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteUninstallRequest {
    pub job_id: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanUninstallResiduesRequest {
    pub job_id: String,
    pub selection: CleanupSelection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinishUninstallRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UninstallJobResponse {
    pub job: UninstallJob,
}

fn default_timeout() -> u64 {
    120
}

#[tauri::command]
pub async fn plan_uninstall(
    app: AppHandle,
    coordinator: State<'_, UninstallCoordinator>,
    request: PlanUninstallRequest,
) -> Result<UninstallJobResponse, CommandError> {
    require_administrator()?;
    coordinator.begin_plan().map_err(CommandError::from)?;
    let port = ProductionUninstallPort;
    let result = run_plan_uninstall(&port, &request.program_id).await;
    let job = match result {
        Ok(job) => job,
        Err(error) => {
            let _ = coordinator.abort_plan();
            return Err(CommandError::from(error));
        }
    };
    coordinator
        .commit_plan(job.clone())
        .map_err(CommandError::from)?;
    emit_job_events(&app, &job);
    Ok(UninstallJobResponse { job })
}

#[tauri::command]
pub async fn execute_uninstall(
    app: AppHandle,
    coordinator: State<'_, UninstallCoordinator>,
    request: ExecuteUninstallRequest,
) -> Result<UninstallJobResponse, CommandError> {
    require_administrator()?;
    let mut job = coordinator
        .begin_operation(&request.job_id, UninstallPhase::Planned)
        .map_err(CommandError::from)?;
    let port = ProductionUninstallPort;
    let result = run_execute_uninstall(&port, &mut job, request.timeout_secs).await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    emit_job_events(&app, &job);
    result.map_err(CommandError::from)?;
    Ok(UninstallJobResponse { job })
}

#[tauri::command]
pub async fn clean_uninstall_residues(
    app: AppHandle,
    coordinator: State<'_, UninstallCoordinator>,
    request: CleanUninstallResiduesRequest,
) -> Result<UninstallJobResponse, CommandError> {
    require_administrator()?;
    let mut job = coordinator
        .begin_operation(&request.job_id, UninstallPhase::AwaitingCleanupConfirmation)
        .map_err(CommandError::from)?;
    let port = ProductionUninstallPort;
    let result = run_clean_uninstall_residues(&port, &mut job, request.selection).await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    emit_job_events(&app, &job);
    result.map_err(CommandError::from)?;
    Ok(UninstallJobResponse { job })
}

#[tauri::command]
pub async fn finish_uninstall(
    app: AppHandle,
    coordinator: State<'_, UninstallCoordinator>,
    request: FinishUninstallRequest,
) -> Result<UninstallJobResponse, CommandError> {
    require_administrator()?;
    let mut job = coordinator
        .begin_operation(&request.job_id, UninstallPhase::AwaitingCleanupConfirmation)
        .map_err(CommandError::from)?;
    let port = ProductionUninstallPort;
    let result = run_clean_uninstall_residues(
        &port,
        &mut job,
        CleanupSelection {
            trace_ids: Vec::new(),
            confirm: true,
        },
    )
    .await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    emit_job_events(&app, &job);
    result.map_err(CommandError::from)?;
    Ok(UninstallJobResponse { job })
}

#[tauri::command]
pub fn get_uninstall_job(
    coordinator: State<'_, UninstallCoordinator>,
    job_id: String,
) -> Result<UninstallJobResponse, CommandError> {
    let job = coordinator.snapshot(&job_id).map_err(CommandError::from)?;
    Ok(UninstallJobResponse { job })
}

fn emit_job_events(app: &AppHandle, job: &UninstallJob) {
    for event in &job.events {
        let _ = app.emit(UNINSTALL_JOB_PROGRESS_EVENT, event);
    }
}
