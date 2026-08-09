use rust_yu_lib::backup::{
    self, BackupPlan, BackupRestoreResult, BackupSession, BackupSessionInfo,
};
use rust_yu_lib::scanner::models::Trace;

use super::{require_administrator, CommandError};

/// 只读生成备份计划，供恢复中心和清理确认页展示预计保护范围。
#[tauri::command]
pub async fn plan_backup(traces: Vec<Trace>) -> Result<BackupPlan, CommandError> {
    tauri::async_runtime::spawn_blocking(move || backup::plan_for_traces(&traces))
        .await
        .map_err(|error| CommandError::new(format!("生成备份计划任务失败: {error}")))
}

/// 列出本机持久化的备份会话。
#[tauri::command]
pub async fn list_backup_sessions() -> Result<Vec<BackupSessionInfo>, CommandError> {
    tauri::async_runtime::spawn_blocking(backup::list_sessions)
        .await
        .map_err(|error| CommandError::new(format!("读取备份会话任务失败: {error}")))?
        .map_err(CommandError::from)
}

/// 查看一个备份会话的逐项状态，用于恢复失败后的人工复核。
#[tauri::command]
pub async fn get_backup_session(session_id: String) -> Result<BackupSession, CommandError> {
    tauri::async_runtime::spawn_blocking(move || backup::get_session(&session_id))
        .await
        .map_err(|error| CommandError::new(format!("读取备份详情任务失败: {error}")))?
        .map_err(CommandError::from)
}

/// 恢复指定会话。恢复逻辑始终拒绝覆盖用户在清理后新建的内容，失败项可再次调用重试。
#[tauri::command]
pub async fn restore_backup_session(
    session_id: String,
) -> Result<BackupRestoreResult, CommandError> {
    require_administrator()?;
    tauri::async_runtime::spawn_blocking(move || backup::restore_session(&session_id))
        .await
        .map_err(|error| CommandError::new(format!("恢复备份任务失败: {error}")))?
        .map_err(CommandError::from)
}
