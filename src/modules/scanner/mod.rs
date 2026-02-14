pub mod models;
pub mod registry;
pub mod filesystem;
pub mod appdata;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use models::{Trace, TraceType};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 扫描所有类型的痕迹
pub async fn scan_all_traces(
    program_name: &str,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    let types = trace_types.unwrap_or_else(|| vec![
        TraceType::RegistryKey,
        TraceType::File,
        TraceType::AppData,
        TraceType::Shortcut,
    ]);

    let _all_traces: Vec<Trace> = Vec::new();
    let program_name = program_name.to_string();

    // 使用 Arc 和 Mutex 来收集结果
    let traces = Arc::new(Mutex::new(Vec::<Trace>::new()));

    let mut handles = vec![];

    // 并行扫描不同类型
    if types.contains(&TraceType::RegistryKey) {
        let name = program_name.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match registry::scan_registry_traces(&name) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("注册表扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::File) {
        let name = program_name.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match filesystem::scan_filesystem_traces(&name) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("文件系统扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::AppData) {
        let name = program_name.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match appdata::scan_appdata_traces(&name) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("AppData扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::Shortcut) {
        let name = program_name.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match shortcuts::scan_shortcut_traces(&name) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("快捷方式扫描失败: {}", e),
            }
        }));
    }

    // 等待所有任务完成
    for handle in handles {
        let _ = handle.await;
    }

    // 获取所有结果
    let mut result = traces.lock().await.clone();

    // 计算置信度
    assign_confidence_scores(&program_name, &mut result);

    // 按置信度排序
    result.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    // 过滤已存在的痕迹
    result.retain(|t| t.exists);

    Ok(result)
}

/// 分配置信度分数
fn assign_confidence_scores(program_name: &str, traces: &mut Vec<Trace>) {
    let name_lower = program_name.to_lowercase();

    for trace in traces.iter_mut() {
        let path_lower = trace.path.to_lowercase();

        // 检查是否包含程序名
        let name_match = path_lower.contains(&name_lower);

        // 检查是否完全匹配
        let exact_match = path_lower.contains(&format!("\\{} ", name_lower))
            || path_lower.contains(&format!("/{} ", name_lower))
            || path_lower.contains(&format!("\\{}.", name_lower));

        trace.confidence = if exact_match {
            models::Confidence::High
        } else if name_match {
            models::Confidence::Medium
        } else {
            models::Confidence::Low
        };

        // 检查是否为关键系统项
        if crate::modules::common::utils::is_system_critical_path(&trace.path) {
            trace.is_critical = true;
        }

        if matches!(trace.trace_type, TraceType::RegistryKey | TraceType::RegistryValue) {
            if crate::modules::common::utils::is_critical_registry_path(&trace.path) {
                trace.is_critical = true;
            }
        }
    }
}
