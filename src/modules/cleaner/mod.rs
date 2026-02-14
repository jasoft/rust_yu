pub mod models;
pub mod safety;
pub mod filesystem;
pub mod registry;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::{Trace, TraceType};
use models::CleanResult;

/// 清理痕迹
pub async fn clean_traces(
    traces: Vec<Trace>,
    confirm: bool,
) -> Result<Vec<CleanResult>, UninstallerError> {
    if !confirm {
        return Err(UninstallerError::PermissionDenied(
            "需要确认才能执行清理".to_string(),
        ));
    }

    let mut results = Vec::new();

    for trace in traces {
        // 安全检查
        if let Err(e) = safety::pre_delete_check(&trace) {
            tracing::warn!("跳过关键系统项: {}", e);
            results.push(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: false,
                error: Some(format!("跳过关键系统项: {}", e)),
                bytes_freed: 0,
            });
            continue;
        }

        let result = match trace.trace_type {
            TraceType::RegistryKey => {
                registry::delete_registry_trace(&trace).await
            }
            TraceType::RegistryValue => {
                registry::delete_registry_trace(&trace).await
            }
            TraceType::File | TraceType::AppData => {
                filesystem::delete_file_trace(&trace).await
            }
            TraceType::Shortcut => {
                shortcuts::delete_shortcut_trace(&trace).await
            }
            _ => {
                results.push(CleanResult {
                    trace_id: trace.id.clone(),
                    path: trace.path.clone(),
                    success: false,
                    error: Some("不支持的痕迹类型".to_string()),
                    bytes_freed: 0,
                });
                continue;
            }
        };

        match result {
            Ok(r) => results.push(r),
            Err(e) => {
                results.push(CleanResult {
                    trace_id: trace.id.clone(),
                    path: trace.path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    bytes_freed: 0,
                });
            }
        }
    }

    Ok(results)
}
