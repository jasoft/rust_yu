pub mod filesystem;
pub mod models;
pub mod registry;
pub mod safety;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::{Trace, TraceType};
use models::CleanResult;
use std::sync::Arc;

fn failed_result(trace: &Trace, error: String, backup_id: Option<String>) -> CleanResult {
    CleanResult {
        trace_id: trace.id.clone(),
        path: trace.path.clone(),
        success: false,
        error: Some(error),
        bytes_freed: 0,
        backup_id,
    }
}

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
    crate::modules::advanced::validate_cleanup_selection(
        crate::modules::advanced::CleanupPolicyKind::Safe,
        &traces,
        confirm,
    )?;

    let backup_traces = traces.clone();
    let backup_result = tokio::task::spawn_blocking(move || {
        crate::modules::backup::prepare_for_traces(&backup_traces, "卸载残留清理")
    })
    .await;
    let (backup_preparation, backup_error) = match backup_result {
        Ok(Ok(preparation)) => (preparation, None),
        Ok(Err(error)) => {
            tracing::error!("生成清理前备份失败: {error}");
            (None, Some(format!("生成清理前备份失败: {error}")))
        }
        Err(error) => {
            tracing::error!("清理前备份任务失败: {error}");
            (None, Some(format!("清理前备份任务失败: {error}")))
        }
    };
    let backup_preparation = backup_preparation.map(Arc::new);
    let mut results = Vec::new();

    for trace in traces {
        // 安全检查
        if let Err(e) = safety::pre_delete_check(&trace) {
            tracing::warn!("跳过关键系统项: {}", e);
            results.push(failed_result(
                &trace,
                format!("跳过关键系统项: {}", e),
                None,
            ));
            continue;
        }

        let backup_id = if crate::modules::backup::requires_backup(&trace) {
            let Some(preparation) = backup_preparation.as_ref() else {
                results.push(failed_result(
                    &trace,
                    backup_error
                        .clone()
                        .unwrap_or_else(|| "没有生成清理前备份会话".to_string()),
                    None,
                ));
                continue;
            };
            if let Err(error) = preparation.validate_trace(&trace) {
                results.push(failed_result(
                    &trace,
                    format!("删除前备份校验失败，已跳过清理: {error}"),
                    Some(preparation.session_id().to_string()),
                ));
                continue;
            }
            Some(preparation.session_id().to_string())
        } else {
            None
        };

        if let Some(error) = backup_error.as_ref() {
            if crate::modules::backup::requires_backup(&trace) {
                results.push(failed_result(&trace, error.clone(), backup_id));
                continue;
            }
        }

        let result = match trace.trace_type {
            TraceType::RegistryKey => registry::delete_registry_trace(&trace).await,
            TraceType::RegistryValue => registry::delete_registry_trace(&trace).await,
            TraceType::File | TraceType::AppData => filesystem::delete_file_trace(&trace).await,
            TraceType::Shortcut => shortcuts::delete_shortcut_trace(&trace).await,
            TraceType::ScheduledTask | TraceType::Service => {
                let candidate = trace.clone();
                match tokio::task::spawn_blocking(move || {
                    crate::modules::system_integration::remove_trace(&candidate)
                })
                .await
                {
                    Ok(result) => result.map(|()| CleanResult {
                        trace_id: trace.id.clone(),
                        path: trace.path.clone(),
                        success: true,
                        error: None,
                        bytes_freed: 0,
                        backup_id: None,
                    }),
                    Err(error) => Err(UninstallerError::Other(format!(
                        "系统集成清理任务失败: {error}"
                    ))),
                }
            }
            _ => {
                results.push(failed_result(
                    &trace,
                    "不支持的痕迹类型".to_string(),
                    backup_id,
                ));
                continue;
            }
        };

        match result {
            Ok(mut result) => {
                if crate::modules::backup::requires_backup(&trace) {
                    match crate::modules::backup::verify_trace_removed(&trace) {
                        Ok(true) => {}
                        Ok(false) => {
                            result.success = false;
                            result.error = Some("删除命令返回成功，但目标仍然存在。".to_string());
                            result.bytes_freed = 0;
                        }
                        Err(error) => {
                            result.success = false;
                            result.error = Some(format!("删除后校验失败：{error}"));
                            result.bytes_freed = 0;
                        }
                    }
                }
                if let Some(session_id) = backup_id.as_deref() {
                    result.backup_id = Some(session_id.to_string());
                    if let Err(error) = crate::modules::backup::record_cleanup_result(
                        session_id,
                        &trace.id,
                        result.success,
                        result.error.clone(),
                    ) {
                        result.success = false;
                        result.error =
                            Some(format!("清理结果已执行，但备份会话状态写入失败: {error}"));
                    }
                }
                results.push(result);
            }
            Err(e) => {
                let mut result = failed_result(&trace, e.to_string(), backup_id.clone());
                if let Some(session_id) = backup_id.as_deref() {
                    if let Err(error) = crate::modules::backup::record_cleanup_result(
                        session_id,
                        &trace.id,
                        false,
                        result.error.clone(),
                    ) {
                        result.error = Some(format!(
                            "{}；备份会话状态写入失败: {error}",
                            result.error.unwrap_or_default()
                        ));
                    }
                }
                results.push(result);
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::clean_traces;
    use crate::modules::backup::restore_session;
    use crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;
    use crate::modules::scanner::models::{Trace, TraceType};
    use std::fs;

    #[tokio::test]
    async fn clean_file_records_backup_and_restore_can_recover_it() {
        let _guard = TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = std::env::temp_dir().join(format!(
            "rust-yu-cleaner-backup-test-{}",
            uuid::Uuid::new_v4()
        ));
        let target = storage_root.join("fixture").join("leftover.txt");
        let target_parent = target.parent().unwrap_or(storage_root.as_path());
        fs::create_dir_all(target_parent)
            .unwrap_or_else(|error| panic!("create fixture parent: {error}"));
        fs::write(&target, b"recoverable leftover")
            .unwrap_or_else(|error| panic!("write fixture: {error}"));

        let previous = std::env::var_os("RUST_YU_STORAGE_DIR");
        std::env::set_var("RUST_YU_STORAGE_DIR", storage_root.join("storage"));
        let trace = Trace::new(
            "Demo App".to_string(),
            TraceType::File,
            target.to_string_lossy().into_owned(),
        )
        .with_confidence(crate::modules::scanner::models::Confidence::High);

        let results = clean_traces(vec![trace], true)
            .await
            .unwrap_or_else(|error| panic!("clean fixture: {error}"));
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        let backup_id = results[0]
            .backup_id
            .as_deref()
            .unwrap_or_else(|| panic!("missing backup id"))
            .to_string();
        assert!(!target.exists());

        let restore =
            restore_session(&backup_id).unwrap_or_else(|error| panic!("restore fixture: {error}"));
        assert!(restore.success);
        assert_eq!(
            fs::read(&target).unwrap_or_default(),
            b"recoverable leftover"
        );

        match previous {
            Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
            None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
        }
        let _ = fs::remove_dir_all(storage_root);
    }
}
