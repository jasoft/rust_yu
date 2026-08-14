pub mod appdata;
pub mod filesystem;
pub mod models;
pub mod registry;
pub mod scope;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use crate::modules::lister::models::InstalledProgram;
use models::{Trace, TraceType};
use scope::ScanIdentity;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 扫描所有类型的痕迹
pub async fn scan_all_traces(
    program_name: &str,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    scan_all_traces_internal(program_name, None, trace_types).await
}

/// 扫描带有安装目录上下文的程序残留，包括服务、计划任务和驱动证据。
pub async fn scan_all_traces_for_program(
    program: &InstalledProgram,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    scan_all_traces_internal(&program.name, Some(program.clone()), trace_types).await
}

async fn scan_all_traces_internal(
    program_name: &str,
    program: Option<InstalledProgram>,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    let types = trace_types.unwrap_or_else(|| {
        vec![
            TraceType::RegistryKey,
            TraceType::File,
            TraceType::AppData,
            TraceType::Shortcut,
            TraceType::ScheduledTask,
            TraceType::Service,
            TraceType::Driver,
        ]
    });

    let _all_traces: Vec<Trace> = Vec::new();
    let program_name = program_name.to_string();
    let identity = program
        .as_ref()
        .map(ScanIdentity::from_program)
        .unwrap_or_else(|| ScanIdentity::from_name(&program_name));

    // 使用 Arc 和 Mutex 来收集结果
    let traces = Arc::new(Mutex::new(Vec::<Trace>::new()));

    let mut handles = vec![];

    // 并行扫描不同类型
    if types.contains(&TraceType::RegistryKey) {
        let registry_identity = identity.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match registry::scan_registry_traces_for_identity(&registry_identity) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("注册表扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::File) {
        let filesystem_identity = identity.clone();
        let filesystem_program = program.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            let scan_result = filesystem_program.as_ref().map_or_else(
                || filesystem::scan_filesystem_traces_with_identity(&filesystem_identity),
                filesystem::scan_filesystem_traces_for_program,
            );
            match scan_result {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("文件系统扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::AppData) {
        let appdata_identity = identity.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match appdata::scan_appdata_traces_with_identity(&appdata_identity) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("AppData扫描失败: {}", e),
            }
        }));
    }

    if types.contains(&TraceType::Shortcut) {
        let shortcut_identity = identity.clone();
        let t = traces.clone();
        handles.push(tokio::spawn(async move {
            match shortcuts::scan_shortcut_traces_with_identity(&shortcut_identity) {
                Ok(mut traces) => {
                    let mut guard = t.lock().await;
                    guard.append(&mut traces);
                }
                Err(e) => tracing::warn!("快捷方式扫描失败: {}", e),
            }
        }));
    }

    if types.iter().any(|trace_type| {
        matches!(
            trace_type,
            TraceType::ScheduledTask | TraceType::Service | TraceType::Driver
        )
    }) {
        if let Some(program) = program {
            let requested_types = types.clone();
            let t = traces.clone();
            handles.push(tokio::task::spawn_blocking(move || {
                let mut integration_traces =
                    crate::modules::system_integration::scan_traces(&program);
                integration_traces.retain(|trace| requested_types.contains(&trace.trace_type));
                let mut guard = t.blocking_lock();
                guard.append(&mut integration_traces);
            }));
        } else {
            tracing::debug!("没有安装目录上下文，跳过服务、计划任务和驱动残留扫描");
        }
    }

    // 等待所有任务完成
    for handle in handles {
        let _ = handle.await;
    }

    // 获取所有结果
    let mut result = traces.lock().await.clone();

    // 并行扫描器可能从“名称命中”和“明确安装目录”同时发现同一文件。
    // 合并重复项，避免用户选中两个指向相同删除目标的条目。
    let mut seen = HashSet::new();
    result.retain(|trace| {
        let normalized_path = trace.path.replace('/', "\\").to_lowercase();
        seen.insert(format!("{}|{normalized_path}", trace.trace_type))
    });

    // 计算置信度
    assign_confidence_scores(&identity, &mut result);

    // 按置信度排序
    result.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    // 过滤已存在的痕迹
    result.retain(|t| t.exists);

    Ok(result)
}

/// 分配置信度分数
fn assign_confidence_scores(identity: &ScanIdentity, traces: &mut Vec<Trace>) {
    for trace in traces.iter_mut() {
        if matches!(
            trace.trace_type,
            TraceType::ScheduledTask | TraceType::Service | TraceType::Driver
        ) {
            // 系统集成项目只有在扫描器拿到明确关联路径时才会进入列表，
            // 因此保留高置信度；名称本身不能提升服务/任务的关联等级。
            trace.confidence = if trace.related_path.is_some() {
                models::Confidence::High
            } else {
                models::Confidence::Low
            };
        } else if trace.confidence != models::Confidence::High {
            // 扫描器已经根据受限根目录和组件级匹配设置了证据等级。
            // 这里仅把仍能定位到精确组件的候选提升到中置信度，绝不再
            // 使用 path.contains(program_name) 这种跨边界启发式。
            let matches_component = trace
                .path
                .replace('/', "\\")
                .split('\\')
                .any(|component| identity.matches_component(component));
            trace.confidence = if matches_component {
                models::Confidence::Medium
            } else {
                models::Confidence::Low
            };
        }

        // 检查是否为关键系统项
        if crate::modules::common::utils::is_system_critical_path(&trace.path) {
            trace.is_critical = true;
        }
        if trace
            .related_path
            .as_deref()
            .is_some_and(crate::modules::common::utils::is_system_critical_path)
        {
            trace.is_critical = true;
        }

        if matches!(
            trace.trace_type,
            TraceType::RegistryKey | TraceType::RegistryValue
        ) {
            if crate::modules::common::utils::is_critical_registry_path(&trace.path) {
                trace.is_critical = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(trace_type: TraceType, path: &str) -> Trace {
        Trace::new("Demo App".to_string(), trace_type, path.to_string())
    }

    #[test]
    fn assign_confidence_scores_distinguishes_exact_partial_and_non_matches() {
        let mut traces = vec![
            trace(TraceType::File, r"C:\Program Files\Demo App\demo app.exe"),
            trace(TraceType::File, r"C:\Temp\backup-demo app\log.txt"),
            trace(TraceType::File, r"C:\Temp\another-app\log.txt"),
        ];

        assign_confidence_scores(&ScanIdentity::from_name("Demo App"), &mut traces);

        assert_eq!(traces[0].confidence, models::Confidence::Medium);
        assert_eq!(traces[1].confidence, models::Confidence::Low);
        assert_eq!(traces[2].confidence, models::Confidence::Low);
    }

    #[test]
    fn assign_confidence_scores_marks_critical_file_and_registry_locations() {
        let mut traces = vec![
            trace(TraceType::File, r"C:\Windows\System32\demo.dll"),
            trace(
                TraceType::RegistryKey,
                r"HKLM\SYSTEM\CurrentControlSet\Services\DemoApp",
            ),
        ];

        assign_confidence_scores(&ScanIdentity::from_name("Demo App"), &mut traces);

        assert!(traces.iter().all(|trace| trace.is_critical));
    }
}
