use rust_yu_lib::backup::{
    self, BackupPlan, BackupRestoreResult, BackupSession, BackupSessionInfo,
};
use rust_yu_lib::reporter::{history, models::UninstallerReport};
use rust_yu_lib::scanner::models::Trace;
use std::collections::HashMap;

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
    tauri::async_runtime::spawn_blocking(|| {
        let mut sessions = backup::list_sessions()?;
        // 旧版会话只保存了泛化标题。报告中的 backup_id 是清理时写入的真实
        // 关联键，可安全用于补全显示名；报告缺失或关联冲突时保持原值。
        if let Ok(reports) = history::list_reports() {
            enrich_session_reasons(&mut sessions, &reports);
        }
        Ok::<Vec<BackupSessionInfo>, rust_yu_lib::UninstallerError>(sessions)
    })
    .await
    .map_err(|error| CommandError::new(format!("读取备份会话任务失败: {error}")))?
    .map_err(CommandError::from)
}

fn enrich_session_reasons(sessions: &mut [BackupSessionInfo], reports: &[UninstallerReport]) {
    let mut program_by_backup = HashMap::<String, Option<String>>::new();
    for report in reports {
        for backup_id in report
            .traces_removed
            .iter()
            .filter_map(|result| result.backup_id.as_ref())
        {
            program_by_backup
                .entry(backup_id.clone())
                .and_modify(|program_name| {
                    if program_name.as_deref() != Some(report.program_name.as_str()) {
                        *program_name = None;
                    }
                })
                .or_insert_with(|| Some(report.program_name.clone()));
        }
    }
    for session in sessions {
        if session.reason.trim().is_empty() || session.reason == "卸载残留清理" {
            if let Some(Some(program_name)) = program_by_backup.get(&session.id) {
                session.reason = format!("{program_name} · 卸载残留清理");
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::enrich_session_reasons;
    use rust_yu_lib::backup::{BackupSessionInfo, BackupSessionStatus};
    use rust_yu_lib::cleaner::models::CleanResult;
    use rust_yu_lib::reporter::models::UninstallerReport;

    #[test]
    fn legacy_generic_backup_title_is_enriched_from_real_report_link() {
        let backup_id = "11111111-1111-4111-8111-111111111111".to_string();
        let report =
            UninstallerReport::new("7-Zip 24.09".to_string()).with_results(vec![CleanResult {
                trace_id: "trace-1".to_string(),
                path: r"C:\Program Files\7-Zip\leftover.log".to_string(),
                success: true,
                error: None,
                bytes_freed: 64,
                backup_id: Some(backup_id.clone()),
            }]);
        let mut sessions = vec![BackupSessionInfo {
            id: backup_id,
            created_at: report.generated_at,
            reason: "卸载残留清理".to_string(),
            status: BackupSessionStatus::PartiallyCleaned,
            item_count: 1,
            restorable_count: 1,
            failed_count: 0,
            bytes: 64,
        }];

        enrich_session_reasons(&mut sessions, &[report]);

        assert_eq!(sessions[0].reason, "7-Zip 24.09 · 卸载残留清理");
    }
}
