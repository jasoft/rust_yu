use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::scanner::models::Trace;
use super::models::CleanResult;

/// 删除文件痕迹
pub async fn delete_file_trace(trace: &Trace) -> Result<CleanResult, UninstallerError> {
    let path = std::path::PathBuf::from(&trace.path);

    // 检查路径是否存在
    if !path.exists() {
        return Ok(CleanResult {
            trace_id: trace.id.clone(),
            path: trace.path.clone(),
            success: true, // 目标已不存在，视为成功
            error: None,
            bytes_freed: 0,
        });
    }

    // 计算并删除
    let bytes_freed: u64;
    let result = if path.is_dir() {
        // 目录：计算大小后删除
        bytes_freed = utils::calculate_dir_size(&path).unwrap_or(0);
        std::fs::remove_dir_all(&path)
    } else {
        // 文件：计算大小后删除
        bytes_freed = path.metadata()?.len();
        std::fs::remove_file(&path)
    };

    match result {
        Ok(_) => {
            tracing::info!("已删除: {}", trace.path);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: true,
                error: None,
                bytes_freed,
            })
        }
        Err(e) => {
            tracing::error!("删除失败 {}: {}", trace.path, e);

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
