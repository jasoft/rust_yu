use rust_yu_lib::fluent_cleaner::{
    self, CleanSelection, CleanerCatalog, CleanerCleanResult, CleanerScanResult,
};
use tauri::{AppHandle, Emitter};

use super::{require_administrator, CommandError};

#[tauri::command]
pub async fn list_cleaner_entries() -> Result<CleanerCatalog, CommandError> {
    tauri::async_runtime::spawn_blocking(fluent_cleaner::list_detected_entries)
        .await
        .map_err(|error| CommandError::new(format!("清理规则检测任务失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn scan_cleaner_entries(
    app: AppHandle,
    entry_ids: Vec<String>,
) -> Result<CleanerScanResult, CommandError> {
    let _ = app.emit(
        "fluent-cleaner-log",
        format!("开始分析 {} 条规则", entry_ids.len()),
    );
    let result =
        tauri::async_runtime::spawn_blocking(move || fluent_cleaner::analyze_entries(&entry_ids))
            .await
            .map_err(|error| CommandError::new(format!("清理分析任务失败: {error}")))?
            .map_err(CommandError::from)?;
    let _ = app.emit(
        "fluent-cleaner-log",
        format!("分析完成：发现 {} 个目标", result.targets.len()),
    );
    Ok(result)
}

#[tauri::command]
pub async fn clean_cleaner_entries(
    app: AppHandle,
    selection: CleanSelection,
) -> Result<CleanerCleanResult, CommandError> {
    require_administrator()?;
    let _ = app.emit(
        "fluent-cleaner-log",
        if selection.dry_run {
            "开始模拟清理"
        } else {
            "开始执行清理"
        },
    );
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        fluent_cleaner::clean_selection_with_progress(selection, |item| {
            let status = if item.success { "已处理" } else { "失败" };
            let _ = progress_app.emit("fluent-cleaner-log", format!("{status}: {}", item.path));
        })
    })
    .await
    .map_err(|error| CommandError::new(format!("系统清理任务失败: {error}")))?
    .map_err(CommandError::from)?;
    let _ = app.emit(
        "fluent-cleaner-log",
        format!("清理完成：释放 {} 字节", result.bytes_freed),
    );
    Ok(result)
}
