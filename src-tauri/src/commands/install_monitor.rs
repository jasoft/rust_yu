use rust_yu_lib::install_monitor::{
    self, InstallMonitorPlan, InstallMonitorSession, InstallMonitorSessionInfo,
    InstallMonitorStartRequest, MonitorExport, MonitorExportFormat,
};
use rust_yu_lib::scanner::models::Trace;

use super::CommandError;

#[tauri::command]
pub async fn plan_install_monitor(
    request: InstallMonitorStartRequest,
) -> Result<InstallMonitorPlan, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        install_monitor::build_plan(
            &request.program,
            &request.extra_file_roots,
            &request.extra_registry_roots,
        )
    })
    .await
    .map_err(|error| CommandError::new(format!("生成安装监控计划任务失败: {error}")))
}

#[tauri::command]
pub async fn start_install_monitor(
    request: InstallMonitorStartRequest,
) -> Result<InstallMonitorSessionInfo, CommandError> {
    tauri::async_runtime::spawn_blocking(move || install_monitor::start_monitor(request))
        .await
        .map_err(|error| CommandError::new(format!("开始安装监控任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn complete_install_monitor(
    session_id: String,
) -> Result<InstallMonitorSession, CommandError> {
    tauri::async_runtime::spawn_blocking(move || install_monitor::complete_monitor(&session_id))
        .await
        .map_err(|error| CommandError::new(format!("完成安装监控任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_install_monitor_sessions() -> Result<Vec<InstallMonitorSessionInfo>, CommandError>
{
    tauri::async_runtime::spawn_blocking(install_monitor::list_sessions)
        .await
        .map_err(|error| CommandError::new(format!("读取安装监控会话任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_install_monitor_session(
    session_id: String,
) -> Result<InstallMonitorSession, CommandError> {
    tauri::async_runtime::spawn_blocking(move || install_monitor::get_session(&session_id))
        .await
        .map_err(|error| CommandError::new(format!("读取安装监控详情任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_install_monitor_traces(session_id: String) -> Result<Vec<Trace>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || install_monitor::traces_for_session(&session_id))
        .await
        .map_err(|error| CommandError::new(format!("生成安装监控证据任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn export_install_monitor(
    session_id: String,
    format: String,
) -> Result<MonitorExport, CommandError> {
    let format = MonitorExportFormat::parse(&format)
        .ok_or_else(|| CommandError::with_code("invalid_format", "仅支持 json 或 csv 导出"))?;
    tauri::async_runtime::spawn_blocking(move || {
        install_monitor::export_session(&session_id, format)
    })
    .await
    .map_err(|error| CommandError::new(format!("导出安装监控报告任务失败: {error}")))?
    .map_err(CommandError::from)
}
