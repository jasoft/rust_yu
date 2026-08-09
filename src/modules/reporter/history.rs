use super::html::generate_html_report;
use super::models::UninstallerReport;
use crate::application::uninstall::UninstallJob;
use crate::modules::common::error::UninstallerError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const REPORTS_DIR_NAME: &str = "reports";
const EXPORTS_DIR_NAME: &str = "exports";
const STORAGE_DIR_ENV: &str = "RUST_YU_STORAGE_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportExportFormat {
    Json,
    Html,
    Text,
}

impl ReportExportFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "html" | "htm" => Some(Self::Html),
            "text" | "txt" => Some(Self::Text),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Html => "html",
            Self::Text => "txt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportExport {
    pub path: String,
    pub format: ReportExportFormat,
    pub report_id: String,
}

pub fn reports_dir() -> Result<PathBuf, UninstallerError> {
    Ok(application_storage_root()?.join(REPORTS_DIR_NAME))
}

pub fn exports_dir() -> Result<PathBuf, UninstallerError> {
    Ok(application_storage_root()?.join(EXPORTS_DIR_NAME))
}

fn application_storage_root() -> Result<PathBuf, UninstallerError> {
    if let Ok(override_dir) = std::env::var(STORAGE_DIR_ENV) {
        if !override_dir.trim().is_empty() {
            return Ok(PathBuf::from(override_dir));
        }
    }
    dirs::data_local_dir()
        .map(|path| path.join("rust-yu"))
        .ok_or_else(|| UninstallerError::Other("无法确定本机应用数据目录".to_string()))
}

pub fn save_job_report(job: &UninstallJob) -> Result<UninstallerReport, UninstallerError> {
    let report = UninstallerReport::from_job(job);
    let directory = reports_dir()?;
    ensure_safe_directory(&directory)?;
    validate_report_id(&report.id)?;
    write_report_files(&directory, &report).map(|_| report)
}

pub fn list_reports() -> Result<Vec<UninstallerReport>, UninstallerError> {
    let directory = reports_dir()?;
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_safe_directory(&directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    }

    let mut reports = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        ensure_regular_file(&path)?;
        let content = fs::read_to_string(&path).map_err(|error| {
            UninstallerError::Other(format!("读取报告 {} 失败: {error}", path.display()))
        })?;
        let report = serde_json::from_str::<UninstallerReport>(&content).map_err(|error| {
            UninstallerError::Other(format!("解析报告 {} 失败: {error}", path.display()))
        })?;
        reports.push(report);
    }
    reports.sort_by(|left, right| right.generated_at.cmp(&left.generated_at));
    Ok(reports)
}

pub fn load_report(report_id: &str) -> Result<UninstallerReport, UninstallerError> {
    let directory = reports_dir()?;
    validate_safe_directory(&directory)?;
    let path = report_file_path(report_id, "json")?;
    ensure_regular_file(&path)?;
    let content = fs::read_to_string(&path).map_err(|error| {
        UninstallerError::Other(format!("读取报告 {} 失败: {error}", path.display()))
    })?;
    serde_json::from_str(&content)
        .map_err(|error| UninstallerError::Other(format!("解析报告失败: {error}")))
}

pub fn export_report(
    report_id: &str,
    format: ReportExportFormat,
) -> Result<ReportExport, UninstallerError> {
    let report = load_report(report_id)?;
    let directory = exports_dir()?;
    ensure_safe_directory(&directory)?;
    let path = directory.join(format!(
        "rust-yu-report-{}.{}",
        report.id,
        format.extension()
    ));
    let content = match format {
        ReportExportFormat::Json => serde_json::to_string_pretty(&report)
            .map_err(|error| UninstallerError::Serde(error.to_string()))?,
        ReportExportFormat::Html => generate_html_report(&report)?,
        ReportExportFormat::Text => generate_text_report(&report),
    };
    write_atomic(&path, content.as_bytes())?;
    Ok(ReportExport {
        path: path.to_string_lossy().to_string(),
        format,
        report_id: report.id,
    })
}

pub fn delete_report(report_id: &str) -> Result<bool, UninstallerError> {
    validate_report_id(report_id)?;
    let directory = reports_dir()?;
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_safe_directory(&directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    }
    let mut deleted = false;
    for extension in ["json", "html", "txt"] {
        let path = report_file_path(report_id, extension)?;
        if !path.exists() {
            continue;
        }
        ensure_regular_file(&path)?;
        fs::remove_file(path)?;
        deleted = true;
    }
    Ok(deleted)
}

pub fn report_file_path(report_id: &str, extension: &str) -> Result<PathBuf, UninstallerError> {
    validate_report_id(report_id)?;
    if !matches!(extension, "json" | "html" | "txt") {
        return Err(UninstallerError::Other("报告格式不受支持".to_string()));
    }
    Ok(reports_dir()?.join(format!("{report_id}.{extension}")))
}

fn write_report_files(
    directory: &Path,
    report: &UninstallerReport,
) -> Result<(), UninstallerError> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    write_atomic(
        &directory.join(format!("{}.json", report.id)),
        json.as_bytes(),
    )?;
    let html = generate_html_report(report)?;
    write_atomic(
        &directory.join(format!("{}.html", report.id)),
        html.as_bytes(),
    )?;
    let text = generate_text_report(report);
    write_atomic(
        &directory.join(format!("{}.txt", report.id)),
        text.as_bytes(),
    )
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), UninstallerError> {
    let parent = path
        .parent()
        .ok_or_else(|| UninstallerError::Other("报告路径缺少父目录".to_string()))?;
    ensure_safe_directory(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(UninstallerError::Other(format!(
                "拒绝覆盖符号链接报告: {}",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(UninstallerError::Other(format!(
                "报告目标不是普通文件: {}",
                path.display()
            )));
        }
    }
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("report"),
        Uuid::new_v4()
    ));
    fs::write(&temporary, content)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let destination_is_regular_file = fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            .unwrap_or(false);
        if destination_is_regular_file {
            fs::remove_file(path)?;
            if let Err(retry_error) = fs::rename(&temporary, path) {
                let _ = fs::remove_file(&temporary);
                return Err(UninstallerError::FileSystem(retry_error));
            }
        } else {
            let _ = fs::remove_file(&temporary);
            return Err(UninstallerError::FileSystem(error));
        }
    }
    Ok(())
}

fn ensure_safe_directory(directory: &Path) -> Result<(), UninstallerError> {
    fs::create_dir_all(directory)?;
    validate_safe_directory(directory)
}

fn validate_safe_directory(directory: &Path) -> Result<(), UninstallerError> {
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() {
        return Err(UninstallerError::Other(format!(
            "报告目录不能是符号链接: {}",
            directory.display()
        )));
    }
    if !metadata.is_dir() {
        return Err(UninstallerError::Other(format!(
            "报告目录不是文件夹: {}",
            directory.display()
        )));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> Result<(), UninstallerError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        UninstallerError::Other(format!("读取报告 {} 失败: {error}", path.display()))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(UninstallerError::Other(format!(
            "拒绝读取符号链接报告: {}",
            path.display()
        )));
    }
    if !metadata.is_file() {
        return Err(UninstallerError::Other(format!(
            "报告不是普通文件: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_report_id(report_id: &str) -> Result<(), UninstallerError> {
    if report_id.is_empty()
        || report_id.len() > 100
        || !report_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(UninstallerError::Other("报告标识无效".to_string()));
    }
    Ok(())
}

fn generate_text_report(report: &UninstallerReport) -> String {
    let mut output = format!(
        "Rust Yu 卸载报告\n程序: {}\n报告 ID: {}\n生成时间: {}\n状态: {}\n发现项目: {}\n成功处理: {}\n失败项目: {}\n释放空间: {}\n",
        report.program_name,
        report.id,
        report.generated_at.to_rfc3339(),
        if report.success { "成功" } else { "失败或部分完成" },
        report.traces_found.len(),
        report.traces_removed.iter().filter(|item| item.success).count(),
        report.traces_removed.iter().filter(|item| !item.success).count(),
        report.total_size_freed,
    );
    if !report.warnings.is_empty() {
        output.push_str("\n警告:\n");
        for warning in &report.warnings {
            output.push_str("- ");
            output.push_str(warning);
            output.push('\n');
        }
    }
    output.push_str("\n处理项目:\n");
    for result in &report.traces_removed {
        output.push_str(if result.success {
            "[成功] "
        } else {
            "[失败] "
        });
        output.push_str(&result.path);
        if let Some(error) = &result.error {
            output.push_str(" | ");
            output.push_str(error);
        }
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{generate_text_report, validate_report_id};
    use crate::modules::reporter::models::UninstallerReport;

    #[test]
    fn report_ids_reject_path_traversal() {
        assert!(validate_report_id("..\\secret").is_err());
        assert!(validate_report_id("valid-report_1").is_ok());
    }

    #[test]
    fn text_report_includes_failure_details() {
        let report = UninstallerReport::new("Demo".to_string());
        let text = generate_text_report(&report);
        assert!(text.contains("Demo"));
        assert!(text.contains("处理项目"));
    }
}
