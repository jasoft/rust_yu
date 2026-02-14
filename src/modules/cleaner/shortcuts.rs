use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::Trace;
use super::models::CleanResult;

/// 删除快捷方式
pub async fn delete_shortcut_trace(trace: &Trace) -> Result<CleanResult, UninstallerError> {
    let path = std::path::PathBuf::from(&trace.path);

    // 检查是否存在
    if !path.exists() {
        return Ok(CleanResult {
            trace_id: trace.id.clone(),
            path: trace.path.clone(),
            success: true,
            error: None,
            bytes_freed: 0,
        });
    }

    // 删除快捷方式文件
    match std::fs::remove_file(&path) {
        Ok(_) => {
            tracing::info!("已删除快捷方式: {}", trace.path);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: true,
                error: None,
                bytes_freed: 0,
            })
        }
        Err(e) => {
            tracing::error!("删除快捷方式失败 {}: {}", trace.path, e);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: false,
                error: Some(e.to_string()),
                bytes_freed: 0,
            })
        }
    }
}
