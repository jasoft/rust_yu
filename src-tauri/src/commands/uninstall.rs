use rust_yu_lib::application::uninstall::{
    clean_uninstall_residues_with_progress as run_clean_uninstall_residues,
    execute_uninstall_with_progress as run_execute_uninstall, plan_uninstall as run_plan_uninstall,
    CleanupSelection, ProductionUninstallPort, UninstallEvent, UninstallJob, UninstallPhase,
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
    let result = run_execute_uninstall(&port, &mut job, request.timeout_secs, |event| {
        emit_job_event(&app, event);
    })
    .await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    finalize_job_result(result, &job)?;
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
    let result = run_clean_uninstall_residues(&port, &mut job, request.selection, |event| {
        emit_job_event(&app, event);
    })
    .await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    finalize_job_result(result, &job)?;
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
        |event| {
            emit_job_event(&app, event);
        },
    )
    .await;
    coordinator
        .commit_operation(job.clone())
        .map_err(CommandError::from)?;
    finalize_job_result(result, &job)?;
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
        emit_job_event(app, event);
    }
}

fn emit_job_event(app: &AppHandle, event: &UninstallEvent) {
    let _ = app.emit(UNINSTALL_JOB_PROGRESS_EVENT, event);
}

fn finalize_job_result<T>(
    result: Result<T, rust_yu_lib::application::uninstall::UninstallError>,
    job: &UninstallJob,
) -> Result<(), CommandError> {
    let operation_result = result.map_err(CommandError::from);
    let report_result = if job.phase.is_terminal() {
        rust_yu_lib::reporter::history::save_job_report(job)
            .map(|_| ())
            .map_err(|error| CommandError::new(format!("卸载报告保存失败: {error}")))
    } else {
        Ok(())
    };
    match (operation_result, report_result) {
        (Ok(_), Ok(())) => Ok(()),
        (Ok(_), Err(report_error)) => Err(report_error),
        (Err(operation_error), Ok(())) => Err(operation_error),
        (Err(operation_error), Err(report_error)) => Err(CommandError::new(format!(
            "{}；{}",
            operation_error.message, report_error.message
        ))),
    }
}
