use rust_yu_lib::reporter::evidence::{self, EvidenceBundleExport};
use rust_yu_lib::reporter::history::{self, ReportExport, ReportExportFormat};
use rust_yu_lib::reporter::models::UninstallerReport;
use serde::Serialize;

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
}

#[tauri::command]
pub async fn get_reports() -> Result<Vec<ReportInfo>, CommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        history::list_reports().map(|reports| reports.into_iter().map(report_info).collect())
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

fn report_info(report: UninstallerReport) -> ReportInfo {
    let id = report.id.clone();
    let formats = ["json", "html", "txt"]
        .into_iter()
        .filter(|extension| {
            history::report_file_path(&id, extension).is_ok_and(|path| path.is_file())
        })
        .map(str::to_string)
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
    }
}
