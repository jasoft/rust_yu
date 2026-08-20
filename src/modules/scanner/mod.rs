pub mod appdata;
pub mod filesystem;
pub mod models;
pub mod registry;
pub mod scope;
pub mod shortcuts;

use crate::modules::common::error::UninstallerError;
use crate::modules::lister::models::InstalledProgram;
use models::{
    ConfidenceReason, ScanArea, ScanAreaStatus, ScanCoverage, ScanReport, Trace, TraceType,
};
use scope::ScanIdentity;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 扫描所有类型的痕迹
pub async fn scan_all_traces(
    program_name: &str,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    Ok(scan_all_traces_internal(program_name, None, trace_types)
        .await?
        .traces)
}

pub async fn scan_all_traces_report(
    program_name: &str,
    trace_types: Option<Vec<TraceType>>,
) -> Result<ScanReport, UninstallerError> {
    scan_all_traces_internal(program_name, None, trace_types).await
}

/// 扫描带有安装目录上下文的程序残留，包括服务、计划任务和驱动证据。
pub async fn scan_all_traces_for_program(
    program: &InstalledProgram,
    trace_types: Option<Vec<TraceType>>,
) -> Result<Vec<Trace>, UninstallerError> {
    Ok(
        scan_all_traces_internal(&program.name, Some(program.clone()), trace_types)
            .await?
            .traces,
    )
}

pub async fn scan_all_traces_report_for_program(
    program: &InstalledProgram,
    trace_types: Option<Vec<TraceType>>,
) -> Result<ScanReport, UninstallerError> {
    scan_all_traces_internal(&program.name, Some(program.clone()), trace_types).await
}

async fn scan_all_traces_internal(
    program_name: &str,
    program: Option<InstalledProgram>,
    trace_types: Option<Vec<TraceType>>,
) -> Result<ScanReport, UninstallerError> {
    let types = trace_types.unwrap_or_else(|| {
        vec![
            TraceType::RegistryKey,
            TraceType::RegistryValue,
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
    let coverage = Arc::new(Mutex::new(Vec::<ScanCoverage>::new()));

    let mut handles = vec![];

    // 并行扫描不同类型
    if types.contains(&TraceType::RegistryKey) || types.contains(&TraceType::RegistryValue) {
        let registry_identity = identity.clone();
        let registry_program = program.clone();
        let t = traces.clone();
        let c = coverage.clone();
        handles.push(tokio::spawn(async move {
            let scan_result = if let Some(program) = registry_program.as_ref() {
                registry::scan_registry_traces_report_for_program(program)
                    .map(|report| (report.traces, report.truncated))
            } else {
                registry::scan_registry_traces_for_identity(&registry_identity)
                    .map(|traces| (traces, false))
            };
            match scan_result {
                Ok((mut traces, truncated)) => {
                    {
                        let mut guard = t.lock().await;
                        guard.append(&mut traces);
                    }
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::Registry,
                        if truncated {
                            ScanAreaStatus::Partial
                        } else {
                            ScanAreaStatus::Complete
                        },
                    ));
                }
                Err(e) => {
                    tracing::warn!("注册表扫描失败: {}", e);
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::Registry,
                        ScanAreaStatus::Failed,
                    ));
                }
            }
        }));
    }

    if types.contains(&TraceType::File) {
        let filesystem_identity = identity.clone();
        let filesystem_program = program.clone();
        let t = traces.clone();
        let c = coverage.clone();
        handles.push(tokio::spawn(async move {
            let scan_result = filesystem_program.as_ref().map_or_else(
                || filesystem::scan_filesystem_traces_with_identity(&filesystem_identity),
                filesystem::scan_filesystem_traces_for_program,
            );
            match scan_result {
                Ok(mut traces) => {
                    {
                        let mut guard = t.lock().await;
                        guard.append(&mut traces);
                    }
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::InstallFiles,
                        ScanAreaStatus::Complete,
                    ));
                }
                Err(e) => {
                    tracing::warn!("文件系统扫描失败: {}", e);
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::InstallFiles,
                        ScanAreaStatus::Failed,
                    ));
                }
            }
        }));
    }

    if types.contains(&TraceType::AppData) {
        let appdata_identity = identity.clone();
        let appdata_program = program.clone();
        let t = traces.clone();
        let c = coverage.clone();
        handles.push(tokio::spawn(async move {
            let scan_result = appdata_program.as_ref().map_or_else(
                || appdata::scan_appdata_traces_with_identity(&appdata_identity),
                appdata::scan_appdata_traces_for_program,
            );
            match scan_result {
                Ok(mut traces) => {
                    {
                        let mut guard = t.lock().await;
                        guard.append(&mut traces);
                    }
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::UserData,
                        ScanAreaStatus::Complete,
                    ));
                }
                Err(e) => {
                    tracing::warn!("AppData扫描失败: {}", e);
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::UserData,
                        ScanAreaStatus::Failed,
                    ));
                }
            }
        }));
    }

    if types.contains(&TraceType::Shortcut) {
        let shortcut_identity = identity.clone();
        let t = traces.clone();
        let c = coverage.clone();
        handles.push(tokio::spawn(async move {
            match shortcuts::scan_shortcut_traces_with_identity(&shortcut_identity) {
                Ok(mut traces) => {
                    {
                        let mut guard = t.lock().await;
                        guard.append(&mut traces);
                    }
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::Shortcuts,
                        ScanAreaStatus::Complete,
                    ));
                }
                Err(e) => {
                    tracing::warn!("快捷方式扫描失败: {}", e);
                    c.lock().await.push(ScanCoverage::new(
                        ScanArea::Shortcuts,
                        ScanAreaStatus::Failed,
                    ));
                }
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
            let c = coverage.clone();
            handles.push(tokio::task::spawn_blocking(move || {
                let report = crate::modules::system_integration::scan_traces_report(&program);
                let mut integration_traces = report.traces;
                integration_traces.retain(|trace| requested_types.contains(&trace.trace_type));
                {
                    let mut guard = t.blocking_lock();
                    guard.append(&mut integration_traces);
                }
                let mut coverage = c.blocking_lock();
                for trace_type in requested_types.iter().copied().filter(|trace_type| {
                    matches!(
                        trace_type,
                        TraceType::ScheduledTask | TraceType::Service | TraceType::Driver
                    )
                }) {
                    let status = if report.failed_types.contains(&trace_type) {
                        ScanAreaStatus::Failed
                    } else if report.skipped_types.contains(&trace_type) {
                        ScanAreaStatus::Skipped
                    } else {
                        ScanAreaStatus::Complete
                    };
                    coverage.push(ScanCoverage::new(system_scan_area(trace_type), status));
                }
            }));
        } else {
            tracing::debug!("没有安装目录上下文，跳过服务、计划任务和驱动残留扫描");
            let mut guard = coverage.lock().await;
            for trace_type in types.iter().copied().filter(|trace_type| {
                matches!(
                    trace_type,
                    TraceType::ScheduledTask | TraceType::Service | TraceType::Driver
                )
            }) {
                guard.push(ScanCoverage::new(
                    system_scan_area(trace_type),
                    ScanAreaStatus::Skipped,
                ));
            }
        }
    }

    // 等待所有任务完成
    for handle in handles {
        let _ = handle.await;
    }

    // 获取所有结果
    let mut result = traces.lock().await.clone();

    // 注册表扫描器一次读取同一卸载键及其值；对外仍严格遵守调用方请求的类型。
    result.retain(|trace| types.contains(&trace.trace_type));

    // 先补齐旧来源的评分，再按分数排序；同一路径同时被名称扫描和明确安装目录
    // 命中时，必须保留证据更强的一条，不能让并发/遍历顺序把 95 分降成 60 分。
    assign_confidence_scores(&identity, &mut result);
    deduplicate_traces_by_score(&mut result);

    // 按置信度排序
    result.sort_by(|a, b| {
        b.confidence_score
            .cmp(&a.confidence_score)
            .then_with(|| b.confidence.cmp(&a.confidence))
    });

    // 过滤已存在的痕迹
    result.retain(|t| t.exists);

    let mut coverage = coverage.lock().await.clone();
    for entry in &mut coverage {
        entry.item_count = result
            .iter()
            .filter(|trace| trace_belongs_to_area(trace, entry.area))
            .count();
        if entry.status == ScanAreaStatus::Complete
            && matches!(entry.area, ScanArea::InstallFiles | ScanArea::UserData)
            && entry.item_count >= 10_000
        {
            entry.status = ScanAreaStatus::Partial;
        }
    }
    coverage.sort_by_key(|entry| scan_area_order(entry.area));

    Ok(ScanReport {
        traces: result,
        coverage,
    })
}

fn deduplicate_traces_by_score(traces: &mut Vec<Trace>) {
    traces.sort_by_key(|trace| std::cmp::Reverse(trace.confidence_score));
    let mut seen = HashSet::new();
    traces.retain(|trace| {
        let normalized_path = trace.path.replace('/', "\\").to_lowercase();
        seen.insert(format!("{}|{normalized_path}", trace.trace_type))
    });
}

fn system_scan_area(trace_type: TraceType) -> ScanArea {
    match trace_type {
        TraceType::ScheduledTask => ScanArea::ScheduledTasks,
        TraceType::Service => ScanArea::Services,
        TraceType::Driver => ScanArea::Drivers,
        _ => ScanArea::InstallFiles,
    }
}

fn trace_belongs_to_area(trace: &Trace, area: ScanArea) -> bool {
    match area {
        ScanArea::Registry => matches!(
            trace.trace_type,
            TraceType::RegistryKey | TraceType::RegistryValue
        ),
        ScanArea::InstallFiles => trace.trace_type == TraceType::File,
        ScanArea::UserData => trace.trace_type == TraceType::AppData,
        ScanArea::Shortcuts => trace.trace_type == TraceType::Shortcut,
        ScanArea::ScheduledTasks => trace.trace_type == TraceType::ScheduledTask,
        ScanArea::Services => trace.trace_type == TraceType::Service,
        ScanArea::Drivers => trace.trace_type == TraceType::Driver,
    }
}

fn scan_area_order(area: ScanArea) -> u8 {
    match area {
        ScanArea::Registry => 0,
        ScanArea::InstallFiles => 1,
        ScanArea::UserData => 2,
        ScanArea::Shortcuts => 3,
        ScanArea::ScheduledTasks => 4,
        ScanArea::Services => 5,
        ScanArea::Drivers => 6,
    }
}

/// 分配置信度分数
fn assign_confidence_scores(identity: &ScanIdentity, traces: &mut [Trace]) {
    for trace in traces.iter_mut() {
        if trace.confidence_score == 0
            && matches!(
                trace.trace_type,
                TraceType::ScheduledTask | TraceType::Service | TraceType::Driver
            )
        {
            // 系统集成项目只有在扫描器拿到明确关联路径时才会进入列表，
            // 因此保留高置信度；名称本身不能提升服务/任务的关联等级。
            let (score, reason) = if trace.related_path.is_some() {
                (95, ConfidenceReason::SystemCommandReferencesInstallLocation)
            } else {
                (25, ConfidenceReason::Unknown)
            };
            trace.confidence_score = score;
            trace.confidence = models::confidence_for_score(score);
            trace.confidence_reason = reason;
        } else if trace.confidence_score == 0 {
            // 扫描器已经根据受限根目录和组件级匹配设置了证据等级。
            // 这里仅把仍能定位到精确组件的候选提升到中置信度，绝不再
            // 使用 path.contains(program_name) 这种跨边界启发式。
            let matches_component = trace
                .path
                .replace('/', "\\")
                .split('\\')
                .any(|component| identity.matches_component(component));
            let (score, reason) = if matches_component {
                (
                    60,
                    match trace.trace_type {
                        TraceType::RegistryKey | TraceType::RegistryValue => {
                            ConfidenceReason::ExactNameRegistry
                        }
                        TraceType::AppData => ConfidenceReason::ExactNameUserData,
                        TraceType::Shortcut => ConfidenceReason::ExactShortcutName,
                        _ => ConfidenceReason::ExactNameStandardLocation,
                    },
                )
            } else {
                (25, ConfidenceReason::Unknown)
            };
            trace.confidence_score = score;
            trace.confidence = models::confidence_for_score(score);
            trace.confidence_reason = reason;
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
        ) && crate::modules::common::utils::is_critical_registry_path(&trace.path)
        {
            trace.is_critical = true;
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

    #[test]
    fn duplicate_paths_keep_the_strongest_evidence() {
        let mut traces = vec![
            trace(TraceType::File, r"C:\Demo App\app.exe")
                .with_confidence_evidence(60, ConfidenceReason::ExactNameStandardLocation),
            trace(TraceType::File, r"c:\demo app\app.exe")
                .with_confidence_evidence(95, ConfidenceReason::ExplicitInstallLocation),
        ];

        deduplicate_traces_by_score(&mut traces);

        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].confidence_score, 95);
        assert_eq!(
            traces[0].confidence_reason,
            ConfidenceReason::ExplicitInstallLocation
        );
    }
}
