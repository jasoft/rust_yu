use rust_yu_lib::file_shredder::{self, ShredMethod, ShredPlan, ShredRequest, ShredResult};
use tauri::{AppHandle, Emitter};

use super::{require_administrator, CommandError};

#[tauri::command]
pub async fn plan_file_shred(
    paths: Vec<String>,
    method: ShredMethod,
) -> Result<ShredPlan, CommandError> {
    tauri::async_runtime::spawn_blocking(move || file_shredder::plan(&paths, method))
        .await
        .map_err(|error| CommandError::new(format!("文件粉碎分析任务失败: {error}")))?
        .map_err(|error| CommandError::with_code("file_shred_plan_failed", error.to_string()))
}

#[tauri::command]
pub async fn execute_file_shred(
    app: AppHandle,
    request: ShredRequest,
) -> Result<ShredResult, CommandError> {
    require_administrator()?;
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        file_shredder::execute_with_progress(request, |event| {
            let _ = progress_app.emit("file-shred-progress", event);
        })
    })
    .await
    .map_err(|error| CommandError::new(format!("文件粉碎任务失败: {error}")))?
    .map_err(|error| CommandError::with_code("file_shred_failed", error.to_string()))?;

    let _ = app.emit(
        "file-shred-log",
        format!(
            "粉碎完成：{} 个文件，{} 个目录，{} 个失败",
            result.shredded_files,
            result.deleted_directories,
            result.failures.len()
        ),
    );
    Ok(result)
}
