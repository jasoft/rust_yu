pub mod filesystem;
pub mod models;
pub mod registry;
pub mod safety;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::{Trace, TraceType};
use models::CleanResult;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupAuthorization {
    SafeDefaults,
    UserReviewed,
}

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
    clean_traces_with_authorization(traces, confirm, CleanupAuthorization::SafeDefaults).await
}

/// 清理由卸载结果页逐项展示并由用户明确确认的目标。
///
/// 中、低置信度只表示归属证据较弱，不等于目标禁止删除；这里保留关键系统项、
/// 删除前重校验和备份门禁，但不再把置信度当作硬拒绝条件。
pub async fn clean_reviewed_traces(
    traces: Vec<Trace>,
    confirm: bool,
) -> Result<Vec<CleanResult>, UninstallerError> {
    clean_traces_with_authorization(traces, confirm, CleanupAuthorization::UserReviewed).await
}

async fn clean_traces_with_authorization(
    traces: Vec<Trace>,
    confirm: bool,
    authorization: CleanupAuthorization,
) -> Result<Vec<CleanResult>, UninstallerError> {
    if !confirm {
        return Err(UninstallerError::PermissionDenied(
            "需要确认才能执行清理".to_string(),
        ));
    }
    if authorization == CleanupAuthorization::SafeDefaults {
        crate::modules::advanced::validate_cleanup_selection(
            crate::modules::advanced::CleanupPolicyKind::Safe,
            &traces,
            confirm,
        )?;
    } else if traces.is_empty() {
        return Err(UninstallerError::Other("没有选择清理目标。".to_string()));
    }

    let backup_reason = backup_reason(&traces);
    let backup_traces = traces.clone();
    let backup_result = tokio::task::spawn_blocking(move || {
        crate::modules::backup::prepare_for_traces(&backup_traces, &backup_reason)
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
        // 共享"应用值容器"（Run/App Paths/Environment/FirewallRules）里的应用
        // 私有值允许值级删除（键本体保留）：这类值先经 safety 的归属重校验，
        // 证据不足时仍然拒绝，防止共享键中他人值被误删。
        let shared_container_value = safety::is_shared_application_value_container(&trace);

        // 扫描器之外的旧快照、导入报告或手工调用也必须经过同一层
        // 共享区域保护，避免历史错误候选绕过新的扫描范围规则。
        if !shared_container_value
            && crate::modules::scanner::scope::is_protected_shared_path(Path::new(&trace.path))
        {
            tracing::warn!(path = %trace.path, "跳过受保护共享区域中的残留候选");
            results.push(failed_result(
                &trace,
                "目标位于受保护的系统或共享区域，已跳过清理".to_string(),
                None,
            ));
            continue;
        }

        // 安全检查：共享容器值走归属重校验，其余目标走原有临界项检查
        let safety_outcome = if shared_container_value {
            safety::revalidate_shared_value_ownership(&trace)
        } else {
            safety::pre_delete_check(&trace)
        };
        if let Err(e) = safety_outcome {
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

/// 备份会话标题必须来自本次真实扫描结果，不能让所有卸载历史都显示成同一个
/// 泛化名称。一个清理批次正常只属于一个程序；若调用方确实混合了多个程序，
/// 明确标记为批量任务，避免把备份错误归到第一个程序名下。
fn backup_reason(traces: &[Trace]) -> String {
    let mut program_names = traces
        .iter()
        .map(|trace| trace.program_name.trim())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    program_names.sort_unstable();
    program_names.dedup();
    match program_names.as_slice() {
        [program_name] => format!("{program_name} · 卸载残留清理"),
        [] => "卸载残留清理".to_string(),
        _ => format!("批量卸载残留清理 · {} 个程序", program_names.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::{backup_reason, clean_reviewed_traces, clean_traces};
    use crate::modules::backup::restore_session;
    use crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;

    #[test]
    fn backup_reason_uses_the_real_program_name() {
        let traces = vec![Trace::new(
            "Rust Yu Legacy Test App".to_string(),
            TraceType::File,
            r"C:\Program Files\RustYu Legacy Test App\leftover.log".to_string(),
        )];

        assert_eq!(
            backup_reason(&traces),
            "Rust Yu Legacy Test App · 卸载残留清理"
        );
    }

    #[test]
    fn backup_reason_does_not_mislabel_mixed_programs() {
        let traces = vec![
            Trace::new(
                "App A".to_string(),
                TraceType::File,
                r"C:\Program Files\App A\leftover.log".to_string(),
            ),
            Trace::new(
                "App B".to_string(),
                TraceType::RegistryKey,
                r"HKCU\Software\App B".to_string(),
            ),
        ];

        assert_eq!(backup_reason(&traces), "批量卸载残留清理 · 2 个程序");
    }
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

    #[tokio::test]
    async fn reviewed_cleanup_allows_confirmed_medium_confidence_target() {
        let _guard = TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let storage_root = std::env::temp_dir().join(format!(
            "rust-yu-reviewed-cleaner-test-{}",
            uuid::Uuid::new_v4()
        ));
        let target = storage_root.join("medium-confidence-leftover.txt");
        fs::create_dir_all(&storage_root)
            .unwrap_or_else(|error| panic!("create fixture directory: {error}"));
        fs::write(&target, b"reviewed leftover")
            .unwrap_or_else(|error| panic!("write fixture: {error}"));

        let previous = std::env::var_os("RUST_YU_STORAGE_DIR");
        std::env::set_var("RUST_YU_STORAGE_DIR", storage_root.join("storage"));
        let trace = Trace::new(
            "Demo App".to_string(),
            TraceType::File,
            target.to_string_lossy().into_owned(),
        )
        .with_confidence(crate::modules::scanner::models::Confidence::Medium);

        assert!(clean_traces(vec![trace.clone()], true).await.is_err());
        let results = clean_reviewed_traces(vec![trace], true)
            .await
            .unwrap_or_else(|error| panic!("reviewed cleanup should be allowed: {error}"));
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(!target.exists());

        match previous {
            Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
            None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
        }
        let _ = fs::remove_dir_all(storage_root);
    }

    #[tokio::test]
    async fn reviewed_cleanup_rejects_protected_shared_area() {
        let _guard = TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!(
            "rust-yu-protected-cleaner-test-{}",
            uuid::Uuid::new_v4()
        ));
        let target = root
            .join("Microsoft")
            .join("Internet Explorer")
            .join("Quick Launch")
            .join("Google Chrome.lnk");
        fs::create_dir_all(target.parent().unwrap_or(&root))
            .unwrap_or_else(|error| panic!("create protected fixture: {error}"));
        fs::write(&target, b"shared shell data")
            .unwrap_or_else(|error| panic!("write protected fixture: {error}"));

        let trace = Trace::new(
            "Xplorer".to_string(),
            TraceType::Shortcut,
            target.to_string_lossy().into_owned(),
        )
        .with_confidence(crate::modules::scanner::models::Confidence::High);
        let results = clean_reviewed_traces(vec![trace], true)
            .await
            .unwrap_or_else(|error| {
                panic!("protected cleanup should return a review result: {error}")
            });

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(target.exists());
        let _ = fs::remove_dir_all(root);
    }
}
