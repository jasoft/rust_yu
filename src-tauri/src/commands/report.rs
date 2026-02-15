use rust_yu_lib::reporter::models::UninstallerReport;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub path: String,
}

fn get_reports_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rust-yu")
        .join("reports")
}

#[tauri::command]
pub async fn get_reports() -> Result<Vec<ReportInfo>, String> {
    let reports_dir = get_reports_dir();

    if !reports_dir.exists() {
        return Ok(vec![]);
    }

    let mut reports = Vec::new();

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("读取报告目录失败: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(report) = serde_json::from_str::<UninstallerReport>(&content) {
                    reports.push(ReportInfo {
                        id: report.id,
                        name: report.program_name,
                        created_at: report.generated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    // 按创建时间排序，最新的在前面
    reports.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(reports)
}

#[tauri::command]
pub async fn delete_report(report_id: String) -> Result<bool, String> {
    let reports_dir = get_reports_dir();

    // 尝试删除 JSON 文件
    let json_path = reports_dir.join(format!("{}.json", report_id));
    if json_path.exists() {
        std::fs::remove_file(&json_path)
            .map_err(|e| format!("删除报告文件失败: {}", e))?;
    }

    // 同时删除 HTML 文件
    let html_path = reports_dir.join(format!("{}.html", report_id));
    if html_path.exists() {
        std::fs::remove_file(&html_path)
            .map_err(|e| format!("删除HTML文件失败: {}", e))?;
    }

    Ok(true)
}
