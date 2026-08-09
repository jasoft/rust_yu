//! 本地软件健康检查。
//!
//! 这里故意不联网、不判断厂商是否真的发布了新版本。健康分数只由本机已读到的
//! 卸载元数据、自启动项、重复名称和历史使用信息组成；更新提示只提供注册表中
//! 已声明的厂商页面，交给用户手动确认，避免把营销内容伪装成安全结论。

use super::lister::enrichment::load_slow_app_info;
use super::lister::models::{InstalledProgram, MetadataConfidence, SlowAppInfo};
use super::startup::models::{StartupItem, StartupState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupImpact {
    None,
    Low,
    Medium,
    High,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFinding {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub severity: HealthSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHint {
    pub url: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramHealth {
    pub program_id: String,
    pub program_name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub score: u8,
    pub status: HealthStatus,
    pub findings: Vec<HealthFinding>,
    pub duplicate_count: usize,
    pub last_used: Option<String>,
    pub times_used: Option<u32>,
    pub startup_entry_count: usize,
    pub startup_impact: StartupImpact,
    pub update_hint: Option<UpdateHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub evaluated_at: DateTime<Utc>,
    pub programs: Vec<ProgramHealth>,
    pub total_programs: usize,
    pub review_count: usize,
    pub healthy_count: usize,
    pub warnings: Vec<String>,
}

/// 读取本机程序、自启动和旧版使用缓存，生成可解释的本地健康报告。
pub fn analyze_programs(
    programs: &[InstalledProgram],
    startup_items: &[StartupItem],
    usage_by_program: &HashMap<String, SlowAppInfo>,
    warnings: Vec<String>,
) -> HealthReport {
    let duplicate_counts = programs.iter().fold(HashMap::new(), |mut counts, program| {
        let key = normalize_key(&program.name);
        if !key.is_empty() {
            *counts.entry(key).or_insert(0usize) += 1;
        }
        counts
    });
    let evaluated_at = Utc::now();
    let health = programs
        .iter()
        .map(|program| {
            let duplicate_count = duplicate_counts
                .get(&normalize_key(&program.name))
                .copied()
                .unwrap_or(1);
            let usage = usage_by_program
                .get(&program.id)
                .cloned()
                .unwrap_or_default();
            evaluate_program(program, duplicate_count, startup_items, &usage)
        })
        .collect::<Vec<_>>();
    let review_count = health
        .iter()
        .filter(|program| program.status == HealthStatus::Review)
        .count();

    HealthReport {
        evaluated_at,
        total_programs: health.len(),
        healthy_count: health.len().saturating_sub(review_count),
        review_count,
        programs: health,
        warnings,
    }
}

/// 在后台任务中加载历史使用信息；没有旧缓存的平台会得到空结果，不影响主报告。
pub fn load_usage_snapshots(programs: &[InstalledProgram]) -> HashMap<String, SlowAppInfo> {
    programs
        .iter()
        .map(|program| (program.id.clone(), load_slow_app_info(program)))
        .collect()
}

fn evaluate_program(
    program: &InstalledProgram,
    duplicate_count: usize,
    startup_items: &[StartupItem],
    usage: &SlowAppInfo,
) -> ProgramHealth {
    let mut score = 100i16;
    let mut findings = Vec::new();

    if program.install_source != super::lister::models::InstallSource::Store
        && program.preferred_uninstall_string().is_none()
    {
        add_finding(
            &mut score,
            &mut findings,
            "missing_uninstaller",
            "缺少原厂卸载命令",
            "当前清单没有可直接调用的卸载命令；如需处理，应先使用强制卸载预览。",
            35,
            HealthSeverity::Critical,
        );
    }

    let location_missing = program.install_source != super::lister::models::InstallSource::Store
        && program
            .install_location
            .as_deref()
            .map(|path| !Path::new(path).is_dir())
            .unwrap_or(true);
    if location_missing {
        add_finding(
            &mut score,
            &mut findings,
            "missing_install_location",
            "安装位置不可确认",
            "安装目录为空或当前不可读取，残留范围和大小统计可能不完整。",
            20,
            HealthSeverity::Warning,
        );
    }

    if duplicate_count > 1 {
        add_finding(
            &mut score,
            &mut findings,
            "duplicate_entry",
            "存在同名清单项",
            &format!(
                "发现 {duplicate_count} 个名称相同或高度相似的清单项，卸载前应核对来源和版本。"
            ),
            20,
            HealthSeverity::Warning,
        );
    }

    if matches!(
        program.metadata_confidence,
        MetadataConfidence::Low | MetadataConfidence::Unknown
    ) {
        add_finding(
            &mut score,
            &mut findings,
            "metadata_incomplete",
            "元数据置信度较低",
            "安装日期、图标或大小等字段缺少可靠来源；健康分数不会据此推断软件有问题。",
            10,
            HealthSeverity::Info,
        );
    }

    if program.install_date.is_none() {
        add_finding(
            &mut score,
            &mut findings,
            "install_date_unknown",
            "安装日期未知",
            "本机清单没有可验证的安装日期。",
            5,
            HealthSeverity::Info,
        );
    }

    if program.size.is_none() && program.estimated_size.is_none() {
        add_finding(
            &mut score,
            &mut findings,
            "size_unknown",
            "占用空间未知",
            "尚未获得可靠的安装目录大小；可以回到应用列表刷新元数据。",
            5,
            HealthSeverity::Info,
        );
    }

    let matched_startup = startup_items
        .iter()
        .filter(|item| startup_item_matches_program(item, program))
        .collect::<Vec<_>>();
    let enabled_startup_count = matched_startup
        .iter()
        .filter(|item| item.state == StartupState::Enabled)
        .count();
    let startup_impact = match enabled_startup_count {
        0 => StartupImpact::None,
        1 => StartupImpact::Low,
        2 => StartupImpact::Medium,
        _ => StartupImpact::High,
    };
    if enabled_startup_count > 0 {
        add_finding(
            &mut score,
            &mut findings,
            "startup_impact",
            "存在启用的自启动项",
            &format!("匹配到 {enabled_startup_count} 个启用的自启动项；这只是启动影响提示，不代表项目应被禁用。"),
            (enabled_startup_count.saturating_mul(5).min(15)) as i16,
            HealthSeverity::Info,
        );
    }

    let score = score.clamp(0, 100) as u8;
    let status = if findings.iter().any(|finding| {
        matches!(
            finding.severity,
            HealthSeverity::Critical | HealthSeverity::Warning
        )
    }) {
        HealthStatus::Review
    } else {
        HealthStatus::Healthy
    };

    ProgramHealth {
        program_id: program.id.clone(),
        program_name: program.name.clone(),
        publisher: program.publisher.clone(),
        version: program.display_version.clone().or(program.version.clone()),
        score,
        status,
        findings,
        duplicate_count,
        last_used: usage.last_used.clone(),
        times_used: usage.times_used,
        startup_entry_count: matched_startup.len(),
        startup_impact,
        update_hint: build_update_hint(program),
    }
}

fn add_finding(
    score: &mut i16,
    findings: &mut Vec<HealthFinding>,
    code: &str,
    title: &str,
    detail: &str,
    penalty: i16,
    severity: HealthSeverity,
) {
    *score = score.saturating_sub(penalty);
    findings.push(HealthFinding {
        code: code.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        severity,
    });
}

fn startup_item_matches_program(item: &StartupItem, program: &InstalledProgram) -> bool {
    let install_location = program
        .install_location
        .as_deref()
        .map(normalize_path_key)
        .filter(|path| !path.is_empty());
    if let Some(location) = install_location.as_deref() {
        for candidate in [
            item.executable_path.as_deref(),
            item.command.as_deref(),
            item.working_dir.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if normalize_path_key(candidate).contains(location) {
                return true;
            }
        }
    }

    let program_key = normalize_key(&program.name);
    !program_key.is_empty() && program_key.len() >= 3 && normalize_key(&item.name) == program_key
}

fn build_update_hint(program: &InstalledProgram) -> Option<UpdateHint> {
    [
        ("url_info_about", program.url_info_about.as_deref()),
        ("help_link", program.help_link.as_deref()),
    ]
    .into_iter()
    .find_map(|(source, value)| {
        let url = value?.trim();
        let lower = url.to_ascii_lowercase();
        if (lower.starts_with("https://") || lower.starts_with("http://"))
            && !url.chars().any(char::is_whitespace)
        {
            Some(UpdateHint {
                url: url.to_string(),
                source: source.to_string(),
                message: "仅提供厂商页面入口，请手动确认版本和更新内容。".to_string(),
            })
        } else {
            None
        }
    })
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_path_key(value: &str) -> String {
    value.trim().replace('/', "\\").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{analyze_programs, HealthStatus, StartupImpact};
    use crate::modules::lister::models::{InstallSource, InstalledProgram, SlowAppInfo};
    use crate::modules::startup::models::{
        StartupItem, StartupLocator, StartupScope, StartupSource,
    };
    use std::collections::HashMap;

    #[test]
    fn health_report_explains_missing_uninstaller_and_location() {
        let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        let report = analyze_programs(&[program], &[], &HashMap::new(), Vec::new());
        let item = report.programs.first().expect("health item should exist");

        assert_eq!(report.review_count, 1);
        assert_eq!(item.status, HealthStatus::Review);
        assert!(item
            .findings
            .iter()
            .any(|finding| finding.code == "missing_uninstaller"));
        assert!(item
            .findings
            .iter()
            .any(|finding| finding.code == "missing_install_location"));
    }

    #[test]
    fn health_report_marks_duplicate_startup_and_safe_update_hint() {
        let mut first = InstalledProgram::new("Demo App".to_string(), InstallSource::Registry);
        first.id = "demo-1".to_string();
        first.uninstall_string = Some("demo-uninstall.exe".to_string());
        first.install_location = Some(r"C:\Demo".to_string());
        first.url_info_about = Some("https://example.com/demo".to_string());
        let mut second = first.clone();
        second.id = "demo-2".to_string();
        let mut startup = StartupItem::new(
            "Demo App",
            StartupSource::RegistryRun,
            StartupScope::User,
            StartupLocator {
                location: r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Demo".to_string(),
                bucket: None,
            },
        );
        startup.executable_path = Some(r"C:\Demo\demo.exe".to_string());
        startup.command = startup.executable_path.clone();
        let mut usage = HashMap::new();
        usage.insert(
            "demo-1".to_string(),
            SlowAppInfo {
                last_used: Some("2026-08-10T00:00:00Z".to_string()),
                times_used: Some(3),
                ..SlowAppInfo::default()
            },
        );

        let report = analyze_programs(&[first, second], &[startup], &usage, Vec::new());
        let item = report.programs.first().expect("health item should exist");
        assert_eq!(item.duplicate_count, 2);
        assert_eq!(item.startup_impact, StartupImpact::Low);
        assert_eq!(item.startup_entry_count, 1);
        assert_eq!(item.last_used.as_deref(), Some("2026-08-10T00:00:00Z"));
        assert_eq!(
            item.update_hint.as_ref().map(|hint| hint.url.as_str()),
            Some("https://example.com/demo")
        );
    }
}
