use rust_yu_lib::browser_cleaner::{
    BrowserCleanupRequest, BrowserCleanupResult, BrowserScanResult,
};

use super::{require_administrator, CommandError};

#[tauri::command]
pub async fn scan_browser_data() -> Result<BrowserScanResult, CommandError> {
    // 浏览器扫描包含大量小文件 I/O，必须放入阻塞线程池，避免界面假死。
    tauri::async_runtime::spawn_blocking(rust_yu_lib::browser_cleaner::scan_browser_data)
        .await
        .map_err(|error| CommandError::with_code("browser_scan_join_failed", error.to_string()))?
        .map_err(|error| CommandError::with_code("browser_scan_failed", error))
}

#[tauri::command]
pub async fn clean_browser_data(
    request: BrowserCleanupRequest,
) -> Result<BrowserCleanupResult, CommandError> {
    require_administrator()?;
    // 核心层会重新扫描并验证目标 ID，不信任前端传入的路径。
    tauri::async_runtime::spawn_blocking(move || {
        rust_yu_lib::browser_cleaner::clean_browser_data(&request)
    })
    .await
    .map_err(|error| CommandError::with_code("browser_clean_join_failed", error.to_string()))?
    .map_err(|error| CommandError::with_code("browser_clean_failed", error))
}
