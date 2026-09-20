use rust_yu_lib::backup::{self, BackupSessionInfo};
use rust_yu_lib::reporter::evidence::{self, EvidenceBundleExport};
use rust_yu_lib::reporter::history::{self, ReportExport, ReportExportFormat};
use rust_yu_lib::reporter::models::UninstallerReport;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::CommandError;

#[derive(Debug, Clone, Serialize)]
pub struct ReportInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub path: String,
    pub success: bool,
    pub traces_count: usize,
    pub cleaned_count: usize,
    pub failed_count: usize,
    pub warning_count: usize,
    pub formats: Vec<String>,
    /// 与该报告清理结果实际关联的持久化备份；不按名称或路径猜测关联。
    pub backup_sessions: Vec<BackupSessionInfo>,
}

#[tauri::command]
pub async fn get_reports() -> Result<Vec<ReportInfo>, CommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let reports = history::list_reports()?;
        // 报告历史本身不应因某个备份清单损坏而整体不可读；恢复中心仍会
        // 单独暴露备份读取错误，历史页则保留报告并隐藏不可验证的恢复入口。
        let backup_sessions = backup::list_sessions()
            .unwrap_or_default()
            .into_iter()
            .map(|session| (session.id.clone(), session))
            .collect::<HashMap<_, _>>();
        Ok::<Vec<ReportInfo>, rust_yu_lib::UninstallerError>(
            reports
                .into_iter()
                .map(|report| report_info(report, &backup_sessions))
                .collect(),
        )
    })
    .await
    .map_err(|error| CommandError::new(format!("读取报告历史任务失败: {error}")))?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_report(report_id: String) -> Result<UninstallerReport, CommandError> {
    tauri::async_runtime::spawn_blocking(move || history::load_report(&report_id))
        .await
        .map_err(|error| CommandError::new(format!("读取报告详情任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn export_report(
    report_id: String,
    format: String,
) -> Result<ReportExport, CommandError> {
    let format = ReportExportFormat::parse(&format).ok_or_else(|| {
        CommandError::with_code("invalid_format", "仅支持 json、html 或 text 导出")
    })?;
    tauri::async_runtime::spawn_blocking(move || history::export_report(&report_id, format))
        .await
        .map_err(|error| CommandError::new(format!("导出卸载报告任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn delete_report(report_id: String) -> Result<bool, CommandError> {
    tauri::async_runtime::spawn_blocking(move || history::delete_report(&report_id))
        .await
        .map_err(|error| CommandError::new(format!("删除报告任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn export_evidence_bundle(
    report_id: String,
) -> Result<EvidenceBundleExport, CommandError> {
    tauri::async_runtime::spawn_blocking(move || evidence::export_evidence_bundle(&report_id))
        .await
        .map_err(|error| CommandError::new(format!("导出专业证据包任务失败: {error}")))?
        .map_err(CommandError::from)
}

fn report_info(
    report: UninstallerReport,
    backup_sessions: &HashMap<String, BackupSessionInfo>,
) -> ReportInfo {
    let id = report.id.clone();
    let formats = ["json", "html", "txt"]
        .into_iter()
        .filter(|extension| {
            history::report_file_path(&id, extension).is_ok_and(|path| path.is_file())
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut seen_backup_ids = HashSet::new();
    let linked_backup_sessions = report
        .traces_removed
        .iter()
        .filter_map(|result| result.backup_id.as_ref())
        .filter(|backup_id| seen_backup_ids.insert((*backup_id).clone()))
        .filter_map(|backup_id| backup_sessions.get(backup_id).cloned())
        .collect::<Vec<_>>();
    ReportInfo {
        id: report.id,
        name: report.program_name,
        created_at: report.generated_at.to_rfc3339(),
        path: history::report_file_path(&id, "json")
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        success: report.success,
        traces_count: report.traces_found.len(),
        cleaned_count: report
            .traces_removed
            .iter()
            .filter(|item| item.success)
            .count(),
        failed_count: report
            .traces_removed
            .iter()
            .filter(|item| !item.success)
            .count(),
        warning_count: report.warnings.len(),
        formats,
        backup_sessions: linked_backup_sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::report_info;
    use rust_yu_lib::backup::{BackupSessionInfo, BackupSessionStatus};
    use rust_yu_lib::cleaner::models::CleanResult;
    use rust_yu_lib::reporter::models::UninstallerReport;
    use std::collections::HashMap;

    #[test]
    fn report_summary_keeps_real_name_and_links_each_backup_once() {
        let backup_id = "11111111-1111-4111-8111-111111111111".to_string();
        let report =
            UninstallerReport::new("Rust Yu Legacy Test App".to_string()).with_results(vec![
                CleanResult {
                    trace_id: "file".to_string(),
                    path: r"C:\Program Files\RustYu Legacy Test App\leftover.log".to_string(),
                    success: true,
                    error: None,
                    bytes_freed: 32,
                    backup_id: Some(backup_id.clone()),
                },
                CleanResult {
                    trace_id: "registry".to_string(),
                    path: r"HKCU\Software\RustYuLegacyTest".to_string(),
                    success: true,
                    error: None,
                    bytes_freed: 0,
                    backup_id: Some(backup_id.clone()),
                },
            ]);
        let created_at = report.generated_at;
        let backup = BackupSessionInfo {
            id: backup_id.clone(),
            created_at,
            reason: "Rust Yu Legacy Test App · 卸载残留清理".to_string(),
            status: BackupSessionStatus::PartiallyCleaned,
            item_count: 2,
            restorable_count: 2,
            failed_count: 0,
            bytes: 32,
        };
        let backups = HashMap::from([(backup_id, backup)]);

        let summary = report_info(report, &backups);

        assert_eq!(summary.name, "Rust Yu Legacy Test App");
        assert_eq!(summary.backup_sessions.len(), 1);
        assert_eq!(summary.backup_sessions[0].restorable_count, 2);
    }
}
